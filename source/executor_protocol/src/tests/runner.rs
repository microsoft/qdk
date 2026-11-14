// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running shots on a chain: the records each shot gets, the run's
//! refusals, the failures that end it, and the checks of each `execute`.

mod boundaries;
mod wait;

use std::any::type_name;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::{Duration, Instant};

use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, EndsBelow, FailsOnce, FromThread, Later, Log, Misshape, Misshapen,
    Panics, PanicsEnding, PanicsInDrop, PanicsWhen, Profiler, Program, Queue, Renamed, Splitter,
    Spread, Trap, Values, YieldNow,
};
use crate::{
    Adapter, Capabilities, ChainPart, Drive, DriveError, Driver, Execute, ExecuteRefusal, Executor,
    Failure, InstructionFault, InstructionSet, Needs, OpcodeMap, ReadoutFailure, ReadoutValue,
    RunError, RunRefusal, Runner, Seed, ShapeFault, ShotConditions, ShotId, ShotSet,
};

const SEED: u64 = 7;

/// How long a test waits for a thread before it fails, rather than hangs.
const PATIENT: Duration = Duration::from_secs(20);

/// The runner of the chain `composed` gave, which must compose.
fn runner<D: Drive, E: Execute>(
    composed: Result<Runner<D, E>, crate::CompositionFaults>,
) -> Runner<D, E> {
    let Ok(runner) = composed else {
        panic!("the chain composes");
    };
    runner
}

/// [`CoinCopy`] on bits alone.
fn coin_copy(log: &Log) -> Runner<CoinCopy, Bits> {
    runner(Driver::new(CoinCopy::new()) >> Executor::new(Bits::new(log)))
}

/// The records of `shots` on [`CoinCopy`] on bits alone.
fn plain(shots: &ShotSet) -> Vec<Values> {
    coin_copy(&Log::default())
        .run(shots, SEED)
        .expect("the run succeeds")
}

/// The failure that ended a run.
fn failed<T: std::fmt::Debug>(result: Result<T, RunError>) -> DriveError {
    match result {
        Err(RunError::Failed(error)) => error,
        other => panic!("a failed run, not {other:?}"),
    }
}

/// The refusal of a run.
fn refused<T: std::fmt::Debug>(result: Result<T, RunError>) -> RunRefusal {
    match result {
        Err(RunError::Refused(refusal)) => refusal,
        other => panic!("a refused run, not {other:?}"),
    }
}

// ---------------------------------------------------------------- records

/// A run of a chain, as a closure.
type Run<'a> = Box<dyn FnMut(&ShotSet) -> Vec<Values> + 'a>;

/// A shot's values on a seed depend on nothing else: the same alone,
/// beside others, in another order, or in a subset, with a noise too.
#[test]
fn a_shots_records_do_not_depend_on_the_other_shots_of_its_run() {
    let log = Log::default();
    let all = ShotSet::ids(0..16);
    let mut runners: [Run<'_>; 2] = [
        Box::new({
            let mut runner = coin_copy(&log);
            move |shots| runner.run(shots, SEED).expect("the run succeeds")
        }),
        Box::new({
            let mut runner = runner(
                Driver::new(CoinCopy::new())
                    >> Adapter::new(BitFlip::new(300, &log))
                    >> Executor::new(Bits::new(&log)),
            );
            move |shots| runner.run(shots, SEED).expect("the run succeeds")
        }),
    ];
    for run in &mut runners {
        let together = run(&all);
        for (i, id) in (0..16).enumerate() {
            assert_eq!(
                run(&ShotSet::ids([id])),
                [together[i].clone()],
                "shot {id} alone"
            );
        }
        let mut reversed = run(&ShotSet::ids((0..16).rev()));
        reversed.reverse();
        assert_eq!(reversed, together, "in reverse");
        let subset = run(&ShotSet::ids([3, 9, 12]));
        assert_eq!(
            subset,
            [3, 9, 12].map(|i| together[i].clone()),
            "as a subset"
        );
    }
}

/// A program flipping qubit 1, reading 0 and 1, flipping 0 and reading it:
/// its values in the order it read them, through every chain.
#[test]
fn a_shots_values_are_in_the_order_it_read_them() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let text = [("x", 1), ("mz", 0), ("mz", 1), ("x", 0), ("mz", 0)];
    let program = || Driver::new(Program::new(&text));
    let bits = || Executor::new(Bits::new(&log));
    let shots = ShotSet::ids(0..4);
    let expected = vec![
        (
            vec![ReadoutValue::Zero, ReadoutValue::One, ReadoutValue::One],
            ShotConditions::NONE,
        );
        4
    ];
    let runs = [
        runner(program() >> bits()).run(&shots, SEED),
        runner(program() >> Adapter::new(Profiler::new(&counted, &log)) >> bits())
            .run(&shots, SEED),
        runner(program() >> Adapter::new(Splitter::default()) >> bits()).run(&shots, SEED),
        runner(program() >> Adapter::new(BitFlip::new(0, &log)) >> bits()).run(&shots, SEED),
        runner(program() >> Adapter::new(Spread::new(3, &log)) >> bits()).run(&shots, SEED),
    ];
    for (chain, run) in runs.into_iter().enumerate() {
        assert_eq!(run.expect("the run succeeds"), expected, "chain {chain}");
    }
}

/// A noise of 0 flips nothing: its records are those without it.
#[test]
fn a_noise_that_never_flips_changes_no_record() {
    let log = Log::default();
    let shots = ShotSet::ids(0..64);
    let mut noisy = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(BitFlip::new(0, &log))
            >> Executor::new(Bits::new(&log)),
    );
    assert_eq!(
        noisy.run(&shots, SEED).expect("the run succeeds"),
        plain(&shots)
    );
}

/// A noise flipping 30% of the `x`s: the coins are those without it, from
/// the same seed, and some copies differ from their coin.
#[test]
fn a_noise_draws_from_its_own_stream_and_leaves_the_coins() {
    let log = Log::default();
    let shots = ShotSet::ids(0..64);
    let mut noisy = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(BitFlip::new(300, &log))
            >> Executor::new(Bits::new(&log)),
    );
    let records = noisy.run(&shots, SEED).expect("the run succeeds");
    let coins = |records: &[Values]| records.iter().map(|r| r[0]).collect::<Vec<_>>();
    assert_eq!(coins(&records), coins(&plain(&shots)));
    assert!(
        records.iter().any(|record| record[0] != record[1]),
        "a copy was flipped"
    );
}

/// A profiler counts the instructions and changes no record; a splitter
/// sends each instruction alone and changes none either.
#[test]
fn an_adapter_that_only_observes_or_splits_changes_no_record() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let shots = ShotSet::ids(0..32);
    let mut profiled = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(Profiler::new(&counted, &log))
            >> Executor::new(Bits::new(&log)),
    );
    assert_eq!(
        profiled.run(&shots, SEED).expect("the run succeeds"),
        plain(&shots)
    );
    assert_eq!(counted.get(), 5, "the toss's 2, the heads' 2, the tails' 1");
    let mut split = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(Splitter::default())
            >> Executor::new(Bits::new(&log)),
    );
    assert_eq!(
        split.run(&shots, SEED).expect("the run succeeds"),
        plain(&shots)
    );
}

/// A seeded run repeats, on the same runner or another; two runs with no
/// seed differ.
#[test]
fn a_seed_fixes_the_records_and_no_seed_draws_new_ones() {
    let log = Log::default();
    let shots = ShotSet::ids(0..64);
    let mut runner = coin_copy(&log);
    let first = runner.run(&shots, SEED).expect("the run succeeds");
    assert_eq!(runner.run(&shots, SEED).expect("the run succeeds"), first);
    assert_eq!(plain(&shots), first);
    let drawn = runner.run(&shots, None).expect("the run succeeds");
    let again = runner.run(&shots, None).expect("the run succeeds");
    assert_ne!(drawn, again, "two drawn seeds, 64 coins each");
}

/// An executor that loses every value and rejects every shot: each shot's
/// record says so, through the adapters.
#[test]
fn a_lost_value_and_a_rejected_shot_reach_the_driver() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let mut runner = runner(
        Driver::new(Program::new(&[("mz", 0)]))
            >> Adapter::new(BitFlip::new(0, &log))
            >> Adapter::new(Profiler::new(&counted, &log))
            >> Executor::new(Bits::new(&log).losing()),
    );
    let records = runner
        .run(&ShotSet::ids(0..3), SEED)
        .expect("the run succeeds");
    assert_eq!(
        records,
        vec![
            (
                vec![ReadoutValue::Missing(ReadoutFailure::Loss)],
                ShotConditions::REJECTED
            );
            3
        ]
    );
}

/// A driver that ends its shots: every part below it is told, and holds
/// nothing of them after, wherever the brackets go; the runner's end of
/// the same shots after the run then does nothing.
#[test]
fn a_drivers_end_of_its_shots_reaches_every_part_below() {
    let log = Log::default();
    let shots = ShotSet::ids(0..8);
    let left = runner(
        (Driver::new(CoinCopy::new()) >> Adapter::new(BitFlip::new(300, &log)))
            >> Executor::new(Bits::new(&log)),
    );
    let right = runner(
        Driver::new(CoinCopy::new())
            >> (Adapter::new(BitFlip::new(300, &log)) >> Executor::new(Bits::new(&log))),
    );
    let mut left = left;
    let mut right = right;
    left.run(&shots, SEED).expect("the run succeeds");
    right.run(&shots, SEED).expect("the run succeeds");
    assert_eq!(log.count("BitFlip end_shots, holding 0"), 4);
    assert_eq!(log.count("Bits end_shots, holding 0"), 4);
    assert_eq!(
        log.entries()
            .iter()
            .filter(|entry| entry.contains("end_shots"))
            .count(),
        8
    );
}

/// A program that never ends its shots, over a noise, a trap that never
/// springs and bits, in each of the five bracketings of four parts: once
/// its run gives its outputs, every part has ended every shot of it, once,
/// by the runner, top to bottom.
#[test]
fn a_successful_runs_shots_are_ended_on_every_part_even_when_the_driver_ends_none() {
    fn ended<D: Drive, E: Execute>(mut runner: Runner<D, E>, log: &Log) -> Vec<String> {
        let records = runner
            .run(&ShotSet::ids(0..3), SEED)
            .expect("the run succeeds");
        assert_eq!(records.len(), 3, "every shot ran");
        log.entries()
            .into_iter()
            .filter(|entry| entry.contains("end_shots"))
            .collect()
    }
    let program = || Driver::new(Program::new(&[("mz", 0)]).not_ending());
    let noise = |log: &Log| Adapter::new(BitFlip::new(300, log));
    let trap = |log: &Log| Adapter::new(Trap::on(99).logging(log));
    let bits = |log: &Log| Executor::new(Bits::new(log));
    let logs: [Log; 5] = Default::default();
    let ends = [
        ended(
            runner(((program() >> noise(&logs[0])) >> trap(&logs[0])) >> bits(&logs[0])),
            &logs[0],
        ),
        ended(
            runner((program() >> noise(&logs[1])) >> (trap(&logs[1]) >> bits(&logs[1]))),
            &logs[1],
        ),
        ended(
            runner(program() >> (noise(&logs[2]) >> (trap(&logs[2]) >> bits(&logs[2])))),
            &logs[2],
        ),
        ended(
            runner(program() >> ((noise(&logs[3]) >> trap(&logs[3])) >> bits(&logs[3]))),
            &logs[3],
        ),
        ended(
            runner((program() >> (noise(&logs[4]) >> trap(&logs[4]))) >> bits(&logs[4])),
            &logs[4],
        ),
    ];
    for (bracketing, ended) in ends.into_iter().enumerate() {
        assert_eq!(
            ended,
            [
                "BitFlip end_shots, holding 0",
                "Trap end_shots",
                "Bits end_shots, holding 0"
            ],
            "bracketing {bracketing}"
        );
    }
}

// ---------------------------------------------------------------- async

/// Polls `future` with a waker that does nothing, until it is ready: an
/// executor of futures other than `run`'s.
fn spin<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let deadline = Instant::now() + PATIENT;
    loop {
        if let Poll::Ready(output) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return output;
        }
        assert!(Instant::now() < deadline, "never ready");
        thread::yield_now();
    }
}

/// A run awaited by another executor of futures, through each kind of
/// `>>`, with executors that answer later or from a thread: the records
/// `run` gives.
#[test]
fn a_run_awaited_elsewhere_gives_the_records_of_run() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let shots = ShotSet::ids(0..16);
    let expected = plain(&shots);
    let from_thread = || FromThread::new(Bits::new(&log), Duration::from_millis(5));
    let mut alone = runner(Driver::new(CoinCopy::new()) >> Executor::new(from_thread()));
    assert_eq!(
        spin(alone.run_async(&shots, SEED)).expect("the run succeeds"),
        expected,
        "alone"
    );
    let mut below = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(Profiler::new(&counted, &log))
            >> Adapter::new(Splitter::default())
            >> Executor::new(Later(Bits::new(&log))),
    );
    assert_eq!(
        spin(below.run_async(&shots, SEED)).expect("the run succeeds"),
        expected,
        "adapters with the executor"
    );
    let mut above = runner(
        (Driver::new(CoinCopy::new()) >> Adapter::new(BitFlip::new(0, &log)))
            >> Executor::new(from_thread()),
    );
    assert_eq!(
        spin(above.run_async(&shots, SEED)).expect("the run succeeds"),
        expected,
        "an adapter with the driver"
    );
}

/// The tasks of a test's runtime on one thread, and the thread to unpark.
struct Tasks {
    ready: Mutex<VecDeque<usize>>,
    thread: thread::Thread,
}

/// The waker of task `index`: queues it, and unparks the thread.
struct TaskWaker {
    index: usize,
    tasks: Arc<Tasks>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.tasks
            .ready
            .lock()
            .expect("no panic while held")
            .push_back(self.index);
        self.tasks.thread.unpark();
    }
}

/// A runtime on one thread, for several tasks: it polls the tasks woken,
/// in the order they were woken, and parks only when none is, logging
/// "park", as a single-threaded runtime does.
fn run_tasks(mut tasks: Vec<Pin<Box<dyn Future<Output = ()> + '_>>>, log: &RefCell<Vec<String>>) {
    let shared = Arc::new(Tasks {
        ready: Mutex::new((0..tasks.len()).collect()),
        thread: thread::current(),
    });
    let wakers: Vec<Waker> = (0..tasks.len())
        .map(|index| {
            Waker::from(Arc::new(TaskWaker {
                index,
                tasks: Arc::clone(&shared),
            }))
        })
        .collect();
    let mut done = vec![false; tasks.len()];
    let deadline = Instant::now() + PATIENT;
    while done.contains(&false) {
        let next = shared
            .ready
            .lock()
            .expect("no panic while held")
            .pop_front();
        let Some(i) = next else {
            log.borrow_mut().push("park".to_owned());
            assert!(Instant::now() < deadline, "never woken");
            thread::park_timeout(Duration::from_millis(100));
            continue;
        };
        if !done[i]
            && tasks[i]
                .as_mut()
                .poll(&mut Context::from_waker(&wakers[i]))
                .is_ready()
        {
            done[i] = true;
        }
    }
}

/// Runs `f` on a thread of its own, and fails the test if it does not end
/// soon: a lost wake, or a blocked thread, would hang it.
fn ends_soon<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || sender.send(f()).expect("the test waits"));
    receiver
        .recv_timeout(PATIENT)
        .expect("it never ended: a wake was lost, or a thread blocked")
}

/// Task A awaits a run whose executor answers only after task B's fifth
/// step, B on the same thread: B runs while A awaits, and the thread does
/// not sleep while B has work.
#[test]
fn an_awaited_run_leaves_its_thread_to_other_tasks() {
    ends_soon(two_tasks_on_one_thread);
}

fn two_tasks_on_one_thread() {
    let bits_log = Log::default();
    let (go, gate) = mpsc::channel::<()>();
    let mut runner = runner(
        Driver::new(CoinCopy::new())
            >> Executor::new(FromThread::new(Bits::new(&bits_log), Duration::ZERO).gated(gate)),
    );
    let shots = ShotSet::ids(0..4);
    let expected = plain(&shots);
    let log = RefCell::new(Vec::new());
    let a = async {
        let records = runner
            .run_async(&shots, SEED)
            .await
            .expect("the run succeeds");
        assert_eq!(records, expected);
        log.borrow_mut().push("A done".to_owned());
    };
    let b = async {
        for step in 0..10 {
            log.borrow_mut().push(format!("B {step}"));
            if step == 4 {
                go.send(()).expect("the executor waits for the gate");
            }
            YieldNow::default().await;
        }
        log.borrow_mut().push("B done".to_owned());
    };
    run_tasks(vec![Box::pin(a), Box::pin(b)], &log);
    let log = log.into_inner();
    let at = |entry: &str| {
        log.iter()
            .position(|e| e == entry)
            .unwrap_or_else(|| panic!("no {entry}: {log:?}"))
    };
    assert_eq!(
        log[0], "B 0",
        "A was polled first, and was pending: {log:?}"
    );
    assert!(at("B 4") < at("A done"), "B ran while A awaited: {log:?}");
    assert!(
        !log[..at("B done")].iter().any(|e| e == "park"),
        "the thread slept while B had work: {log:?}"
    );
}

/// Coin copies under a noise, over bits that answer later, in both
/// bracketings: a run whose future is dropped after its first `execute`
/// leaves no shot's state in any part, so the rerun with the same seed
/// gives the records of a new chain.
#[test]
fn a_rerun_after_a_dropped_run_gives_a_new_chains_records() {
    /// The records of `runner`'s second run of `shots`, its first dropped
    /// unfinished.
    fn rerun<D: Drive<Output = Values>, E: Execute>(
        mut runner: Runner<D, E>,
        shots: &ShotSet,
        log: &Log,
    ) -> Vec<Values> {
        {
            let mut run = std::pin::pin!(runner.run_async(shots, SEED));
            for _ in 0..2 {
                let poll = run.as_mut().poll(&mut Context::from_waker(Waker::noop()));
                assert!(poll.is_pending(), "the run is unfinished");
            }
        }
        assert_eq!(log.count("Bits execute"), 1, "one execute before the drop");
        runner.run(shots, SEED).expect("the rerun succeeds")
    }
    let shots = ShotSet::ids(0..16);
    let new_chain = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(BitFlip::new(300, &Log::default()))
            >> Executor::new(Bits::new(&Log::default())),
    )
    .run(&shots, SEED)
    .expect("the run succeeds");
    let log = Log::default();
    let left = runner(
        (Driver::new(CoinCopy::new()) >> Adapter::new(BitFlip::new(300, &log)))
            >> Executor::new(Later(Bits::new(&log))),
    );
    assert_eq!(
        rerun(left, &shots, &log),
        new_chain,
        "the noise with the driver"
    );
    let log = Log::default();
    let right = runner(
        Driver::new(CoinCopy::new())
            >> (Adapter::new(BitFlip::new(300, &log)) >> Executor::new(Later(Bits::new(&log)))),
    );
    assert_eq!(
        rerun(right, &shots, &log),
        new_chain,
        "the noise with the bits"
    );
}

/// Whether `T` may move to another thread; it compiles only if so.
fn is_send<T: Send>(_: &T) {}

/// Parts that are `Send`, and whose futures are, in three bracketings:
/// the future of `run_async` may move to another thread.
#[test]
fn a_runs_future_is_send_when_every_part_and_its_future_are() {
    let shots = ShotSet::ids(0..2);
    let driver = || Driver::new(Panics(PanicsWhen::Polled));
    let splitter = || Adapter::new(Splitter::default());
    let executor = || Executor::new(Panics(PanicsWhen::Polled));
    let mut joined = runner(driver() >> executor());
    is_send(&joined.run_async(&shots, SEED));
    let mut left = runner((driver() >> splitter()) >> executor());
    is_send(&left.run_async(&shots, SEED));
    let mut right = runner(driver() >> (splitter() >> executor()));
    is_send(&right.run_async(&shots, SEED));
}

// ---------------------------------------------------------------- refusals

/// An executor running 4 shots at most: a run of 5 is refused before any
/// part is asked, and a run of 4 runs.
#[test]
fn a_run_of_more_shots_than_the_executor_runs_is_refused() {
    let log = Log::default();
    let offer = Capabilities::unlimited().max_qubits(64).max_shots(4);
    let mut runner =
        runner(Driver::new(CoinCopy::new()) >> Executor::new(Bits::new(&log).offering(offer)));
    let refusal = refused(runner.run(&ShotSet::ids(0..5), SEED));
    assert!(
        matches!(refusal, RunRefusal::TooManyShots { asked: 5, most: 4 }),
        "{refusal:?}"
    );
    assert_eq!(log.count("Bits ready"), 0, "no part asked");
    runner
        .run(&ShotSet::ids(0..4), SEED)
        .expect("as many shots as the executor runs");
}

/// Bits under a job queue of 4 shots at most, bracketed both ways: the
/// queue's limit holds for each, so a run of 5 is refused.
#[test]
fn where_the_brackets_go_changes_no_run_refusal() {
    let log = Log::default();
    let d = || Driver::new(CoinCopy::new());
    let queue = || Adapter::new(Queue::new(4, &log));
    let bits = || Executor::new(Bits::new(&log));
    let shots = ShotSet::ids(0..5);
    let left = refused(runner((d() >> queue()) >> bits()).run(&shots, SEED));
    let right = refused(runner(d() >> (queue() >> bits())).run(&shots, SEED));
    for (refusal, brackets) in [(left, "left"), (right, "right")] {
        assert!(
            matches!(refusal, RunRefusal::TooManyShots { asked: 5, most: 4 }),
            "{brackets}: {refusal:?}"
        );
    }
}

/// An executor that cannot be seeded: a seeded run is refused before any
/// part is asked, and a run with no seed runs.
#[test]
fn a_seeded_run_on_a_chain_not_seedable_is_refused() {
    let log = Log::default();
    let offer = Capabilities::unlimited().max_qubits(64).not_seedable();
    let mut runner =
        runner(Driver::new(CoinCopy::new()) >> Executor::new(Bits::new(&log).offering(offer)));
    let refusal = refused(runner.run(&ShotSet::ids(0..4), SEED));
    assert!(matches!(refusal, RunRefusal::NotSeedable), "{refusal:?}");
    assert_eq!(log.count("Bits ready"), 0, "no part asked");
    runner
        .run(&ShotSet::ids(0..4), None)
        .expect("a run with no seed");
}

/// Busy bits under a profiler: the run is refused with the bits' reason,
/// and nothing above them is asked or runs.
#[test]
fn a_part_that_cannot_take_a_run_refuses_it() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let mut runner = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(Profiler::new(&counted, &log))
            >> Executor::new(Bits::new(&log).busy("calibrating")),
    );
    let refusal = refused(runner.run(&ShotSet::ids(0..4), SEED));
    assert!(
        matches!(&refusal, RunRefusal::Unavailable(why) if why.to_string() == "calibrating"),
        "{refusal:?}"
    );
    assert_eq!(log.count("Profiler ready"), 0);
    assert_eq!(counted.get(), 0, "nothing ran");
}

/// Each part is asked whether it can take a run, the lowest first,
/// wherever the brackets go.
#[test]
fn the_parts_are_asked_bottom_up() {
    fn asked(log: &Log) -> Vec<String> {
        log.entries()
            .into_iter()
            .filter(|entry| entry.ends_with("ready"))
            .collect()
    }
    let shots = ShotSet::ids(0..2);
    let program = || Driver::new(Program::new(&[("mz", 0)]));
    let counted = Rc::new(Cell::new(0));
    let log = Log::default();
    let profiler = Adapter::new(Profiler::new(&counted, &log));
    runner((program() >> profiler) >> Executor::new(Bits::new(&log)))
        .run(&shots, SEED)
        .expect("the run succeeds");
    assert_eq!(
        asked(&log),
        ["Bits ready", "Profiler ready"],
        "the profiler with the driver"
    );
    let log = Log::default();
    let profiler = Adapter::new(Profiler::new(&counted, &log));
    runner(program() >> (profiler >> Executor::new(Bits::new(&log))))
        .run(&shots, SEED)
        .expect("the run succeeds");
    assert_eq!(
        asked(&log),
        ["Bits ready", "Profiler ready"],
        "the profiler with the bits"
    );
}

/// An instruction on a qubit beyond the program's needs: the `execute` is
/// refused, typed, naming the batch's first shot, and nothing of it runs.
#[test]
fn an_instruction_beyond_the_needs_is_refused_before_it_runs() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(Program::new(&[("mz", 0), ("x", 5)]).needing(Needs::qubits(2)))
            >> Executor::new(Bits::new(&log)),
    );
    let error = failed(runner.run(&ShotSet::ids([4, 2]), SEED));
    assert_eq!(error.shot, ShotId::new(4));
    let Failure::Refused(refusal) = error.failure else {
        panic!("a refusal, not {:?}", error.failure);
    };
    assert_eq!(
        refusal,
        ExecuteRefusal::Instruction {
            shot: ShotId::new(4),
            batch: 0,
            at: 1,
            fault: InstructionFault::BlockBeyondNeeds {
                mnemonic: "x".to_owned(),
                block: 5,
                needs: 2,
            },
        }
    );
    assert_eq!(log.count("Bits execute"), 0, "nothing ran");
}

/// A batch longer than the executor takes: refused, typed, and nothing of
/// it runs.
#[test]
fn a_batch_longer_than_the_executor_takes_is_refused_before_it_runs() {
    let log = Log::default();
    let offer = Capabilities::unlimited().max_qubits(64).max_instructions(1);
    let mut runner = runner(
        Driver::new(Program::new(&[("x", 0), ("mz", 0)]))
            >> Executor::new(Bits::new(&log).offering(offer)),
    );
    let error = failed(runner.run(&ShotSet::ids(0..2), SEED));
    assert!(
        matches!(
            error.failure,
            Failure::Refused(ExecuteRefusal::TooManyInstructions {
                batch: 0,
                max: 1,
                got: 2,
                ..
            })
        ),
        "{error:?}"
    );
    assert_eq!(log.count("Bits execute"), 0, "nothing ran");
}

/// Two batches in one `execute`, shots 4 and 2 measuring qubit 0, shots 9
/// and 7 flipping qubit 5 beyond the program's 2 qubits: the second batch
/// is refused, named by its first shot, 9, and neither batch runs.
#[test]
fn a_later_batchs_refusal_names_it_and_runs_no_batch() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(Program::halves(&[("mz", 0)], &[("x", 5)]).needing(Needs::qubits(2)))
            >> Executor::new(Bits::new(&log)),
    );
    let error = failed(runner.run(&ShotSet::ids([4, 2, 9, 7]), SEED));
    assert_eq!(error.shot, ShotId::new(9));
    let Failure::Refused(refusal) = error.failure else {
        panic!("a refusal, not {:?}", error.failure);
    };
    assert_eq!(
        refusal,
        ExecuteRefusal::Instruction {
            shot: ShotId::new(9),
            batch: 1,
            at: 0,
            fault: InstructionFault::BlockBeyondNeeds {
                mnemonic: "x".to_owned(),
                block: 5,
                needs: 2,
            },
        }
    );
    assert_eq!(log.count("Bits execute"), 0, "neither batch ran");
}

/// A driver sending no batch: an empty answer, and the executor is not
/// called.
struct SendsNothing;

impl Drive for SendsNothing {
    type Output = usize;

    fn sends(&self) -> &InstructionSet {
        static SET: std::sync::OnceLock<InstructionSet> = std::sync::OnceLock::new();
        SET.get_or_init(crate::tests::fake_parts::CoinGates::set)
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    fn needs(&self) -> Needs {
        Needs::qubits(1)
    }

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<usize>, DriveError> {
        let answer = below.execute(&[], seed).await?;
        Ok(vec![answer.len(); shots.len()])
    }
}

/// An `execute` of no batches answers no readouts, without reaching the
/// executor.
#[test]
fn an_execute_of_no_batches_answers_none_without_the_executor() {
    let log = Log::default();
    let mut runner = runner(Driver::new(SendsNothing) >> Executor::new(Bits::new(&log)));
    let answer = runner
        .run(&ShotSet::ids(0..2), SEED)
        .expect("the run succeeds");
    assert_eq!(answer, [0, 0]);
    assert_eq!(log.count("Bits execute"), 0);
}

// ---------------------------------------------------------------- failures

/// A program trapping on shot 5, run on 8 shots: the run fails naming shot
/// 5 and the trap, and gives no records.
#[test]
fn a_failing_shot_ends_the_run_naming_it() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(CoinCopy::new()) >> Adapter::new(Trap::on(5)) >> Executor::new(Bits::new(&log)),
    );
    let error = failed(runner.run(&ShotSet::ids(0..8), SEED));
    assert_eq!(error.shot, ShotId::new(5));
    assert!(
        matches!(&error.failure, Failure::Part(why) if why.to_string() == "the program trapped"),
        "{error:?}"
    );
    assert_eq!(error.to_string(), "shot 5 failed: the program trapped");
}

/// Coin copies under a noise flipping 30% of the `x`s, above an adapter
/// failing the run's second `execute`, in both bracketings: the failed run
/// leaves no shot's state in any part, so the rerun with the same seed gives
/// the records of a new chain.
#[test]
fn a_rerun_after_a_failed_run_gives_a_new_chains_records() {
    /// The records of `runner`'s second run of `shots`, its first failing.
    fn rerun<D: Drive<Output = Values>, E: Execute>(
        mut runner: Runner<D, E>,
        shots: &ShotSet,
    ) -> Vec<Values> {
        failed(runner.run(shots, SEED));
        runner.run(shots, SEED).expect("the rerun succeeds")
    }
    let shots = ShotSet::ids(0..16);
    let log = Log::default();
    let new_chain = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(BitFlip::new(300, &log))
            >> Executor::new(Bits::new(&log)),
    )
    .run(&shots, SEED)
    .expect("the run succeeds");
    let left = runner(
        ((Driver::new(CoinCopy::new()) >> Adapter::new(BitFlip::new(300, &log)))
            >> Adapter::new(FailsOnce::new()))
            >> Executor::new(Bits::new(&log)),
    );
    let right = runner(
        Driver::new(CoinCopy::new())
            >> (Adapter::new(BitFlip::new(300, &log))
                >> (Adapter::new(FailsOnce::new()) >> Executor::new(Bits::new(&log)))),
    );
    assert_eq!(rerun(left, &shots), new_chain, "the noise with the driver");
    assert_eq!(rerun(right, &shots), new_chain, "the noise with the bits");
}

/// An executor answering a readouts more than the batches: a failure of
/// the first batch's first shot, with the shape fault.
#[test]
fn an_answer_of_more_readouts_than_batches_is_a_failure() {
    assert_eq!(
        shape_fault(Misshape::ExtraReadouts),
        ShapeFault::Batches {
            expected: 1,
            got: 2
        }
    );
}

/// An executor answering a row fewer than the batch's shots: a failure.
#[test]
fn an_answer_missing_a_shots_row_is_a_failure() {
    assert_eq!(
        shape_fault(Misshape::MissingRow),
        ShapeFault::Rows {
            batch: 0,
            expected: 3,
            got: 2
        }
    );
}

/// An executor answering a value more than the batch's readout slots: a
/// failure.
#[test]
fn an_answer_wider_than_the_batchs_readouts_is_a_failure() {
    assert_eq!(
        shape_fault(Misshape::ExtraValue),
        ShapeFault::Slots {
            batch: 0,
            expected: 1,
            got: 2
        }
    );
}

/// Two batches in one `execute`, shots 4 and 2 then 9 and 7, each measuring
/// qubit 0, on an executor misshaping the second batch's readouts: the run
/// fails naming that batch and its first shot, 9.
#[test]
fn a_later_batchs_misshapen_readouts_name_it() {
    for (misshape, expected) in [
        (
            Misshape::MissingRow,
            ShapeFault::Rows {
                batch: 1,
                expected: 2,
                got: 1,
            },
        ),
        (
            Misshape::ExtraValue,
            ShapeFault::Slots {
                batch: 1,
                expected: 1,
                got: 2,
            },
        ),
    ] {
        let mut runner = runner(
            Driver::new(Program::halves(&[("mz", 0)], &[("mz", 0)]))
                >> Executor::new(Misshapen(misshape)),
        );
        let error = failed(runner.run(&ShotSet::ids([4, 2, 9, 7]), SEED));
        assert_eq!(error.shot, ShotId::new(9), "{expected}");
        let Failure::Part(fault) = error.failure else {
            panic!("the part's error, not {:?}", error.failure);
        };
        assert_eq!(fault.downcast_ref::<ShapeFault>(), Some(&expected));
    }
}

/// The shape fault of a run of 3 shots, first shot 6, of one `mz` on a
/// misshapen executor, checking the failure names shot 6.
fn shape_fault(misshape: Misshape) -> ShapeFault {
    let mut runner =
        runner(Driver::new(Program::new(&[("mz", 0)])) >> Executor::new(Misshapen(misshape)));
    let error = failed(runner.run(&ShotSet::ids([6, 1, 2]), SEED));
    assert_eq!(error.shot, ShotId::new(6));
    let Failure::Part(fault) = error.failure else {
        panic!("the part's error, not {:?}", error.failure);
    };
    *fault.downcast_ref::<ShapeFault>().expect("a shape fault")
}

/// Coin copies under a noise, above a trap on shot 5, over bits, in each
/// bracketing: a run's shots are ended on the noise, then the trap, then
/// the bits, both when the run succeeds, by the driver and then by the
/// runner, and when it fails, by the runner.
#[test]
fn a_runs_shots_are_ended_top_to_bottom_even_when_it_fails() {
    /// The ends logged by a run that succeeds, then by one that fails.
    type Ends = [Vec<String>; 2];
    /// The shots ended by `runner`'s run of shots 0 to 3, which succeeds,
    /// then by its run of shots 0 to 7, which fails on shot 5, as logged
    /// to `log`.
    fn ends<D: Drive, E: Execute>(mut runner: Runner<D, E>, log: &Log) -> Ends {
        let ended = |log: &Log| -> Vec<String> {
            log.entries()
                .into_iter()
                .filter(|entry| entry.contains("end_shots"))
                .collect()
        };
        runner
            .run(&ShotSet::ids(0..4), SEED)
            .expect("a run without shot 5 succeeds");
        let succeeded = ended(log);
        failed(
            runner
                .run(&ShotSet::ids(0..8), SEED)
                .map(|records| records.len()),
        );
        let failed_run = ended(log)[succeeded.len()..].to_vec();
        [succeeded, failed_run]
    }
    let bracketings: [fn(&Log) -> Ends; 5] = [
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> Adapter::new(BitFlip::new(300, log))
                >> Adapter::new(Trap::on(5).logging(log))
                >> Executor::new(Bits::new(log));
            ends(runner(chain), log)
        },
        |log| {
            let chain = (Driver::new(CoinCopy::new())
                >> (Adapter::new(BitFlip::new(300, log))
                    >> Adapter::new(Trap::on(5).logging(log))))
                >> Executor::new(Bits::new(log));
            ends(runner(chain), log)
        },
        |log| {
            let chain = (Driver::new(CoinCopy::new()) >> Adapter::new(BitFlip::new(300, log)))
                >> (Adapter::new(Trap::on(5).logging(log)) >> Executor::new(Bits::new(log)));
            ends(runner(chain), log)
        },
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> ((Adapter::new(BitFlip::new(300, log))
                    >> Adapter::new(Trap::on(5).logging(log)))
                    >> Executor::new(Bits::new(log)));
            ends(runner(chain), log)
        },
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> (Adapter::new(BitFlip::new(300, log))
                    >> (Adapter::new(Trap::on(5).logging(log)) >> Executor::new(Bits::new(log))));
            ends(runner(chain), log)
        },
    ];
    let top_to_bottom = [
        "BitFlip end_shots, holding 0",
        "Trap end_shots",
        "Bits end_shots, holding 0",
    ];
    for (bracketing, chain) in bracketings.into_iter().enumerate() {
        let [succeeded, failed_run] = chain(&Log::default());
        assert_eq!(
            succeeded,
            [top_to_bottom, top_to_bottom].concat(),
            "bracketing {bracketing}, ended by the driver, then the runner"
        );
        assert_eq!(failed_run, top_to_bottom, "bracketing {bracketing}, failed");
    }
}

/// Coin copies under an adapter that panics in `end_shots`, over a noise,
/// over bits, in a run with no other fault, in each bracketing: the
/// driver's end of its shots fails the run, the runner ends them again on
/// every part, each part beside the one that panics is ended both times,
/// and `run` then raises the panic.
#[test]
fn a_panic_ending_shots_ends_every_other_part_then_leaves_the_run() {
    /// Runs `runner` on 8 shots, and checks it as above.
    fn ends_every_other_part<D: Drive, E: Execute>(mut runner: Runner<D, E>, log: &Log) {
        let run = catch_unwind(AssertUnwindSafe(|| {
            runner
                .run(&ShotSet::ids(0..8), SEED)
                .map(|records| records.len())
        }));
        let panic = run.expect_err("the run raises the panic");
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"end_shots broke"));
        assert_eq!(log.count("BitFlip end_shots, holding 0"), 2);
        assert_eq!(log.count("Bits end_shots, holding 0"), 2);
    }
    let bracketings: [fn(&Log); 5] = [
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> Adapter::new(PanicsEnding::new(log))
                >> Adapter::new(BitFlip::new(300, log))
                >> Executor::new(Bits::new(log));
            ends_every_other_part(runner(chain), log);
        },
        |log| {
            let chain = (Driver::new(CoinCopy::new())
                >> (Adapter::new(PanicsEnding::new(log)) >> Adapter::new(BitFlip::new(300, log))))
                >> Executor::new(Bits::new(log));
            ends_every_other_part(runner(chain), log);
        },
        |log| {
            let chain = (Driver::new(CoinCopy::new()) >> Adapter::new(PanicsEnding::new(log)))
                >> (Adapter::new(BitFlip::new(300, log)) >> Executor::new(Bits::new(log)));
            ends_every_other_part(runner(chain), log);
        },
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> ((Adapter::new(PanicsEnding::new(log)) >> Adapter::new(BitFlip::new(300, log)))
                    >> Executor::new(Bits::new(log)));
            ends_every_other_part(runner(chain), log);
        },
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> (Adapter::new(PanicsEnding::new(log))
                    >> (Adapter::new(BitFlip::new(300, log)) >> Executor::new(Bits::new(log))));
            ends_every_other_part(runner(chain), log);
        },
    ];
    for chain in bracketings {
        chain(&Log::default());
    }
}

/// Coin copies over an adapter whose first `end_shots` panics, over a
/// noise, over bits, in each bracketing: the driver ends each shot on the
/// chain below, so the panic unwinds through the driver's `drive`, and the
/// run fails naming the driver, part 0, not the adapter.
#[test]
fn a_panic_ending_shots_the_driver_ends_is_named_as_the_driver() {
    /// Runs `runner` on 8 shots, and checks it as above.
    fn names_the_driver<D: Drive, E: Execute>(mut runner: Runner<D, E>) {
        let error = failed(
            runner
                .run(&ShotSet::ids(0..8), SEED)
                .map(|records| records.len()),
        );
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if *part == ChainPart::new(0, type_name::<CoinCopy>())
                        && message == "end_shots broke"
            ),
            "{error:?}"
        );
    }
    let bracketings: [fn(&Log); 5] = [
        |log| {
            names_the_driver(runner(
                Driver::new(CoinCopy::new())
                    >> Adapter::new(PanicsEnding::once(log))
                    >> Adapter::new(BitFlip::new(300, log))
                    >> Executor::new(Bits::new(log)),
            ));
        },
        |log| {
            names_the_driver(runner(
                (Driver::new(CoinCopy::new())
                    >> (Adapter::new(PanicsEnding::once(log))
                        >> Adapter::new(BitFlip::new(300, log))))
                    >> Executor::new(Bits::new(log)),
            ));
        },
        |log| {
            names_the_driver(runner(
                (Driver::new(CoinCopy::new()) >> Adapter::new(PanicsEnding::once(log)))
                    >> (Adapter::new(BitFlip::new(300, log)) >> Executor::new(Bits::new(log))),
            ));
        },
        |log| {
            names_the_driver(runner(
                Driver::new(CoinCopy::new())
                    >> ((Adapter::new(PanicsEnding::once(log))
                        >> Adapter::new(BitFlip::new(300, log)))
                        >> Executor::new(Bits::new(log))),
            ));
        },
        |log| {
            names_the_driver(runner(
                Driver::new(CoinCopy::new())
                    >> (Adapter::new(PanicsEnding::once(log))
                        >> (Adapter::new(BitFlip::new(300, log)) >> Executor::new(Bits::new(log)))),
            ));
        },
    ];
    for chain in bracketings {
        chain(&Log::default());
    }
}

/// A program that never ends its shots, over an adapter that ends its
/// shots below inside `adapt`, over an adapter whose first `end_shots`
/// panics, over bits, in each bracketing: the panic unwinds through the
/// ending adapter's `adapt`, so the run fails naming that adapter, part 1,
/// not the driver, and not the part that panicked.
#[test]
fn a_panic_ending_shots_an_adapter_ends_is_named_as_that_adapter() {
    /// Runs `runner` on 8 shots, and checks it as above.
    fn names_the_adapter<D: Drive, E: Execute>(mut runner: Runner<D, E>) {
        let error = failed(
            runner
                .run(&ShotSet::ids(0..8), SEED)
                .map(|records| records.len()),
        );
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if *part == ChainPart::new(1, type_name::<EndsBelow>())
                        && message == "end_shots broke"
            ),
            "{error:?}"
        );
    }
    let program = || Driver::new(Program::new(&[("mz", 0)]).not_ending());
    let ends = |log: &Log| Adapter::new(EndsBelow::new(log));
    let panics = |log: &Log| Adapter::new(PanicsEnding::once(log));
    let bits = |log: &Log| Executor::new(Bits::new(log));
    let logs: [Log; 5] = Default::default();
    names_the_adapter(runner(
        ((program() >> ends(&logs[0])) >> panics(&logs[0])) >> bits(&logs[0]),
    ));
    names_the_adapter(runner(
        (program() >> ends(&logs[1])) >> (panics(&logs[1]) >> bits(&logs[1])),
    ));
    names_the_adapter(runner(
        program() >> (ends(&logs[2]) >> (panics(&logs[2]) >> bits(&logs[2]))),
    ));
    names_the_adapter(runner(
        program() >> ((ends(&logs[3]) >> panics(&logs[3])) >> bits(&logs[3])),
    ));
    names_the_adapter(runner(
        (program() >> (ends(&logs[4]) >> panics(&logs[4]))) >> bits(&logs[4]),
    ));
}

/// A program that never ends its shots, over an adapter whose first
/// `end_shots` panics, over a noise, over bits, in each bracketing: the
/// runner's end is the only one, so the panic is raised again from `run`,
/// once the noise and the bits are ended, and the outputs are lost, where
/// the same panic in the driver's end fails the run naming the driver.
#[test]
fn a_panic_ending_shots_the_runner_ends_is_raised_from_the_run() {
    /// Runs `runner` on 8 shots, and checks it as above.
    fn raised<D: Drive, E: Execute>(mut runner: Runner<D, E>, log: &Log) {
        let run = catch_unwind(AssertUnwindSafe(|| {
            runner
                .run(&ShotSet::ids(0..8), SEED)
                .map(|records| records.len())
        }));
        let panic = run.expect_err("the run raises the panic");
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"end_shots broke"));
        assert_eq!(log.count("BitFlip end_shots, holding 0"), 1);
        assert_eq!(log.count("Bits end_shots, holding 0"), 1);
    }
    let program = || Driver::new(Program::new(&[("mz", 0)]).not_ending());
    let breaks = |log: &Log| Adapter::new(PanicsEnding::once(log));
    let noise = |log: &Log| Adapter::new(BitFlip::new(300, log));
    let bits = |log: &Log| Executor::new(Bits::new(log));
    let logs: [Log; 5] = Default::default();
    raised(
        runner(((program() >> breaks(&logs[0])) >> noise(&logs[0])) >> bits(&logs[0])),
        &logs[0],
    );
    raised(
        runner((program() >> breaks(&logs[1])) >> (noise(&logs[1]) >> bits(&logs[1]))),
        &logs[1],
    );
    raised(
        runner(program() >> (breaks(&logs[2]) >> (noise(&logs[2]) >> bits(&logs[2])))),
        &logs[2],
    );
    raised(
        runner(program() >> ((breaks(&logs[3]) >> noise(&logs[3])) >> bits(&logs[3]))),
        &logs[3],
    );
    raised(
        runner((program() >> (breaks(&logs[4]) >> noise(&logs[4]))) >> bits(&logs[4])),
        &logs[4],
    );
}

/// Coin copies under a noise, over an adapter that panics in `end_shots`,
/// over bits that answer later: a run whose future is dropped by the
/// caller's own panic still ends its shots on every other part, and drops
/// the end's panic rather than raising a second one, which would abort.
#[test]
fn a_run_dropped_by_a_panic_ends_its_shots_and_keeps_that_panic() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(BitFlip::new(300, &log))
            >> Adapter::new(PanicsEnding::new(&log))
            >> Executor::new(Later(Bits::new(&log))),
    );
    let shots = ShotSet::ids(0..8);
    let caught = catch_unwind(AssertUnwindSafe(|| {
        let mut run = std::pin::pin!(runner.run_async(&shots, SEED));
        let poll = run.as_mut().poll(&mut Context::from_waker(Waker::noop()));
        assert!(poll.is_pending(), "the run is unfinished");
        panic!("the caller broke");
    }));
    let panic = caught.expect_err("the caller's panic leaves the closure");
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"the caller broke"));
    assert_eq!(log.count("BitFlip end_shots, holding 0"), 1);
    assert_eq!(log.count("Bits end_shots, holding 0"), 1);
}

/// Coin copies over an adapter that panics in `end_shots`, over bits that
/// answer later: a run whose future is dropped unfinished, outside any
/// panic, ends its shots on the bits, then raises the end's panic from
/// the drop.
#[test]
fn a_run_dropped_unfinished_raises_an_ends_panic_from_the_drop() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(PanicsEnding::new(&log))
            >> Executor::new(Later(Bits::new(&log))),
    );
    let shots = ShotSet::ids(0..8);
    let mut run = Box::pin(runner.run_async(&shots, SEED));
    let poll = run.as_mut().poll(&mut Context::from_waker(Waker::noop()));
    assert!(poll.is_pending(), "the run is unfinished");
    let dropped = catch_unwind(AssertUnwindSafe(|| drop(run)));
    let panic = dropped.expect_err("the drop raises the end's panic");
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"end_shots broke"));
    assert_eq!(log.count("Bits end_shots, holding 0"), 1);
}

/// Coin copies over two adapters that panic in `end_shots`, "first" above
/// "second", over bits, in each bracketing: `run` raises the first panic,
/// top to bottom, and the bits are still ended.
#[test]
fn of_two_panics_ending_shots_the_first_is_raised() {
    /// Runs `runner` on 8 shots, and checks it as above.
    fn raises_the_first<D: Drive, E: Execute>(mut runner: Runner<D, E>, log: &Log) {
        let run = catch_unwind(AssertUnwindSafe(|| {
            runner
                .run(&ShotSet::ids(0..8), SEED)
                .map(|records| records.len())
        }));
        let panic = run.expect_err("the run raises a panic");
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"first"));
        assert_eq!(log.count("Bits end_shots, holding 0"), 2);
    }
    let bracketings: [fn(&Log); 5] = [
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> Adapter::new(PanicsEnding::saying("first", log))
                >> Adapter::new(PanicsEnding::saying("second", log))
                >> Executor::new(Bits::new(log));
            raises_the_first(runner(chain), log);
        },
        |log| {
            let chain = (Driver::new(CoinCopy::new())
                >> (Adapter::new(PanicsEnding::saying("first", log))
                    >> Adapter::new(PanicsEnding::saying("second", log))))
                >> Executor::new(Bits::new(log));
            raises_the_first(runner(chain), log);
        },
        |log| {
            let chain = (Driver::new(CoinCopy::new())
                >> Adapter::new(PanicsEnding::saying("first", log)))
                >> (Adapter::new(PanicsEnding::saying("second", log))
                    >> Executor::new(Bits::new(log)));
            raises_the_first(runner(chain), log);
        },
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> ((Adapter::new(PanicsEnding::saying("first", log))
                    >> Adapter::new(PanicsEnding::saying("second", log)))
                    >> Executor::new(Bits::new(log)));
            raises_the_first(runner(chain), log);
        },
        |log| {
            let chain = Driver::new(CoinCopy::new())
                >> (Adapter::new(PanicsEnding::saying("first", log))
                    >> (Adapter::new(PanicsEnding::saying("second", log))
                        >> Executor::new(Bits::new(log))));
            raises_the_first(runner(chain), log);
        },
    ];
    for chain in bracketings {
        chain(&Log::default());
    }
}

/// An executor that panics, while making its future or while it is
/// polled: the run fails naming the batch's first shot, the executor at
/// its position and the message.
#[test]
fn an_executors_panic_is_a_failure_of_its_batch() {
    for (when, message) in [
        (
            PanicsWhen::MakingItsFuture,
            "the part broke while making its future",
        ),
        (PanicsWhen::Polled, "the part broke while it was polled"),
    ] {
        let mut runner =
            runner(Driver::new(Program::new(&[("mz", 0)])) >> Executor::new(Panics(when)));
        let error = failed(runner.run(&ShotSet::ids([3, 1]), SEED));
        assert_eq!(error.shot, ShotId::new(3));
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(m) }
                    if *part == ChainPart::new(1, type_name::<Panics>()) && m == message
            ),
            "{error:?}"
        );
    }
}

/// A driver that panics: the run fails naming its first shot and the
/// driver, part 0.
#[test]
fn a_drivers_panic_is_a_failure_of_the_run() {
    for (when, message) in [
        (
            PanicsWhen::MakingItsFuture,
            "the part broke while making its future",
        ),
        (PanicsWhen::Polled, "the part broke while it was polled"),
    ] {
        let log = Log::default();
        let mut runner = runner(Driver::new(Panics(when)) >> Executor::new(Bits::new(&log)));
        let error = failed(runner.run(&ShotSet::ids([9, 1]), SEED));
        assert_eq!(error.shot, ShotId::new(9));
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(m) }
                    if *part == ChainPart::new(0, type_name::<Panics>()) && m == message
            ),
            "{error:?}"
        );
    }
}

/// A driver panicking while polled, composed with a profiler: the run
/// fails naming the driver, not the profiler.
#[test]
fn a_composed_drivers_panic_names_the_driver() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let mut runner = runner(
        (Driver::new(Panics(PanicsWhen::Polled)) >> Adapter::new(Profiler::new(&counted, &log)))
            >> Executor::new(Bits::new(&log)),
    );
    let error = failed(runner.run(&ShotSet::ids([9, 1]), SEED));
    assert!(
        matches!(
            &error.failure,
            Failure::Panicked { part, .. }
                if *part == ChainPart::new(0, type_name::<Panics>())
        ),
        "{error:?}"
    );
}

/// A driver and an executor that panic, each named as a part built at
/// run time is, by what it was built from: the failure names the part by
/// that name, at its position.
#[test]
fn a_panic_names_the_part_by_its_name() {
    let log = Log::default();
    let shots = ShotSet::ids([9, 1]);
    let mut executor = runner(
        Driver::new(Program::new(&[("mz", 0)]))
            >> Executor::new(Renamed::new(Panics(PanicsWhen::Polled), "qodec runtime")),
    );
    let mut driver = runner(
        Driver::new(Renamed::new(Panics(PanicsWhen::Polled), "h2 program"))
            >> Executor::new(Bits::new(&log)),
    );
    for (runner_name, error, position, name) in [
        (
            "executor",
            failed(executor.run(&shots, SEED)),
            1,
            "qodec runtime",
        ),
        ("driver", failed(driver.run(&shots, SEED)), 0, "h2 program"),
    ] {
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, .. } if *part == ChainPart::new(position, name)
            ),
            "{runner_name}: {error:?}"
        );
        assert!(
            error
                .to_string()
                .contains(&format!("`{name}` (part {position}) panicked")),
            "{runner_name}: {error}"
        );
    }
}

/// A driver that panics, over a noise, over bits, bracketed both ways:
/// the failed run ends its shots on the noise and the bits.
#[test]
fn a_run_failed_by_a_drivers_panic_ends_its_shots() {
    /// Runs `runner` on 2 shots, which fails, and checks it as above.
    fn ends_its_shots<D: Drive, E: Execute>(mut runner: Runner<D, E>, log: &Log) {
        failed(
            runner
                .run(&ShotSet::ids([9, 1]), SEED)
                .map(|records| records.len()),
        );
        assert_eq!(log.count("BitFlip end_shots, holding 0"), 1);
        assert_eq!(log.count("Bits end_shots, holding 0"), 1);
    }
    for when in [PanicsWhen::MakingItsFuture, PanicsWhen::Polled] {
        let log = Log::default();
        ends_its_shots(
            runner(
                (Driver::new(Panics(when)) >> Adapter::new(BitFlip::new(0, &log)))
                    >> Executor::new(Bits::new(&log)),
            ),
            &log,
        );
        let log = Log::default();
        ends_its_shots(
            runner(
                Driver::new(Panics(when))
                    >> (Adapter::new(BitFlip::new(0, &log)) >> Executor::new(Bits::new(&log))),
            ),
            &log,
        );
    }
}

/// A program, over a profiler, over a noise, over bits, each a part whose
/// futures may panic once dropped.
type DropChain = (
    Driver<PanicsInDrop<Program>>,
    Adapter<PanicsInDrop<Profiler>>,
    Adapter<PanicsInDrop<BitFlip>>,
    Executor<PanicsInDrop<Bits>>,
);

/// The parts of a [`DropChain`], whose part at position `armed` panics in
/// its futures' destructor.
fn drop_chain(armed: usize, log: &Log) -> DropChain {
    let counted = Rc::new(Cell::new(0));
    (
        Driver::new(PanicsInDrop::new(Program::new(&[("mz", 0)]), armed == 0)),
        Adapter::new(PanicsInDrop::new(Profiler::new(&counted, log), armed == 1)),
        Adapter::new(PanicsInDrop::new(BitFlip::new(0, log), armed == 2)),
        Executor::new(PanicsInDrop::new(Bits::new(log), armed == 3)),
    )
}

/// A program, over a profiler, over a noise, over bits, in each
/// bracketing, one of them panicking in the destructor of each future it
/// returns, once its answer is given: the run fails naming that part, at
/// its position, with its message, as for a panic while it is polled, and
/// its shots are ended on the bits.
#[test]
fn a_panic_dropping_a_parts_future_is_a_failure_naming_that_part() {
    /// Runs `runner` on 2 shots, and checks it as above.
    fn names<D: Drive, E: Execute>(mut runner: Runner<D, E>, named: &ChainPart, log: &Log) {
        let error = failed(
            runner
                .run(&ShotSet::ids([9, 1]), SEED)
                .map(|records| records.len()),
        );
        assert_eq!(error.shot, ShotId::new(9));
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if part == named && message == "the part's future broke when dropped"
            ),
            "{named}: {error:?}"
        );
        assert!(log.count("Bits end_shots, holding 0") >= 1, "{named}");
    }
    let bracketings: [fn(usize, &ChainPart); 5] = [
        |armed, named| {
            let log = Log::default();
            let (d, a, b, e) = drop_chain(armed, &log);
            names(runner(((d >> a) >> b) >> e), named, &log);
        },
        |armed, named| {
            let log = Log::default();
            let (d, a, b, e) = drop_chain(armed, &log);
            names(runner((d >> a) >> (b >> e)), named, &log);
        },
        |armed, named| {
            let log = Log::default();
            let (d, a, b, e) = drop_chain(armed, &log);
            names(runner(d >> (a >> (b >> e))), named, &log);
        },
        |armed, named| {
            let log = Log::default();
            let (d, a, b, e) = drop_chain(armed, &log);
            names(runner(d >> ((a >> b) >> e)), named, &log);
        },
        |armed, named| {
            let log = Log::default();
            let (d, a, b, e) = drop_chain(armed, &log);
            names(runner((d >> (a >> b)) >> e), named, &log);
        },
    ];
    let parts = [
        type_name::<Program>(),
        type_name::<Profiler>(),
        type_name::<BitFlip>(),
        type_name::<Bits>(),
    ];
    for (armed, name) in parts.into_iter().enumerate() {
        for chain in bracketings {
            chain(armed, &ChainPart::new(armed, name));
        }
    }
}

/// A program over an executor that panics while polled, and again when
/// its future is dropped: the run fails naming the executor, with the
/// message of its first panic, and the second is dropped.
#[test]
fn a_part_panicking_polled_then_dropped_is_named_with_its_first_panic() {
    let mut runner = runner(
        Driver::new(Program::new(&[("mz", 0)]))
            >> Executor::new(PanicsInDrop::new(Panics(PanicsWhen::Polled), true)),
    );
    let error = failed(runner.run(&ShotSet::ids([3, 1]), SEED));
    assert!(
        matches!(
            &error.failure,
            Failure::Panicked { part, message: Some(message) }
                if *part == ChainPart::new(1, type_name::<Panics>())
                    && message == "the part broke while it was polled"
        ),
        "{error:?}"
    );
}

/// Coin copies over bits that answer later, and whose futures panic once
/// dropped: a run whose future is dropped while the bits' future is
/// pending raises that panic again from the drop, and its shots are ended
/// on the bits.
#[test]
fn a_panic_dropping_a_parts_future_with_the_run_is_raised_from_the_drop() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(CoinCopy::new())
            >> Executor::new(PanicsInDrop::new(Later(Bits::new(&log)), true)),
    );
    let shots = ShotSet::ids(0..4);
    let dropped = catch_unwind(AssertUnwindSafe(|| {
        let mut run = std::pin::pin!(runner.run_async(&shots, SEED));
        let poll = run.as_mut().poll(&mut Context::from_waker(Waker::noop()));
        assert!(poll.is_pending(), "the bits answer later");
    }));
    let panic = dropped.expect_err("the drop raises the panic");
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"the part's future broke when dropped")
    );
    assert_eq!(log.count("Bits end_shots, holding 0"), 1);
}
