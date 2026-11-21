// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running shots on a chain: the records each shot gets, how a run is
//! awaited, and the parts' refusals. The runner's own refusals are in
//! `run_limits.rs`, and the failures that end a run in `run_error.rs`.

mod run_error;
mod run_limits;
mod wait;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::{Duration, Instant};

use crate::prelude::*;
use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, FailsOnce, FromThread, Later, Log, Panics, PanicsWhen, Profiler,
    Program, SeesSeeds, Splitter, Spread, Trap, Values, YieldNow, in_each_bracketing,
};
use crate::{
    DriveError, Failure, ReadoutFailure, ReadoutValue, RunError, RunRefusal, Runner,
    ShotConditions, ShotSet,
};

pub(super) const SEED: u64 = 7;

/// How long a test waits for a thread before it fails, rather than hangs.
const PATIENT: Duration = Duration::from_secs(20);

/// The runner of the chain `composed` gave, which must compose.
pub(super) fn runner<D: Driver, E: Executor>(
    composed: Result<Runner<D, E>, crate::CompositionFaults>,
) -> Runner<D, E> {
    let Ok(runner) = composed else {
        panic!("the chain composes");
    };
    runner
}

/// [`CoinCopy`] on bits alone.
fn coin_copy(log: &Log) -> Runner<CoinCopy, Bits> {
    runner(CoinCopy::new().on(Bits::new(log)))
}

/// The records of `shots` on [`CoinCopy`] on bits alone.
fn plain(shots: &ShotSet) -> Vec<Values> {
    coin_copy(&Log::default())
        .run(shots, SEED)
        .expect("the run succeeds")
}

/// The failure that ended a run.
pub(super) fn failed<T: std::fmt::Debug>(result: Result<T, RunError>) -> DriveError {
    match result {
        Err(RunError::Failed(error)) => error,
        other => panic!("a failed run, not {other:?}"),
    }
}

/// The refusal of a run.
pub(super) fn refused<T: std::fmt::Debug>(result: Result<T, RunError>) -> RunRefusal {
    match result {
        Err(RunError::Refused(refusal)) => refusal,
        other => panic!("a refused run, not {other:?}"),
    }
}

/// The shots' ends `log` saw, in order.
fn ended(log: &Log) -> Vec<String> {
    log.entries()
        .into_iter()
        .filter(|entry| entry.contains("end_shots"))
        .collect()
}

// ---------------------------------------------------------------- records

/// Coin copies under a noise and a splitter, which sends each instruction
/// as a batch of its own, over bits, each part logging the seed of every
/// call it gets, in each bracketing, on runs of other shots: every part
/// gets the run's seed, unchanged, the one a driver alone over bits gets,
/// whatever the shots and the batches, so that a replayable part's streams
/// depend on the run's seed alone; a run of another seed gets that seed.
#[test]
fn every_part_gets_the_runs_seed_unchanged() {
    /// The seeds `log` holds, by the label of the part that got each, in
    /// the order they came.
    fn seen(log: &Log) -> Vec<(String, String)> {
        log.entries()
            .into_iter()
            .filter_map(|entry| {
                let (label, seed) = entry.split_once(" got seed ")?;
                Some((label.to_owned(), seed.to_owned()))
            })
            .collect()
    }
    /// The seed a driver alone over bits gets in a run on `seed`.
    fn runs_seed(seed: u64) -> String {
        let log = Log::default();
        let driver = SeesSeeds::new(CoinCopy::new(), "driver", &log);
        runner(driver.on(Bits::new(&log)))
            .run(&ShotSet::ids([0]), seed)
            .expect("the run succeeds");
        seen(&log).swap_remove(0).1
    }
    let (run_seed, other_seed) = (runs_seed(SEED), runs_seed(SEED + 1));
    let labels = ["driver", "noise", "splitter", "bits"];
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = SeesSeeds::new(CoinCopy::new(), "driver", &log);
            let a = SeesSeeds::new(BitFlip::new(0.3, &log), "noise", &log);
            let b = SeesSeeds::new(Splitter::default(), "splitter", &log);
            let e = SeesSeeds::new(Bits::new(&log), "bits", &log);
        },
        |chain, brackets| {
            let mut runner = runner(chain);
            runner.run(&ShotSet::ids(0..8), SEED).expect("the run succeeds");
            runner.run(&ShotSet::ids([5, 2]), SEED).expect("the run succeeds");
            let seen_on_seed = seen(&log);
            for label in labels {
                assert!(
                    seen_on_seed.iter().any(|(got, _)| got == label),
                    "{brackets}: the {label} got a call"
                );
            }
            for (label, seed) in &seen_on_seed {
                assert_eq!(seed, &run_seed, "{brackets}: the {label}'s seed");
            }
            runner.run(&ShotSet::ids(0..8), SEED + 1).expect("the run succeeds");
            for (label, seed) in &seen(&log)[seen_on_seed.len()..] {
                assert_eq!(seed, &other_seed, "{brackets}: the {label}'s other seed");
            }
        }
    );
}

/// A program flipping qubit 1, reading 0 and 1, flipping 0 and reading it,
/// over bits: its values in the order it read them.
#[test]
fn a_shots_values_are_in_the_order_it_read_them() {
    let text = [("x", 1), ("mz", 0), ("mz", 1), ("x", 0), ("mz", 0)];
    let records = runner(Program::new(&text).on(Bits::new(&Log::default())))
        .run(&ShotSet::ids(0..4), SEED)
        .expect("the run succeeds");
    let expected = (
        vec![ReadoutValue::Zero, ReadoutValue::One, ReadoutValue::One],
        ShotConditions::NONE,
    );
    assert_eq!(records, vec![expected; 4]);
}

/// An adapter that changes no instruction's effect changes no record: a
/// profiler, which counts the instructions; a splitter, which sends each
/// instruction alone; a noise of 0, which flips nothing; a toy encoding
/// of 3 qubits per qubit, which uses the first. Coin copies, which branch
/// on a readout, and a program of one batch, through each, give the
/// records they give over bits alone.
#[test]
fn an_adapter_that_changes_no_instructions_effect_changes_no_record() {
    /// Checks the records of `driver` through each adapter.
    fn unchanged<D: Driver<Output: PartialEq + std::fmt::Debug>>(
        row: &str,
        driver: impl Fn() -> D,
    ) {
        let log = Log::default();
        let counted = Rc::new(Cell::new(0));
        let shots = ShotSet::ids(0..32);
        let bits = || Bits::new(&log);
        let expected = runner(driver().on(bits()))
            .run(&shots, SEED)
            .expect("the run succeeds");
        for (adapter, run) in [
            (
                "a profiler",
                runner(driver().through(Profiler::new(&counted, &log)).on(bits()))
                    .run(&shots, SEED),
            ),
            (
                "a splitter",
                runner(driver().through(Splitter::default()).on(bits())).run(&shots, SEED),
            ),
            (
                "a noise of 0",
                runner(driver().through(BitFlip::new(0.0, &log)).on(bits())).run(&shots, SEED),
            ),
            (
                "a toy encoding",
                runner(driver().through(Spread::new(3, &log)).on(bits())).run(&shots, SEED),
            ),
        ] {
            assert_eq!(
                run.expect("the run succeeds"),
                expected,
                "{row}, through {adapter}"
            );
        }
    }
    unchanged("coin copies", CoinCopy::new);
    unchanged("a program", || {
        Program::new(&[("x", 1), ("mz", 0), ("mz", 1), ("x", 0), ("mz", 0)])
    });
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
        Program::new(&[("mz", 0)])
            .through(BitFlip::new(0.0, &log))
            .through(Profiler::new(&counted, &log))
            .on(Bits::new(&log).losing()),
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

/// A run's shots are ended on every part, top to bottom, wherever the
/// brackets go: by the driver, if it ends them, and then always by the
/// runner, whose end of shots the driver ended finds nothing held, both
/// when the run succeeds and when it fails. Coin copies, which end their
/// shots, and a program that never ends them, each under a noise, above a
/// trap (on shot 5, or never springing), over bits; and coin copies under
/// a noise over bits alone, in both bracketings of three parts.
#[test]
fn a_runs_shots_are_ended_on_every_part_top_to_bottom() {
    let top_to_bottom = [
        "BitFlip end_shots, holding 0",
        "Trap end_shots",
        "Bits end_shots, holding 0",
    ];
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = CoinCopy::new();
            let a = BitFlip::new(0.3, &log);
            let b = Trap::on(5).logging(&log);
            let e = Bits::new(&log);
        },
        |chain, brackets| {
            let mut runner = runner(chain);
            runner
                .run(&ShotSet::ids(0..4), SEED)
                .expect("a run without shot 5 succeeds");
            let succeeded = ended(&log);
            failed(runner.run(&ShotSet::ids(0..8), SEED));
            assert_eq!(
                succeeded,
                [top_to_bottom, top_to_bottom].concat(),
                "{brackets}, ended by the driver, then the runner"
            );
            assert_eq!(
                ended(&log)[succeeded.len()..],
                top_to_bottom,
                "{brackets}, failed: ended by the runner"
            );
        }
    );
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = Program::new(&[("mz", 0)]).not_ending();
            let a = BitFlip::new(0.3, &log);
            let b = Trap::on(99).logging(&log);
            let e = Bits::new(&log);
        },
        |chain, brackets| {
            let records = runner(chain)
                .run(&ShotSet::ids(0..3), SEED)
                .expect("the run succeeds");
            assert_eq!(records.len(), 3, "{brackets}: every shot ran");
            assert_eq!(
                ended(&log),
                top_to_bottom,
                "{brackets}, the driver ending none: ended by the runner alone"
            );
        }
    );
    in_each_bracketing!(
        [d, a, e] from {
            let log = Log::default();
            let d = CoinCopy::new();
            let a = BitFlip::new(0.3, &log);
            let e = Bits::new(&log);
        },
        |chain, brackets| {
            runner(chain)
                .run(&ShotSet::ids(0..8), SEED)
                .expect("the run succeeds");
            let top_to_bottom = ["BitFlip end_shots, holding 0", "Bits end_shots, holding 0"];
            assert_eq!(
                ended(&log),
                [top_to_bottom, top_to_bottom].concat(),
                "{brackets}, three parts: ended by the driver, then the runner"
            );
        }
    );
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

/// A run awaited by another executor of futures than `run`'s, on an
/// executor answering from a thread: the records `run` gives.
#[test]
fn a_run_awaited_elsewhere_gives_the_records_of_run() {
    let log = Log::default();
    let shots = ShotSet::ids(0..16);
    let mut runner =
        runner(CoinCopy::new().on(FromThread::new(Bits::new(&log), Duration::from_millis(5))));
    assert_eq!(
        spin(runner.run_async(&shots, SEED)).expect("the run succeeds"),
        plain(&shots)
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
        CoinCopy::new().on(FromThread::new(Bits::new(&bits_log), Duration::ZERO).gated(gate)),
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
/// failing, or panicking in, its second `execute`. The unfinished run
/// leaves no shot's state in any part, and a caught panic does not keep
/// the chain from running, so the rerun with the same seed gives the
/// records of a new chain.
#[test]
fn a_rerun_after_an_unfinished_run_gives_a_new_chains_records() {
    /// Polls `runner`'s run of `shots` twice, then drops it unfinished.
    fn drop_unfinished<D: Driver, E: Executor>(
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
    let noise = |log: &Log| BitFlip::new(0.3, log);
    let coin_copy = || CoinCopy::new();
    let new_chain = runner(
        coin_copy()
            .through(noise(&Log::default()))
            .on(Bits::new(&Log::default())),
    )
    .run(&shots, SEED)
    .expect("the run succeeds");
    let mut reruns = Vec::new();
    let log = Log::default();
    let mut left = runner(coin_copy().through(noise(&log)).on(Later(Bits::new(&log))));
    drop_unfinished(&mut left, &shots, &log);
    reruns.push((
        "dropped, the noise with the driver".to_owned(),
        left.run(&shots, SEED),
    ));
    let log = Log::default();
    let mut right = runner(coin_copy().on(noise(&log).on(Later(Bits::new(&log)))));
    drop_unfinished(&mut right, &shots, &log);
    reruns.push((
        "dropped, the noise with the bits".to_owned(),
        right.run(&shots, SEED),
    ));
    for (ended, panics) in [("failed", false), ("panicked", true)] {
        let fails_once = || {
            if panics {
                FailsOnce::panicking()
            } else {
                FailsOnce::new()
            }
        };
        let as_expected = |failure: &Failure| {
            if panics {
                matches!(failure, Failure::Panicked { .. })
            } else {
                matches!(failure, Failure::Part(_))
            }
        };
        let log = Log::default();
        let mut left = runner(
            coin_copy()
                .through(noise(&log))
                .through(fails_once())
                .on(Bits::new(&log)),
        );
        let first = failed(left.run(&shots, SEED));
        assert!(as_expected(&first.failure), "{ended}: {first:?}");
        let row = format!("{ended}, the noise with the driver");
        reruns.push((row, left.run(&shots, SEED)));
        let log = Log::default();
        let mut right = runner(coin_copy().on(noise(&log).on(fails_once().on(Bits::new(&log)))));
        let first = failed(right.run(&shots, SEED));
        assert!(as_expected(&first.failure), "{ended}: {first:?}");
        reruns.push((
            format!("{ended}, the noise with the bits"),
            right.run(&shots, SEED),
        ));
    }
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
    let driver = || Panics(PanicsWhen::Polled);
    let splitter = || Splitter::default();
    let executor = || Panics(PanicsWhen::Polled);
    let mut joined = runner(driver().on(executor()));
    is_send(&joined.run_async(&shots, SEED));
    let mut left = runner(driver().through(splitter()).on(executor()));
    is_send(&left.run_async(&shots, SEED));
    let mut right = runner(driver().on(splitter().on(executor())));
    is_send(&right.run_async(&shots, SEED));
}

// ---------------------------------------------------------------- refusals

/// How often `log` says [`Bits`] were asked whether they can take a run.
pub(super) fn bits_asked(log: &Log) -> usize {
    log.entries()
        .iter()
        .filter(|entry| entry.starts_with("Bits ready"))
        .count()
}

/// The position and the name's last segment of the part `refusal`
/// names, if it refuses for "calibrating".
fn calibrating(refusal: &RunRefusal) -> Option<(usize, &str)> {
    match refusal {
        RunRefusal::PartNotReady { part, not_ready }
            if not_ready.why().to_string() == "calibrating" =>
        {
            Some((part.position, part.name.rsplit("::").next()?))
        }
        _ => None,
    }
}

/// Each part is asked whether it can take a run, the lowest first,
/// wherever the brackets go, until one refuses: a part that cannot take a
/// run refuses it, named by its position and name, with its reason; the
/// parts below it are asked, those above it are not, and nothing runs: no
/// shot is ended on any part, as no drive started.
#[test]
fn the_parts_are_asked_bottom_up_until_one_refuses() {
    /// The parts `log` says were asked, in order.
    fn asked(log: &Log) -> Vec<String> {
        log.entries()
            .into_iter()
            .filter(|entry| entry.contains(" ready"))
            .collect()
    }
    let counted = Rc::new(Cell::new(0));
    // Busy bits under a profiler: refused with the bits' reason, and the
    // profiler above them is not asked.
    let log = Log::default();
    let mut busy = runner(
        CoinCopy::new()
            .through(Profiler::new(&counted, &log))
            .on(Bits::new(&log).busy("calibrating")),
    );
    let refusal = refused(busy.run(&ShotSet::ids(0..4), SEED));
    assert_eq!(
        calibrating(&refusal),
        Some((2, "Bits")),
        "busy bits: {refusal:?}"
    );
    assert_eq!(
        log.count("Profiler ready"),
        0,
        "busy bits: the profiler not asked"
    );
    assert_eq!(ended(&log), [] as [String; 0], "busy bits: no shot ended");
    // A profiler that cannot take a run, between bits and another profiler:
    // the bits below it are asked, and the profiler above it is not.
    let shots = ShotSet::ids(0..2);
    in_each_bracketing!([d, above, refusing, bits] from {
        let log = Log::default();
        let refusing_log = Log::default();
        let d = CoinCopy::new();
        let above = Profiler::new(&counted, &log);
        let refusing = Profiler::new(&counted, &refusing_log).busy("calibrating");
        let bits = Bits::new(&log);
    }, |chain, brackets| {
        let refusal = refused(runner(chain).run(&shots, SEED));
        assert_eq!(
            calibrating(&refusal),
            Some((2, "Profiler")),
            "{brackets}: {refusal:?}"
        );
        assert_eq!(bits_asked(&log), 1, "{brackets}: the bits asked");
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
        assert_eq!(ended(&log), [] as [String; 0], "{brackets}: no shot ended");
    });
    // A driver that cannot take a run: every part below it is asked.
    let log = Log::default();
    let refusal = refused(
        runner(
            CoinCopy::new()
                .busy("calibrating")
                .through(Profiler::new(&counted, &log))
                .on(Bits::new(&log)),
        )
        .run(&shots, SEED),
    );
    assert_eq!(
        calibrating(&refusal),
        Some((0, "CoinCopy")),
        "busy driver: {refusal:?}"
    );
    assert_eq!(
        asked(&log),
        ["Bits ready for 2 qubits", "Profiler ready"],
        "busy driver: the parts below asked"
    );
    assert_eq!(ended(&log), [] as [String; 0], "busy driver: no shot ended");
    assert_eq!(counted.get(), 0, "nothing ran");
    // Every part takes the run: each is asked, bottom up.
    let program = || Program::new(&[("mz", 0)]);
    let log = Log::default();
    let profiler = Profiler::new(&counted, &log);
    runner(program().through(profiler).on(Bits::new(&log)))
        .run(&shots, SEED)
        .expect("the run succeeds");
    assert_eq!(
        asked(&log),
        ["Bits ready for 1 qubits", "Profiler ready"],
        "the profiler with the driver"
    );
    let log = Log::default();
    let profiler = Profiler::new(&counted, &log);
    runner(program().on(profiler.on(Bits::new(&log))))
        .run(&shots, SEED)
        .expect("the run succeeds");
    assert_eq!(
        asked(&log),
        ["Bits ready for 1 qubits", "Profiler ready"],
        "the profiler with the bits"
    );
}
