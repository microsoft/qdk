// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running shots on a chain: the records each shot gets, the run's
//! refusals, the failures that end it, and the checks of each `execute`.

mod boundaries;
mod failures;
mod wait;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::{Duration, Instant};

use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, FromThread, Later, Log, Panics, PanicsWhen, Profiler, Program, Queue,
    Restating, Splitter, Spread, Trap, Unseedable, Values, YieldNow,
};
use crate::{
    Adapter, Capabilities, Drive, DriveError, Driver, Execute, ExecuteRefusal, Executor, Failure,
    InstructionFault, InstructionSet, Needs, OpcodeMap, ReadoutFailure, ReadoutValue, RunError,
    RunRefusal, Runner, Seed, ShotConditions, ShotId, ShotSet,
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

/// Bits running 1 shot at most, under a decoder that restates its offer
/// with no shot limit, or with a limit of 4, bracketed both ways: the
/// bits' limit still holds, so a run of 2 is refused before any part is
/// asked or runs.
#[test]
fn an_adapter_cannot_raise_the_most_shots_a_run_may_have() {
    let log = Log::default();
    let d = || Driver::new(CoinCopy::new());
    let offer = Capabilities::unlimited().max_qubits(64).max_shots(1);
    let bits = || Executor::new(Bits::new(&log).offering(offer.clone()));
    let shots = ShotSet::ids(0..2);
    for most in [None, Some(4)] {
        let decoder = || {
            let restating = Restating::new(64, &log);
            Adapter::new(match most {
                Some(most) => restating.most_shots(most),
                None => restating,
            })
        };
        let left = refused(runner((d() >> decoder()) >> bits()).run(&shots, SEED));
        let right = refused(runner(d() >> (decoder() >> bits())).run(&shots, SEED));
        for (refusal, brackets) in [(left, "left"), (right, "right")] {
            assert!(
                matches!(refusal, RunRefusal::TooManyShots { asked: 2, most: 1 }),
                "{most:?}, {brackets}: {refusal:?}"
            );
        }
    }
    assert_eq!(log.count("Bits ready"), 0, "no part asked");
    assert_eq!(log.count("Bits execute"), 0, "nothing ran");
}

/// Bits running 8 shots at most, under a decoder that restates its offer
/// with a limit of 4, bracketed both ways: the decoder's lower limit holds,
/// so a run of 5 is refused before any part is asked, and a run of 4 runs.
#[test]
fn an_adapter_can_lower_the_most_shots_below_the_executors() {
    let log = Log::default();
    let d = || Driver::new(CoinCopy::new());
    let decoder = || Adapter::new(Restating::new(64, &log).most_shots(4));
    let offer = Capabilities::unlimited().max_qubits(64).max_shots(8);
    let bits = || Executor::new(Bits::new(&log).offering(offer.clone()));
    let mut left = runner((d() >> decoder()) >> bits());
    let mut right = runner(d() >> (decoder() >> bits()));
    let five = ShotSet::ids(0..5);
    for (refusal, brackets) in [
        (refused(left.run(&five, SEED)), "left"),
        (refused(right.run(&five, SEED)), "right"),
    ] {
        assert!(
            matches!(refusal, RunRefusal::TooManyShots { asked: 5, most: 4 }),
            "{brackets}: {refusal:?}"
        );
    }
    assert_eq!(log.count("Bits ready"), 0, "no part asked");
    let four = ShotSet::ids(0..4);
    left.run(&four, SEED).expect("left: a run of 4");
    right.run(&four, SEED).expect("right: a run of 4");
}

/// Bits that cannot be seeded, under a decoder that restates its offer as
/// seedable, bracketed both ways: a seeded run of 1 shot is still refused
/// before any part is asked or runs.
#[test]
fn an_adapter_cannot_make_a_chain_seedable() {
    let log = Log::default();
    let d = || Driver::new(CoinCopy::new());
    let decoder = || Adapter::new(Restating::new(64, &log));
    let offer = Capabilities::unlimited().max_qubits(64).not_seedable();
    let bits = || Executor::new(Bits::new(&log).offering(offer.clone()));
    let shots = ShotSet::ids(0..1);
    let left = refused(runner((d() >> decoder()) >> bits()).run(&shots, SEED));
    let right = refused(runner(d() >> (decoder() >> bits())).run(&shots, SEED));
    for (refusal, brackets) in [(left, "left"), (right, "right")] {
        assert!(
            matches!(refusal, RunRefusal::NotSeedable),
            "{brackets}: {refusal:?}"
        );
    }
    assert_eq!(log.count("Bits ready"), 0, "no part asked");
    assert_eq!(log.count("Bits execute"), 0, "nothing ran");
}

/// Bits running 8 shots at most, under a job queue of 4, under a decoder
/// that restates its offer with no shot limit, bracketed both ways: the
/// queue's limit, though neither the executor's nor the one reaching the
/// driver, holds, so a run of 5 is refused before any part is asked, and a
/// run of 4 runs.
#[test]
fn an_adapter_in_the_middle_lowers_the_most_shots_under_one_restating_its_offer() {
    let log = Log::default();
    let d = || Driver::new(CoinCopy::new());
    let decoder = || Adapter::new(Restating::new(64, &log));
    let queue = || Adapter::new(Queue::new(4, &log));
    let offer = Capabilities::unlimited().max_qubits(64).max_shots(8);
    let bits = || Executor::new(Bits::new(&log).offering(offer.clone()));
    let mut left = runner(((d() >> decoder()) >> queue()) >> bits());
    let mut right = runner(d() >> (decoder() >> (queue() >> bits())));
    let five = ShotSet::ids(0..5);
    for (refusal, brackets) in [
        (refused(left.run(&five, SEED)), "left"),
        (refused(right.run(&five, SEED)), "right"),
    ] {
        assert!(
            matches!(refusal, RunRefusal::TooManyShots { asked: 5, most: 4 }),
            "{brackets}: {refusal:?}"
        );
    }
    assert_eq!(log.count("Bits ready"), 0, "no part asked");
    let four = ShotSet::ids(0..4);
    left.run(&four, SEED).expect("left: a run of 4");
    right.run(&four, SEED).expect("right: a run of 4");
}

/// Seedable bits under a link that cannot be seeded, under a decoder that
/// restates its offer as seedable, bracketed both ways: the link's
/// seedability, though neither the executor's nor the one reaching the
/// driver, holds, so a seeded run is refused before any part is asked, and
/// a run with no seed runs.
#[test]
fn an_adapter_in_the_middle_makes_a_chain_not_seedable_under_one_restating_its_offer() {
    let log = Log::default();
    let d = || Driver::new(CoinCopy::new());
    let decoder = || Adapter::new(Restating::new(64, &log));
    let link = || Adapter::new(Unseedable::new(&log));
    let bits = || Executor::new(Bits::new(&log));
    let shots = ShotSet::ids(0..2);
    let mut left = runner(((d() >> decoder()) >> link()) >> bits());
    let mut right = runner(d() >> (decoder() >> (link() >> bits())));
    for (refusal, brackets) in [
        (refused(left.run(&shots, SEED)), "left"),
        (refused(right.run(&shots, SEED)), "right"),
    ] {
        assert!(
            matches!(refusal, RunRefusal::NotSeedable),
            "{brackets}: {refusal:?}"
        );
    }
    assert_eq!(log.count("Bits ready"), 0, "no part asked");
    left.run(&shots, None).expect("left: a run with no seed");
    right.run(&shots, None).expect("right: a run with no seed");
}

/// Seedable bits under a link that cannot be seeded, under a profiler,
/// bracketed both ways: a seeded run is refused before any part is asked,
/// and a run with no seed runs.
#[test]
fn an_adapter_can_make_a_chain_not_seedable() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let d = || Driver::new(CoinCopy::new());
    let profiler = || Adapter::new(Profiler::new(&counted, &log));
    let link = || Adapter::new(Unseedable::new(&log));
    let bits = || Executor::new(Bits::new(&log));
    let shots = ShotSet::ids(0..2);
    let mut left = runner(((d() >> profiler()) >> link()) >> bits());
    let mut right = runner(d() >> (profiler() >> (link() >> bits())));
    for (refusal, brackets) in [
        (refused(left.run(&shots, SEED)), "left"),
        (refused(right.run(&shots, SEED)), "right"),
    ] {
        assert!(
            matches!(refusal, RunRefusal::NotSeedable),
            "{brackets}: {refusal:?}"
        );
    }
    assert_eq!(log.count("Bits ready"), 0, "no part asked");
    left.run(&shots, None).expect("left: a run with no seed");
    right.run(&shots, None).expect("right: a run with no seed");
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

/// A profiler that cannot take a run, between bits and another profiler,
/// in each of the five bracketings: the run is refused with its reason;
/// the bits below it are asked, and the profiler above it is not.
#[test]
fn a_part_refusing_a_run_stops_the_asking_wherever_the_brackets_go() {
    let counted = Rc::new(Cell::new(0));
    let shots = ShotSet::ids(0..2);
    for brackets in 0..5 {
        let log = Log::default();
        let refusing_log = Log::default();
        let d = || Driver::new(CoinCopy::new());
        let above = || Adapter::new(Profiler::new(&counted, &log));
        let refusing = || Adapter::new(Profiler::new(&counted, &refusing_log).busy("calibrating"));
        let bits = || Executor::new(Bits::new(&log));
        let refusal = match brackets {
            0 => refused(runner(((d() >> above()) >> refusing()) >> bits()).run(&shots, SEED)),
            1 => refused(runner((d() >> (above() >> refusing())) >> bits()).run(&shots, SEED)),
            2 => refused(runner((d() >> above()) >> (refusing() >> bits())).run(&shots, SEED)),
            3 => refused(runner(d() >> ((above() >> refusing()) >> bits())).run(&shots, SEED)),
            _ => refused(runner(d() >> (above() >> (refusing() >> bits()))).run(&shots, SEED)),
        };
        assert!(
            matches!(&refusal, RunRefusal::Unavailable(why) if why.to_string() == "calibrating"),
            "{brackets}: {refusal:?}"
        );
        assert_eq!(log.count("Bits ready"), 1, "{brackets}: the bits asked");
        assert_eq!(
            refusing_log.count("Profiler ready"),
            1,
            "{brackets}: the refusing profiler asked"
        );
        assert_eq!(
            log.count("Profiler ready"),
            0,
            "{brackets}: the profiler above not asked"
        );
    }
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
