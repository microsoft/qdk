// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running shots on a chain: the records each shot gets, the run's
//! refusals, the failures that end it, the checks at each boundary, and
//! what an `execute` does with the readouts its caller lends.

mod boundaries;
mod failures;
mod lent_readouts;
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
    BitFlip, Bits, CoinCopy, FailsOnce, FromThread, Later, Log, Offering, Panics, PanicsWhen,
    Profiler, Program, Splitter, Spread, Trap, Values, YieldNow, in_each_bracketing,
};
use crate::{
    Adapter, Capabilities, Drive, DriveError, Driver, Execute, ExecuteRefusal, Executor, Failure,
    InstructionFault, InstructionSet, Needs, OpcodeMap, ReadoutFailure, ReadoutValue, Readouts,
    RunError, RunRefusal, Runner, Seed, ShotConditions, ShotId, ShotSet,
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

/// Coin copies under a noise flipping 30% of the `x`s, over bits, in both
/// bracketings, whose first run does not finish: it is dropped after its
/// first `execute`, the bits answering later, or failed by an adapter
/// failing its second `execute`. The unfinished run leaves no shot's state
/// in any part, so the rerun with the same seed gives the records of a new
/// chain.
#[test]
fn a_rerun_after_an_unfinished_run_gives_a_new_chains_records() {
    /// Polls `runner`'s run of `shots` twice, then drops it unfinished.
    fn drop_unfinished<D: Drive, E: Execute>(
        runner: &mut Runner<D, E>,
        shots: &ShotSet,
        log: &Log,
    ) {
        {
            let mut run = std::pin::pin!(runner.run_async(shots, SEED));
            for _ in 0..2 {
                let poll = run.as_mut().poll(&mut Context::from_waker(Waker::noop()));
                assert!(poll.is_pending(), "the run is unfinished");
            }
        }
        assert_eq!(log.count("Bits execute"), 1, "one execute before the drop");
    }
    let shots = ShotSet::ids(0..16);
    let noise = |log: &Log| Adapter::new(BitFlip::new(300, log));
    let coin_copy = || Driver::new(CoinCopy::new());
    let new_chain =
        runner(coin_copy() >> noise(&Log::default()) >> Executor::new(Bits::new(&Log::default())))
            .run(&shots, SEED)
            .expect("the run succeeds");
    let mut reruns = Vec::new();
    let log = Log::default();
    let mut left = runner((coin_copy() >> noise(&log)) >> Executor::new(Later(Bits::new(&log))));
    drop_unfinished(&mut left, &shots, &log);
    reruns.push(("dropped, the noise with the driver", left.run(&shots, SEED)));
    let log = Log::default();
    let mut right = runner(coin_copy() >> (noise(&log) >> Executor::new(Later(Bits::new(&log)))));
    drop_unfinished(&mut right, &shots, &log);
    reruns.push(("dropped, the noise with the bits", right.run(&shots, SEED)));
    let log = Log::default();
    let mut left = runner(
        ((coin_copy() >> noise(&log)) >> Adapter::new(FailsOnce::new()))
            >> Executor::new(Bits::new(&log)),
    );
    failed(left.run(&shots, SEED));
    reruns.push(("failed, the noise with the driver", left.run(&shots, SEED)));
    let log = Log::default();
    let mut right = runner(
        coin_copy()
            >> (noise(&log) >> (Adapter::new(FailsOnce::new()) >> Executor::new(Bits::new(&log)))),
    );
    failed(right.run(&shots, SEED));
    reruns.push(("failed, the noise with the bits", right.run(&shots, SEED)));
    for (row, rerun) in reruns {
        assert_eq!(rerun.expect("the rerun succeeds"), new_chain, "{row}");
    }
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

/// Runs one shot more than `most` on `runner`, refused before any part is
/// asked or runs, then `most` shots, a record each: a row of
/// [`a_run_of_more_shots_than_its_chain_takes_is_refused`], named `row`.
fn takes_at_most<D: Drive<Output: std::fmt::Debug>, E: Execute>(
    row: &str,
    mut runner: Runner<D, E>,
    most: u32,
    log: &Log,
) {
    let asked = log.count("Bits ready");
    let ran = log.count("Bits execute");
    let refusal = refused(runner.run(&ShotSet::ids(0..=most), SEED));
    assert!(
        matches!(refusal, RunRefusal::TooManyShots { asked, most: limit } if asked == most + 1 && limit == most),
        "{row}: {refusal:?}"
    );
    assert_eq!(log.count("Bits ready"), asked, "{row}: no part asked");
    assert_eq!(log.count("Bits execute"), ran, "{row}: nothing ran");
    let records = runner
        .run(&ShotSet::ids(0..most), SEED)
        .unwrap_or_else(|error| panic!("{row}: a run of {most}: {error:?}"));
    assert_eq!(records.len(), most as usize, "{row}: a record per shot");
}

/// A chain runs at most the fewest shots any of its parts offers, in every
/// bracketing: a run of one more is refused before any part is asked or
/// runs, and a run of that many runs. So an adapter may lower the most
/// shots a run may have, but not raise it.
#[test]
fn a_run_of_more_shots_than_its_chain_takes_is_refused() {
    let log = Log::default();
    let bits = |most| {
        let offer = Capabilities::unlimited().max_qubits(64).max_shots(most);
        Executor::new(Bits::new(&log).offering(offer))
    };
    // An executor running 4 shots at most.
    let alone = runner(Driver::new(CoinCopy::new()) >> bits(4));
    takes_at_most("the executor alone", alone, 4, &log);
    // Bits under a job queue of 4 shots at most: the queue's limit holds
    // wherever the brackets go.
    in_each_bracketing!([d, queue, e] from {
        let d = Driver::new(CoinCopy::new());
        let queue = Adapter::new(Offering::queue(4, &log));
        let e = Executor::new(Bits::new(&log));
    }, |chain, brackets| {
        takes_at_most(&format!("under a queue, {brackets}"), runner(chain), 4, &log);
    });
    // Bits running 1 shot at most, under a decoder that restates its offer
    // with no shot limit, or with a limit of 4: the bits' limit still holds.
    in_each_bracketing!([d, decoder, e] from {
        let d = Driver::new(CoinCopy::new());
        let decoder = Adapter::new(Offering::restating(64, &log));
        let e = bits(1);
    }, |chain, brackets| {
        let row = format!("a decoder with no limit, {brackets}");
        takes_at_most(&row, runner(chain), 1, &log);
    });
    in_each_bracketing!([d, decoder, e] from {
        let d = Driver::new(CoinCopy::new());
        let decoder = Adapter::new(Offering::restating_most_shots(64, 4, &log));
        let e = bits(1);
    }, |chain, brackets| {
        let row = format!("a decoder raising the limit, {brackets}");
        takes_at_most(&row, runner(chain), 1, &log);
    });
    // Bits running 8 shots at most, under a decoder that restates its offer
    // with a limit of 4: the decoder's lower limit holds.
    in_each_bracketing!([d, decoder, e] from {
        let d = Driver::new(CoinCopy::new());
        let decoder = Adapter::new(Offering::restating_most_shots(64, 4, &log));
        let e = bits(8);
    }, |chain, brackets| {
        let row = format!("a decoder lowering the limit, {brackets}");
        takes_at_most(&row, runner(chain), 4, &log);
    });
    // Bits running 8 shots at most, under a job queue of 4, under a decoder
    // that restates its offer with no shot limit: the queue's limit, though
    // neither the executor's nor the one reaching the driver, holds.
    in_each_bracketing!([d, decoder, queue, e] from {
        let d = Driver::new(CoinCopy::new());
        let decoder = Adapter::new(Offering::restating(64, &log));
        let queue = Adapter::new(Offering::queue(4, &log));
        let e = bits(8);
    }, |chain, brackets| {
        let row = format!("a queue under a decoder, {brackets}");
        takes_at_most(&row, runner(chain), 4, &log);
    });
}

/// Runs `shots` shots with a seed on `runner`, refused before any part is
/// asked or runs, then with no seed: a row of
/// [`a_seeded_run_is_refused_unless_every_part_is_seedable`], named `row`.
fn refuses_a_seed<D: Drive<Output: std::fmt::Debug>, E: Execute>(
    row: &str,
    mut runner: Runner<D, E>,
    shots: u32,
    log: &Log,
) {
    let shots = ShotSet::ids(0..shots);
    let asked = log.count("Bits ready");
    let ran = log.count("Bits execute");
    let refusal = refused(runner.run(&shots, SEED));
    assert!(
        matches!(refusal, RunRefusal::NotSeedable),
        "{row}: {refusal:?}"
    );
    assert_eq!(log.count("Bits ready"), asked, "{row}: no part asked");
    assert_eq!(log.count("Bits execute"), ran, "{row}: nothing ran");
    runner
        .run(&shots, None)
        .unwrap_or_else(|error| panic!("{row}: a run with no seed: {error:?}"));
}

/// A chain is seedable only if every part is, in every bracketing: a seeded
/// run on one that is not is refused before any part is asked or runs, and
/// a run with no seed runs. So an adapter may make a chain not seedable,
/// but cannot make a chain that is not seedable seedable.
#[test]
fn a_seeded_run_is_refused_unless_every_part_is_seedable() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let unseedable_bits = || {
        let offer = Capabilities::unlimited().max_qubits(64).not_seedable();
        Executor::new(Bits::new(&log).offering(offer))
    };
    // An executor that cannot be seeded.
    let alone = runner(Driver::new(CoinCopy::new()) >> unseedable_bits());
    refuses_a_seed("the executor alone", alone, 4, &log);
    // Bits that cannot be seeded, under a decoder that restates its offer
    // as seedable: the bits' seedability still holds.
    in_each_bracketing!([d, decoder, e] from {
        let d = Driver::new(CoinCopy::new());
        let decoder = Adapter::new(Offering::restating(64, &log));
        let e = unseedable_bits();
    }, |chain, brackets| {
        let row = format!("a decoder over bits, {brackets}");
        refuses_a_seed(&row, runner(chain), 1, &log);
    });
    // Seedable bits under a link that cannot be seeded, under a profiler.
    in_each_bracketing!([d, profiler, link, e] from {
        let d = Driver::new(CoinCopy::new());
        let profiler = Adapter::new(Profiler::new(&counted, &log));
        let link = Adapter::new(Offering::unseedable(&log));
        let e = Executor::new(Bits::new(&log));
    }, |chain, brackets| {
        let row = format!("a link under a profiler, {brackets}");
        refuses_a_seed(&row, runner(chain), 2, &log);
    });
    // Seedable bits under a link that cannot be seeded, under a decoder
    // that restates its offer as seedable: the link's seedability, though
    // neither the executor's nor the one reaching the driver, holds.
    in_each_bracketing!([d, decoder, link, e] from {
        let d = Driver::new(CoinCopy::new());
        let decoder = Adapter::new(Offering::restating(64, &log));
        let link = Adapter::new(Offering::unseedable(&log));
        let e = Executor::new(Bits::new(&log));
    }, |chain, brackets| {
        let row = format!("a link under a decoder, {brackets}");
        refuses_a_seed(&row, runner(chain), 2, &log);
    });
}

/// A part that cannot take a run refuses it with its reason, wherever the
/// brackets go: the parts below it are asked, those above it are not, and
/// nothing runs.
#[test]
fn a_part_refusing_a_run_stops_the_asking_wherever_the_brackets_go() {
    let counted = Rc::new(Cell::new(0));
    let calibrating = |refusal: &RunRefusal| matches!(refusal, RunRefusal::Unavailable(why) if why.to_string() == "calibrating");
    // Busy bits under a profiler: refused with the bits' reason, and the
    // profiler above them is not asked.
    let log = Log::default();
    let mut busy = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(Profiler::new(&counted, &log))
            >> Executor::new(Bits::new(&log).busy("calibrating")),
    );
    let refusal = refused(busy.run(&ShotSet::ids(0..4), SEED));
    assert!(calibrating(&refusal), "busy bits: {refusal:?}");
    assert_eq!(
        log.count("Profiler ready"),
        0,
        "busy bits: the profiler not asked"
    );
    // A profiler that cannot take a run, between bits and another profiler:
    // the bits below it are asked, and the profiler above it is not.
    let shots = ShotSet::ids(0..2);
    in_each_bracketing!([d, above, refusing, bits] from {
        let log = Log::default();
        let refusing_log = Log::default();
        let d = Driver::new(CoinCopy::new());
        let above = Adapter::new(Profiler::new(&counted, &log));
        let refusing = Adapter::new(Profiler::new(&counted, &refusing_log).busy("calibrating"));
        let bits = Executor::new(Bits::new(&log));
    }, |chain, brackets| {
        let refusal = refused(runner(chain).run(&shots, SEED));
        assert!(calibrating(&refusal), "{brackets}: {refusal:?}");
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
    });
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

/// Two checked batches in one `execute`, shots 4 and 2 measuring qubit 0,
/// shots 9 and 7 flipping qubit 5 beyond the program's 2 qubits: the
/// second batch is refused, named by its first shot, 9, and neither batch
/// runs.
#[test]
fn a_later_batchs_refusal_names_it_and_runs_no_batch() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(
            Program::halves(&[("mz", 0)], &[("x", 5)])
                .needing(Needs::qubits(2))
                .with_checked_batches(),
        ) >> Executor::new(Bits::new(&log)),
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
        let mut answer = Readouts::new();
        below.execute(&[], seed, &mut answer).await?;
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
