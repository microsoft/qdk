//! What the Qodec layer's blocking costs an async caller (Q65): the wall
//! time of a run, awaited, over a device that answers each call after a
//! fixed latency, from threads of its own; and the time the thread that
//! polls the run is held inside a poll, against the time it is free.
//!
//! The device is the tests' coins, placed a shot per thread
//! (`.run_on(OwnThread).shots(OnePerShot)`), each shot's logic sleeping
//! for the latency before it reads: so a call on several shots answers
//! after one latency, the shots' threads sleeping together. Four ways to
//! run, on c422 and c4c6:
//!
//! - `layer`: `static_driver() >> qodec >> profiler() >> call_counter() >>
//!   device`, awaited on the caller's thread;
//! - `thread`: the same chain built and run on a thread of its own, the
//!   caller awaiting its end (today's way to free an async caller);
//! - `shots`: the same chain, a chain per shot, each built and run on a
//!   thread of its own, the caller awaiting their ends: the shots' reads
//!   overlap, each a call on its shot alone;
//! - `bare`: `static_driver() >> profiler() >> call_counter() >> device`,
//!   awaited, running the bottom batches that shot 0 sends in the stack, as
//!   a static program on all the shots (no Qodec layer).
//!
//! It first checks that each way gives, over the slow device, the outcomes
//! it gives over the tests' coins that answer within their call; then
//! times each, best of 3 by wall time, and reports the device calls, the
//! wall time, the time held inside polls, and the polls. The times of
//! `thread` and `shots` include building the chain on its thread, which
//! it reports apart.
//!
//! `cargo run --release -p qodec_demo --example latency [ms...]`
//! (by default 0.1, 1 and 10 ms).

#[path = "../tests/common/mod.rs"]
mod common;

use std::future::{Future, IntoFuture};
use std::pin::{Pin, pin};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use executor_protocol::{
    CallInput, Capabilities, ExecutorLogic, InstructionCall, InstructionSet, MakeState, Needs,
    OnePerShot, OnePerShotPlacedExecutor, Operand, Outcome, Outcomes, OwnThread, ShotContext,
    ShotId, ShotSet, StaticProgram, Value, executor_fn, static_driver,
};
use qodec::Qodec;
use qodec_demo::call_counter::{Calls, call_counter};
use qodec_demo::coin::{coin, coin_device};
use qodec_demo::profiler::profiler;
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::QodecLayer;
use qodec_demo::stack::on_the_stack;

use common::{SEED, bottom, c4c6, c4c6_program, c422, c422_program, sorted};

const REPEATS: usize = 3;

/// One example: its qodec, how many Qodec layers the chain stacks, its
/// program, and the bottom batches its shot 0 sends in the stack.
struct Case {
    name: &'static str,
    qodec: Qodec,
    layers: usize,
    program: Program,
    bare: Vec<Vec<InstructionCall>>,
}

/// One shot's coins, as the tests' coins draw them.
#[derive(Clone)]
struct SlowCoins {
    seed: u64,
    drawn: u64,
}

/// Sleeps for `latency`, then a coin per tag, in call order: the tests'
/// coins ([`coin_device`]), answering later.
fn slow_toss(
    latency: Duration,
) -> impl FnMut(&mut SlowCoins, &CallInput<'_, InstructionCall>, &mut Outcomes<'_>) -> Result<(), String>
+ Clone
+ Send {
    move |coins, input, outcomes| {
        thread::sleep(latency);
        for call in input.instructions() {
            for operand in call.operands.iter() {
                if let Operand::Tag(tag) = *operand {
                    let value = coin(coins.seed, coins.drawn).into();
                    coins.drawn += 1;
                    outcomes.push(Outcome::new(tag, value));
                }
            }
        }
        Ok(())
    }
}

/// The tests' coins of `set`, each shot on a thread of its own, answering
/// each call after `latency`.
fn slow_coins(
    set: Arc<InstructionSet>,
    seed: u64,
    latency: Duration,
) -> OnePerShotPlacedExecutor<
    InstructionCall,
    SlowCoins,
    impl MakeState<SlowCoins> + Clone + Send,
    impl ExecutorLogic<InstructionCall, SlowCoins> + Clone + Send,
    OwnThread,
> {
    let promise = Capabilities::default()
        .with_resumable_shots(true)
        .with_instruction_set(Some(set));
    let make = |shot: ShotContext| {
        Ok(SlowCoins {
            seed: shot.seed(),
            drawn: 0,
        })
    };
    executor_fn(promise, make, slow_toss(latency))
        .seed(seed)
        .run_on(OwnThread)
        .shots(OnePerShot)
}

/// What a run awaited took.
#[derive(Clone, Copy, Debug)]
struct Timing {
    wall: Duration,
    /// The time inside the run's polls.
    held: Duration,
    polls: usize,
}

/// Wakes a thread parked waiting for a run.
struct Unpark(Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// `run`, awaited on this thread as an executor of futures does: polled,
/// the thread parked between polls until woken. Its output, and how long
/// the polls held the thread.
fn awaited<F: IntoFuture>(run: F) -> (F::Output, Timing) {
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut run = pin!(run.into_future());
    let start = Instant::now();
    let (mut held, mut polls) = (Duration::ZERO, 0);
    loop {
        let poll = Instant::now();
        let ready = run.as_mut().poll(&mut context);
        held += poll.elapsed();
        polls += 1;
        if let Poll::Ready(output) = ready {
            let wall = start.elapsed();
            return (output, Timing { wall, held, polls });
        }
        thread::park();
    }
}

/// The end of work on another thread, as a future.
struct Joined<T> {
    shared: Arc<Mutex<(Option<T>, Option<Waker>)>>,
}

impl<T> Future for Joined<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<T> {
        let mut shared = self.shared.lock().expect("the work did not panic");
        match shared.0.take() {
            Some(output) => Poll::Ready(output),
            None => {
                shared.1 = Some(context.waker().clone());
                Poll::Pending
            }
        }
    }
}

/// `work`, run on a thread of its own: its end, as a future.
fn on_a_thread<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Joined<T> {
    let shared = Arc::new(Mutex::new((None, None::<Waker>)));
    let theirs = Arc::clone(&shared);
    thread::spawn(move || {
        let output = work();
        let mut shared = theirs.lock().expect("the caller did not panic");
        shared.0 = Some(output);
        if let Some(waker) = shared.1.take() {
            waker.wake();
        }
    });
    Joined { shared }
}

/// How a case runs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Way {
    Layer,
    Thread,
    ThreadPerShot,
    Bare,
}

const WAYS: [(Way, &str); 4] = [
    (Way::Layer, "layer"),
    (Way::Thread, "thread"),
    (Way::ThreadPerShot, "shots"),
    (Way::Bare, "bare"),
];

/// `$chain` (whose report is `$report`) run on `$program` and `$shots`,
/// awaited, or waited for when `$wait`: its sorted outcomes, the device
/// calls, and the timing (zero when waited for).
macro_rules! go {
    ($chain:expr, $program:expr, $shots:expr, $wait:expr, |$report:pat_param| $calls:expr) => {{
        let run = $chain
            .expect("every boundary matches")
            .run($program, $shots)
            .expect("the job fits the chain");
        let ((outcomes, $report), timing) = if $wait {
            let zero = Timing {
                wall: Duration::ZERO,
                held: Duration::ZERO,
                polls: 0,
            };
            (run.wait(), zero)
        } else {
            awaited(run)
        };
        let calls: Calls = $calls;
        (sorted(outcomes.expect("the run succeeds")), calls, timing)
    }};
}

/// The outcomes of `case`'s layers over `$device` on `$shots`, the device
/// calls, and the timing.
macro_rules! layered {
    ($case:expr, $shots:expr, $wait:expr, $device:expr) => {{
        let case: &Case = $case;
        let layer = |k| {
            QodecLayer::new(&case.qodec, k)
                .expect("the qodec plans")
                .adapter()
        };
        let program = case.program.to_static(Needs::default());
        match case.layers {
            1 => go!(
                static_driver() >> layer(0) >> profiler() >> call_counter() >> $device,
                program,
                $shots,
                $wait,
                |(_, (calls, _))| calls
            ),
            2 => go!(
                static_driver() >> layer(0) >> layer(1) >> profiler() >> call_counter() >> $device,
                program,
                $shots,
                $wait,
                |(_, (calls, _))| calls
            ),
            layers => panic!("{}: {layers} layers", case.name),
        }
    }};
}

/// The outcomes of `case`'s shot-0 bottom batches as a static program
/// over `$device` on `$shots`, the device calls, and the timing.
macro_rules! bare {
    ($case:expr, $shots:expr, $wait:expr, $device:expr) => {{
        let case: &Case = $case;
        let needs = Needs::default().with_instruction_set(Some(bottom(&case.qodec).set.clone()));
        go!(
            static_driver() >> profiler() >> call_counter() >> $device,
            StaticProgram::new(needs, case.bare.clone()),
            $shots,
            $wait,
            |(_, (calls, _))| calls
        )
    }};
}

type Ran = (Vec<(ShotId, u32, Value)>, Calls, Timing);

/// `case` run `way` on `shots`, over the slow coins, awaited.
fn slow(case: &Arc<Case>, way: Way, shots: &ShotSet, latency: Duration) -> Ran {
    let set = || bottom(&case.qodec).set.clone();
    let shots = shots.clone();
    match way {
        Way::Layer => layered!(case, shots, false, slow_coins(set(), SEED, latency)),
        Way::Bare => bare!(case, shots, false, slow_coins(set(), SEED, latency)),
        Way::Thread => {
            let case = Arc::clone(case);
            let (ran, timing) = awaited(on_a_thread(move || {
                let set = bottom(&case.qodec).set.clone();
                layered!(&case, shots, true, slow_coins(set, SEED, latency))
            }));
            (ran.0, ran.1, timing)
        }
        Way::ThreadPerShot => {
            let joined: Vec<_> = (shots.iter())
                .map(|&j| {
                    let case = Arc::clone(case);
                    on_a_thread(move || {
                        let set = bottom(&case.qodec).set.clone();
                        layered!(&case, ShotSet::one(j), true, slow_coins(set, SEED, latency))
                    })
                })
                .collect();
            let (ran, timing) = awaited(async {
                let mut all = Vec::new();
                for shot in joined {
                    all.push(shot.await);
                }
                all
            });
            let calls = Calls(ran.iter().map(|(_, calls, _)| calls.0).sum());
            let mut outcomes: Vec<_> = ran.into_iter().flat_map(|(o, _, _)| o).collect();
            outcomes.sort_unstable_by_key(|&(shot, tag, _)| (shot, tag));
            (outcomes, calls, timing)
        }
    }
}

/// `case` run `way` on `shots`, over the tests' coins, waited for.
fn quick(case: &Case, way: Way, shots: &ShotSet) -> Ran {
    let set = || bottom(&case.qodec).set.clone();
    let shots = shots.clone();
    match way {
        Way::Layer | Way::Thread => layered!(case, shots, true, coin_device(set(), SEED)),
        Way::ThreadPerShot => {
            let mut outcomes = Vec::new();
            let mut calls = Calls(0);
            for &j in shots.iter() {
                let ran = layered!(case, ShotSet::one(j), true, coin_device(set(), SEED));
                outcomes.extend(ran.0);
                calls.0 += ran.1.0;
            }
            let zero = Duration::ZERO;
            let timing = Timing {
                wall: zero,
                held: zero,
                polls: 0,
            };
            (outcomes, calls, timing)
        }
        Way::Bare => bare!(case, shots, true, coin_device(set(), SEED)),
    }
}

/// `batches` with their tags renumbered from 0 over the program, so that
/// no tag repeats.
fn tagged_once(batches: &[Vec<InstructionCall>]) -> Vec<Vec<InstructionCall>> {
    let mut next = 0;
    let mut retag = |operand: &Operand| match *operand {
        Operand::Tag(_) => {
            next += 1;
            Operand::Tag(next - 1)
        }
        other => other,
    };
    (batches.iter())
        .map(|batch| {
            (batch.iter())
                .map(|call| InstructionCall::new(call.opcode, call.operands.iter().map(&mut retag)))
                .collect()
        })
        .collect()
}

fn main() {
    let latencies: Vec<f64> = std::env::args()
        .skip(1)
        .map(|s| s.parse().expect("a latency in ms"))
        .collect();
    let latencies = if latencies.is_empty() {
        vec![0.1, 1.0, 10.0]
    } else {
        latencies
    };
    let case = |name, qodec: Qodec, layers, program: fn(&Qodec) -> Program| {
        let program = program(&qodec);
        let stack = on_the_stack(&qodec, &program, &ShotSet::one(0), SEED).expect("the stack runs");
        let bare = tagged_once(stack.backends[0].1.sent());
        Arc::new(Case {
            name,
            qodec,
            layers,
            program,
            bare,
        })
    };
    let cases = [
        case("c422", c422(), 1, c422_program),
        case("c4c6", c4c6(), 2, c4c6_program),
    ];
    let shot_counts: [ShotId; 2] = [1, 4];

    for case in &cases {
        for &n in &shot_counts {
            let shots = ShotSet::range(0..=n - 1).expect("not empty");
            let (in_one_chain, ..) = quick(case, Way::Layer, &shots);
            let (a_chain_per_shot, ..) = quick(case, Way::ThreadPerShot, &shots);
            assert_eq!(
                a_chain_per_shot, in_one_chain,
                "{}: a shot alone gives other outcomes",
                case.name
            );
            for (way, name) in WAYS {
                let (expected, calls, _) = quick(case, way, &shots);
                assert!(!expected.is_empty(), "{}: the run reads", case.name);
                let (outcomes, slow_calls, _) = slow(case, way, &shots, Duration::from_micros(10));
                assert_eq!(outcomes, expected, "{}: {name}: outcomes differ", case.name);
                assert_eq!(slow_calls, calls, "{}: {name}: calls differ", case.name);
            }
        }
    }

    println!("best of {REPEATS} by wall time; times in ms");
    for case in &cases {
        let build = (0..REPEATS)
            .map(|_| {
                let start = Instant::now();
                for k in 0..case.layers {
                    drop(QodecLayer::new(&case.qodec, k).expect("the qodec plans"));
                }
                start.elapsed()
            })
            .min()
            .expect("a build");
        println!(
            "{}: building its layers takes {:.2} ms (in the times of `thread` and `shots`)",
            case.name,
            build.as_secs_f64() * 1e3
        );
    }
    println!(
        "{:<5} {:>5} {:>8} {:<6} {:>6} {:>9} {:>9} {:>9} {:>6} {:>7} {:>10}",
        "code",
        "shots",
        "latency",
        "way",
        "calls",
        "wall",
        "held",
        "free",
        "held%",
        "polls",
        "wall/call"
    );
    for &ms in &latencies {
        let latency = Duration::from_secs_f64(ms / 1e3);
        for case in &cases {
            for &n in &shot_counts {
                let shots = ShotSet::range(0..=n - 1).expect("not empty");
                for (way, name) in WAYS {
                    let mut best: Option<(Calls, Timing)> = None;
                    for _ in 0..REPEATS {
                        let (_, calls, timing) = slow(case, way, &shots, latency);
                        if best.is_none_or(|(_, b)| timing.wall < b.wall) {
                            best = Some((calls, timing));
                        }
                    }
                    let (calls, t) = best.expect("a run");
                    let ms = |d: Duration| d.as_secs_f64() * 1e3;
                    #[expect(clippy::cast_precision_loss, reason = "a count of calls")]
                    let per_call = ms(t.wall) / calls.0 as f64;
                    println!(
                        "{:<5} {n:>5} {:>8.1} {name:<6} {:>6} {:>9.2} {:>9.2} {:>9.2} {:>5.0}% {:>7} {per_call:>10.3}",
                        case.name,
                        latency.as_secs_f64() * 1e3,
                        calls.0,
                        ms(t.wall),
                        ms(t.held),
                        ms(t.wall.saturating_sub(t.held)),
                        100.0 * t.held.as_secs_f64() / t.wall.as_secs_f64(),
                        t.polls,
                    );
                }
            }
        }
    }
}
