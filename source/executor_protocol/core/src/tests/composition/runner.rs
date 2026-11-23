// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running shots on a chain: the records each shot gets, how a run is
//! awaited, and the refusals of a run, the runner's own and the parts'.
//! The failures that end a run are in `run_error.rs`.

mod run_error;
mod wait;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::{Duration, Instant};

use super::driver_joins::{NO_X, fault, listed, part, short_shots};
use super::{PATIENT, SEED, ends_soon, failed, plain, refused};
use crate::prelude::*;
use crate::tests::example_sets::Measuring;
use crate::tests::example_sets::{BitReadout, ShotFlags, ToffoliGate};
use crate::tests::fake_parts::{
    BitFlip, Bits, CczDecomposition, CliffordTRegister, CoinCopy, FailsOnce, FromThread,
    GateForGate, Later, Log, Panics, PanicsWhen, Profiler, Program, Renamed, Splitter, Spread,
    Stating, ToffoliProgram, Trap, Values, YieldNow, batches_of, coin_gates, in_each_bracketing,
};
use crate::tests::runner;
use crate::{
    BoundaryCompatible, BoundaryMismatch, CapabilitiesMismatch, CompositionFault, Failure,
    FeatureMismatch, Features, ResourceMismatch, RunError, RunRefusal, Runner, ShotSet,
    StatesLimits,
};

/// [`CoinCopy`] on bits alone.
fn coin_copy(log: &Log) -> Runner<StatesLimits<CoinCopy>, StatesLimits<Bits>> {
    runner(CoinCopy::new().on(Bits::new(log)))
}

/// The shots' ends `log` saw, in order.
fn ended(log: &Log) -> Vec<String> {
    log.entries_containing("end_shots")
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
    /// The seeds `log` holds, by the name of the part that got each, in
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
        let driver = Renamed::seeing_seeds(CoinCopy::new(), "driver", &log);
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
            let d = Renamed::seeing_seeds(CoinCopy::new(), "driver", &log);
            let a = Renamed::seeing_seeds(BitFlip::new(0.3, &log), "noise", &log);
            let b = Renamed::seeing_seeds(Splitter::new(), "splitter", &log);
            let e = Renamed::seeing_seeds(Bits::new(&log), "bits", &log);
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
        vec![BitReadout::Zero, BitReadout::One, BitReadout::One],
        ShotFlags::NONE,
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
    fn unchanged<D: Driver<Sends = Measuring, Output: PartialEq + std::fmt::Debug>>(
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
                runner(driver().through(Splitter::new()).on(bits())).run(&shots, SEED),
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
        vec![(vec![BitReadout::Lost], ShotFlags::REJECTED); 3]
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

/// Coin copies under a noise flipping 30% of the `x`s, over bits, in each
/// bracketing, whose first run does not finish: it is dropped after its
/// first `execute`, the bits answering later, or failed by an adapter
/// between the noise and the bits failing, or panicking in, its second
/// `execute`. The unfinished run
/// leaves no shot's state in any part, and a caught panic does not keep
/// the chain from running, so the rerun with the same seed gives the
/// records of a new chain.
#[test]
fn a_rerun_after_an_unfinished_run_gives_a_new_chains_records() {
    /// Polls `runner`'s run of `shots` twice, then drops it unfinished.
    fn drop_unfinished<D: Driver, E: Executor<Accepts = D::Sends>>(
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
    let new_chain = runner(
        CoinCopy::new()
            .through(BitFlip::new(0.3, &Log::default()))
            .on(Bits::new(&Log::default())),
    )
    .run(&shots, SEED)
    .expect("the run succeeds");
    let mut reruns = Vec::new();
    in_each_bracketing!(
        [d, a, e] from {
            let log = Log::default();
            let d = CoinCopy::new();
            let a = BitFlip::new(0.3, &log);
            let e = Later(Bits::new(&log)).stating_limits();
        },
        |chain, brackets| {
            let mut runner = runner(chain);
            drop_unfinished(&mut runner, &shots, &log);
            reruns.push((format!("dropped, {brackets}"), runner.run(&shots, SEED)));
        }
    );
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
        in_each_bracketing!(
            [d, a, b, e] from {
                let log = Log::default();
                let d = CoinCopy::new();
                let a = BitFlip::new(0.3, &log);
                let b = fails_once();
                let e = Bits::new(&log);
            },
            |chain, brackets| {
                let mut runner = runner(chain);
                let first = failed(runner.run(&shots, SEED));
                assert!(as_expected(&first.failure), "{ended}, {brackets}: {first:?}");
                reruns.push((format!("{ended}, {brackets}"), runner.run(&shots, SEED)));
            }
        );
    }
    for (row, rerun) in reruns {
        assert_eq!(rerun.expect("the rerun succeeds"), new_chain, "{row}");
    }
}

/// Whether `T` may move to another thread; it compiles only if so.
fn is_send<T: Send>(_: &T) {}

/// Parts that are `Send`, and whose futures are, in three bracketings, and
/// a translating adapter between them in two: the future of `run_async`
/// may move to another thread.
#[test]
fn a_runs_future_is_send_when_every_part_and_its_future_are() {
    let shots = ShotSet::ids(0..2);
    let driver = || Panics(PanicsWhen::Polled).stating_limits();
    let splitter = || Splitter::new();
    let executor = || Panics(PanicsWhen::Polled);
    let mut joined = runner(driver().on(executor()));
    is_send(&joined.run_async(&shots, SEED));
    let mut left = runner(driver().through(splitter()).on(executor()));
    is_send(&left.run_async(&shots, SEED));
    let mut right = runner(driver().on(splitter().on(executor())));
    is_send(&right.run_async(&shots, SEED));
    let translating = GateForGate::default;
    let mut translated_left = runner(driver().through(translating()).on(executor()));
    is_send(&translated_left.run_async(&shots, SEED));
    let mut translated_right = runner(driver().on(translating().on(executor())));
    is_send(&translated_right.run_async(&shots, SEED));
}

// ---------------------------------------------------------------- refusals

/// How often `log` says [`Bits`] were asked whether they can take a run.
pub(super) fn bits_asked(log: &Log) -> usize {
    log.entries_starting_with("Bits ready").len()
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
/// shot is ended on any part, as no drive started. A part refusing inside
/// a joined part held by a part of a user's, which the chain's check does
/// not see into, is named by the user's part, with its own reason; one
/// below a joined part lent by `&mut` is named by its own position; a
/// translating adapter that cannot take a run is named as any part, and
/// nothing is sent below it.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one scene per place a refusal comes from"
)]
fn the_parts_are_asked_bottom_up_until_one_refuses() {
    /// The parts `log` says were asked, in order.
    fn asked(log: &Log) -> Vec<String> {
        log.entries_containing(" ready")
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
    // Busy bits inside a user's executor: refused naming the user's part.
    let log = Log::default();
    let held = Renamed::new(
        Profiler::new(&counted, &log).on(Bits::new(&log).busy("calibrating")),
        "user's executor",
    );
    let refusal = refused(runner(CoinCopy::new().on(held)).run(&ShotSet::ids(0..4), SEED));
    assert_eq!(
        calibrating(&refusal),
        Some((1, "user's executor")),
        "held busy bits: {refusal:?}"
    );
    assert_eq!(
        ended(&log),
        [] as [String; 0],
        "held busy bits: no shot ended"
    );
    // Busy bits below a joined part lent by `&mut`: named by their
    // position, below each part the lent part holds.
    let log = Log::default();
    let mut lent = Profiler::new(&counted, &log).through(Profiler::new(&counted, &log));
    let refusal = refused(
        runner(CoinCopy::new().on((&mut lent).on(Bits::new(&log).busy("calibrating"))))
            .run(&ShotSet::ids(0..4), SEED),
    );
    assert_eq!(
        calibrating(&refusal),
        Some((3, "Bits")),
        "busy bits below a lent part: {refusal:?}"
    );
    // A translating adapter that cannot take a run: named, and nothing is
    // sent below it.
    let log = Log::default();
    let refusal = refused(
        runner(
            ToffoliProgram::new(&[ToffoliGate::Mz(0)], 3)
                .through(CczDecomposition::not_ready("calibrating"))
                .on(CliffordTRegister::new(3, &[BitReadout::Zero], &log)),
        )
        .run(&ShotSet::ids([0]), SEED),
    );
    assert_eq!(
        calibrating(&refusal),
        Some((1, "CczDecomposition")),
        "a translating adapter not ready: {refusal:?}"
    );
    assert_eq!(
        log.entries_starting_with("runs"),
        [] as [String; 0],
        "a translating adapter not ready: nothing sent below"
    );
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
        ["Bits ready for batches of 2", "Profiler ready"],
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
        ["Bits ready for batches of 1", "Profiler ready"],
        "the profiler with the driver"
    );
    let log = Log::default();
    let profiler = Profiler::new(&counted, &log);
    runner(program().on(profiler.on(Bits::new(&log))))
        .run(&shots, SEED)
        .expect("the run succeeds");
    assert_eq!(
        asked(&log),
        ["Bits ready for batches of 1", "Profiler ready"],
        "the profiler with the bits"
    );
}

/// The refusals of runs of `shots`, in turn, on the one runner of the
/// chain `compose` makes on a new log, which must compose, and whether
/// anything ran: the instructions `counted` saw and the shots' ends the
/// log saw.
fn refused_in_turn<D: Driver, E: Executor<Accepts = D::Sends>>(
    shots: &[u32],
    counted: &Cell<usize>,
    compose: impl FnOnce(&Log) -> Result<Runner<D, E>, crate::CompositionFaults>,
) -> (Vec<RunRefusal>, usize, Vec<String>)
where
    D::Output: std::fmt::Debug,
{
    let log = Log::default();
    let mut runner = runner(compose(&log));
    let refusals = (shots.iter())
        .map(|&n| refused(runner.run(&ShotSet::ids(0..n), SEED)))
        .collect();
    (refusals, counted.get(), ended(&log))
}

/// A part of each role joined from two parts, wrapped by a part of a
/// user's that forwards its role's methods and the limits of what it
/// holds, where the chain's check does not see its inner boundary: the
/// chain composes, as its outer boundaries fit, but the joined part
/// refuses each run its inner boundary does not take, at `ready`, as a
/// part that cannot take it, named by the user's part's position and name,
/// with the boundary's mismatch as its reason; nothing runs, and no shot
/// is ended on any part. Its halves' sets not fitting, it refuses every
/// run, and so too held by a part that forwards only its role's public
/// methods, which gets it ready alone; an executor whose lower part
/// takes 3 shots a run, and a driver joined through a job queue of 3
/// shots, refuse a run of 4; one whose inner join, under a repetition
/// doubling the shots, has a lower part taking 3 shots and no `x`, refuses
/// runs of 1, 2 and 1 shots with the instructions alone, then with the 4
/// shots too, in one `BoundaryMismatch`, then the instructions alone
/// again, each run checked anew; and an interleaver joined with a decoder
/// limiting batches to 4, under a program of batches of 5, refuses every
/// run, as the program's batches of 5 pass through the interleaver to the
/// decoder.
#[test]
#[expect(clippy::too_many_lines, reason = "one row per role and boundary")]
fn a_wrapped_joined_part_refuses_each_run_its_inner_boundary_does_not_take() {
    let no_x = || Some(Box::new(coin_gates().fits(&NO_X).expect_err("no `x`")) as _);
    let resource = |resource| CapabilitiesMismatch {
        resources: vec![resource],
        ..CapabilitiesMismatch::default()
    };
    let four_shots_over_3 = || {
        resource(ResourceMismatch::Shots {
            required: 4,
            provided: 3,
        })
    };
    let no_x_fault = format!(
        "{:?}",
        BoundaryMismatch {
            instructions: no_x(),
            ..BoundaryMismatch::default()
        }
    );
    let shots_fault = format!(
        "{:?}",
        BoundaryMismatch {
            capabilities: four_shots_over_3(),
            ..BoundaryMismatch::default()
        }
    );
    let batches_fault = format!(
        "{:?}",
        BoundaryMismatch {
            capabilities: resource(ResourceMismatch::InstructionsPerBatch {
                required: 5,
                provided: 4,
            }),
            ..BoundaryMismatch::default()
        }
    );
    let no_x_and_shots = format!(
        "{:?}",
        BoundaryMismatch {
            instructions: no_x(),
            capabilities: four_shots_over_3(),
        }
    );
    let counted = Rc::new(Cell::new(0));
    let profiler = |set, log: &Log| Profiler::on(set, &counted, log);
    let rows = [
        (
            "a driver through an adapter",
            0,
            "user's driver",
            vec![&no_x_fault; 2],
            refused_in_turn(&[4, 4], &counted, |log| {
                let held = Renamed::new(
                    CoinCopy::new().through(profiler(NO_X, log)),
                    "user's driver",
                );
                held.on(Bits::accepting(NO_X, log))
            }),
        ),
        (
            "an adapter through an adapter",
            1,
            "user's adapter",
            vec![&no_x_fault; 2],
            refused_in_turn(&[4, 4], &counted, |log| {
                let held = Renamed::new(
                    profiler(*coin_gates(), log).through(profiler(NO_X, log)),
                    "user's adapter",
                );
                CoinCopy::new().through(held).on(Bits::accepting(NO_X, log))
            }),
        ),
        (
            "an adapter on an executor",
            1,
            "user's executor",
            vec![&no_x_fault; 2],
            refused_in_turn(&[4, 4], &counted, |log| {
                let held = Renamed::new(
                    profiler(*coin_gates(), log).on(Bits::accepting(NO_X, log)),
                    "user's executor",
                );
                CoinCopy::new().on(held)
            }),
        ),
        (
            "an adapter on an executor, held stating nothing",
            1,
            "user's executor",
            vec![&no_x_fault; 2],
            refused_in_turn(&[4, 4], &counted, |log| {
                let held = Renamed::stating_nothing(
                    profiler(*coin_gates(), log).on(Bits::accepting(NO_X, log)),
                    "user's executor",
                );
                CoinCopy::new().on(held)
            }),
        ),
        (
            "an adapter on an executor of 3 shots",
            1,
            "user's executor",
            vec![&shots_fault; 2],
            refused_in_turn(&[4, 4], &counted, |log| {
                let held = Renamed::new(
                    profiler(*coin_gates(), log)
                        .on(Bits::new(log).with_resources(batches_of(64).with_shots(3))),
                    "user's executor",
                );
                CoinCopy::new().on(held)
            }),
        ),
        (
            "a repetition on an adapter on an executor of 3 shots and no `x`",
            1,
            "user's executor",
            vec![&no_x_fault, &no_x_and_shots, &no_x_fault],
            refused_in_turn(&[1, 2, 1], &counted, |log| {
                let bits = Bits::accepting(NO_X, log).with_resources(batches_of(64).with_shots(3));
                let held = Renamed::new(
                    Stating::repeating_shots(2, log).on(profiler(*coin_gates(), log).on(bits)),
                    "user's executor",
                );
                CoinCopy::new().on(held)
            }),
        ),
        (
            "a driver through a queue of 3 shots",
            0,
            "user's driver",
            vec![&shots_fault; 2],
            refused_in_turn(&[4, 4], &counted, |log| {
                let held = Renamed::new(
                    CoinCopy::new().through(Stating::queue(3, log)),
                    "user's driver",
                );
                held.on(Bits::new(log))
            }),
        ),
        (
            "an adapter through a decoder of batches of 4",
            1,
            "user's adapter",
            vec![&batches_fault; 2],
            refused_in_turn(&[4, 4], &counted, |log| {
                let held = Renamed::new(
                    Stating::interleaving(3, log).through(Stating::capping(4, log)),
                    "user's adapter",
                );
                Program::new(&[("mz", 0)])
                    .with_resources(batches_of(5))
                    .through(held)
                    .on(Bits::new(log).with_resources(batches_of(10)))
            }),
        ),
    ];
    for (row, position, name, faults, (refusals, ran, ended)) in rows {
        assert_eq!(refusals.len(), faults.len(), "{row}: one fault a run");
        for (refusal, fault) in refusals.iter().zip(faults) {
            let RunRefusal::PartNotReady { part, not_ready } = refusal else {
                panic!("{row}: a part's refusal, not {refusal:?}");
            };
            assert_eq!(
                (part.position, part.name.as_str()),
                (position, name),
                "{row}"
            );
            assert_eq!(
                (not_ready.why().downcast_ref::<BoundaryMismatch>())
                    .map(|mismatch| format!("{mismatch:?}"))
                    .as_ref(),
                Some(fault),
                "{row}: {refusal:?}"
            );
        }
        assert_eq!(ran, 0, "{row}: no instruction ran");
        assert_eq!(ended, [] as [String; 0], "{row}: no shot ended");
    }
}

/// The capabilities of the one fault `refusal` lists, if it lists one
/// fault of what arrives at a part.
fn the_one_fault(refusal: &RunRefusal) -> Option<&CapabilitiesMismatch> {
    match refusal {
        RunRefusal::NotProvided(faults) => match &faults[..] {
            [fault] => Some(&fault.mismatch.capabilities),
            _ => None,
        },
        RunRefusal::PartNotReady { .. } => None,
    }
}

/// A run a chain does not provide, in a row of
/// [`a_run_the_chain_does_not_provide_is_refused_before_any_part_runs`].
#[derive(Clone, Copy)]
enum Unprovided {
    /// One shot more than the most the chain provides.
    ShotsBeyond(u32),
    /// A seed, on a chain not providing seeded runs, for so many shots.
    Seed(u32),
}

/// Runs `unprovided` on `runner`, refused before any part is asked or
/// runs, with the one fault it names, then the run the chain provides
/// instead (the most shots, or no seed), a record each: a row named `row`.
fn refused_before_any_part_runs<
    D: Driver<Output: std::fmt::Debug>,
    E: Executor<Accepts = D::Sends>,
>(
    row: &str,
    mut runner: Runner<D, E>,
    unprovided: Unprovided,
    log: &Log,
) {
    let asked = bits_asked(log);
    let ran = log.count("Bits execute");
    let (refused_run, provided) = match unprovided {
        Unprovided::ShotsBeyond(most) => (
            (ShotSet::ids(0..=most), Some(SEED)),
            (ShotSet::ids(0..most), Some(SEED)),
        ),
        Unprovided::Seed(shots) => (
            (ShotSet::ids(0..shots), Some(SEED)),
            (ShotSet::ids(0..shots), None),
        ),
    };
    let refusal = refused(runner.run(&refused_run.0, refused_run.1));
    let mismatch = the_one_fault(&refusal);
    let named = match unprovided {
        Unprovided::ShotsBeyond(most) => {
            let short = ResourceMismatch::Shots {
                required: most + 1,
                provided: most,
            };
            mismatch.map(|mismatch| &mismatch.resources[..]) == Some(&[short][..])
        }
        Unprovided::Seed(_) => {
            let not_seeded = FeatureMismatch::SeededRuns;
            mismatch.map(|mismatch| &mismatch.features[..]) == Some(&[not_seeded][..])
        }
    };
    assert!(named, "{row}: {refusal:?}");
    assert_eq!(bits_asked(log), asked, "{row}: no part asked");
    assert_eq!(log.count("Bits execute"), ran, "{row}: nothing ran");
    let records = runner
        .run(&provided.0, provided.1)
        .unwrap_or_else(|error| panic!("{row}: the run provided: {error:?}"));
    assert_eq!(records.len(), provided.0.len(), "{row}: a record per shot");
}

/// A run asks of the parts below its driver its shots and, if seeded,
/// seeded runs; a chain that does not provide them refuses the run before
/// any part is asked or runs, and runs what it provides.
///
/// A run's shots arrive at each part below its driver, passed unchanged
/// by each part that does not require otherwise, so a chain runs at most
/// the fewest the parts they arrive at take. In every bracketing, a run of
/// one more is refused, and a run of that many runs. So an adapter may
/// lower the most shots a run may have, and raise it only by requiring
/// fewer below, as a job splitter does, which then runs any.
///
/// A seeded run requires seeded runs below its driver: on an executor
/// stating seeded runs `false`, a seeded run is refused, and a run with no
/// seed runs. An adapter requiring no seeded runs below, as a sampler
/// seeding its own draws does, runs a seeded run over the same executor,
/// in every bracketing.
#[test]
fn a_run_the_chain_does_not_provide_is_refused_before_any_part_runs() {
    use Unprovided::{Seed, ShotsBeyond};
    let log = Log::default();
    let bits = |most| Bits::new(&log).with_resources(batches_of(64).with_shots(most));
    // An executor running 4 shots at most.
    let alone = runner(CoinCopy::new().on(bits(4)));
    refused_before_any_part_runs("the executor alone", alone, ShotsBeyond(4), &log);
    // Bits under a job queue of 4 shots at most: the fewer of the two
    // holds, wherever the brackets go.
    for (bits_most, most) in [(None, 4), (Some(8), 4), (Some(1), 1)] {
        in_each_bracketing!([d, queue, e] from {
            let d = CoinCopy::new();
            let queue = Stating::queue(4, &log);
            let e = bits_most.map_or_else(|| Bits::new(&log), bits);
        }, |chain, brackets| {
            let row = format!("under a queue, over bits of {bits_most:?}, {brackets}");
            refused_before_any_part_runs(&row, runner(chain), ShotsBeyond(most), &log);
        });
    }
    // Bits running 1 shot at most, under a decoder that does not state
    // shots: the bits' limit still holds.
    in_each_bracketing!([d, decoder, e] from {
        let d = CoinCopy::new();
        let decoder = Stating::capping(64, &log);
        let e = bits(1);
    }, |chain, brackets| {
        let row = format!("a decoder with no limit, {brackets}");
        refused_before_any_part_runs(&row, runner(chain), ShotsBeyond(1), &log);
    });
    // Bits running 8 shots at most, under a job queue of 4, under a decoder
    // with no shot limit: the queue's limit, though neither the executor's
    // nor the one the driver meets, holds.
    in_each_bracketing!([d, decoder, queue, e] from {
        let d = CoinCopy::new();
        let decoder = Stating::capping(64, &log);
        let queue = Stating::queue(4, &log);
        let e = bits(8);
    }, |chain, brackets| {
        let row = format!("a queue under a decoder, {brackets}");
        refused_before_any_part_runs(&row, runner(chain), ShotsBeyond(4), &log);
    });
    let bits_without_seeded_runs =
        || Bits::new(&log).with_features(Features::nothing_stated().with_seeded_runs(false));
    // An executor without seeded runs.
    let alone = runner(CoinCopy::new().on(bits_without_seeded_runs()));
    refused_before_any_part_runs("the executor alone, seeded", alone, Seed(4), &log);
    // Bits without seeded runs, under a sampler that seeds its own
    // draws: a seeded run runs.
    in_each_bracketing!([d, sampler, e] from {
        let d = CoinCopy::new();
        let sampler = Stating::seeding_itself(&log);
        let e = bits_without_seeded_runs();
    }, |chain, brackets| {
        (runner(chain).run(&ShotSet::ids(0..2), SEED))
            .unwrap_or_else(|error| panic!("a sampler, {brackets}: {error:?}"));
    });
}

/// A job splitter, requiring runs of 100 shots below whatever run arrives,
/// over bits running 100 at most, directly or under a profiler, in every
/// bracketing, or joined with the driver and wrapped by a part of a
/// user's: a run of 1000 runs, a record per shot, though the fake sends it
/// through whole; without the splitter, the same run is refused.
#[test]
fn a_part_requiring_fewer_shots_below_runs_more_than_the_parts_below_take() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let bits = || Bits::new(&log).with_resources(batches_of(64).with_shots(100));
    let shots = ShotSet::ids(0..1000);
    let runs_all = |records: Result<Vec<Values>, RunError>, row: &str| {
        let records = records.unwrap_or_else(|error| panic!("{row}: {error:?}"));
        assert_eq!(records.len(), 1000, "{row}: a record per shot");
    };
    in_each_bracketing!([d, splitter, e] from {
        let d = CoinCopy::new();
        let splitter = Stating::splitting_runs(100, &log);
        let e = bits();
    }, |chain, brackets| {
        runs_all(runner(chain).run(&shots, SEED), &format!("a splitter, {brackets}"));
    });
    in_each_bracketing!([d, splitter, profiler, e] from {
        let d = CoinCopy::new();
        let splitter = Stating::splitting_runs(100, &log);
        let profiler = Profiler::new(&counted, &log);
        let e = bits();
    }, |chain, brackets| {
        let row = format!("a splitter over a profiler, {brackets}");
        runs_all(runner(chain).run(&shots, SEED), &row);
    });
    let wrapped = Renamed::new(
        CoinCopy::new().through(Stating::splitting_runs(100, &log)),
        "user's driver",
    );
    runs_all(
        runner(wrapped.on(bits())).run(&shots, SEED),
        "a wrapped splitter",
    );
    let refusal = refused(runner(CoinCopy::new().on(bits())).run(&shots, SEED));
    assert!(
        the_one_fault(&refusal).is_some_and(|mismatch| mismatch.resources.len() == 1),
        "without the splitter: {refusal:?}"
    );
}

/// The faults a run's refusal lists, if it lists what arrives at a part
/// that does not provide it.
fn faults_listed(refusal: RunRefusal) -> Vec<CompositionFault> {
    match refusal {
        RunRefusal::NotProvided(faults) => faults,
        RunRefusal::PartNotReady { .. } => panic!("faults listed, not {refusal:?}"),
    }
}

/// A run of 3 shots through a job queue of 2, then a repetition running
/// each shot twice, over bits running 4 at most: the run is refused with
/// every fault the walk finds, the program against the queue and the
/// repetition against the bits, not only the first. Their order and the
/// positions they name are the walk's, pinned in `driver_joins`:
/// `every_fault_is_listed_by_requirer_then_by_provider` and
/// `inserting_a_profiler_changes_only_the_positions_of_the_faults`.
#[test]
fn a_refused_run_names_each_part_whose_requirement_is_not_provided_and_its_provider() {
    let log = Log::default();
    let chain = Program::new(&[("mz", 0)])
        .through(Stating::queue(2, &log))
        .through(Stating::repeating_shots(2, &log))
        .on(Bits::new(&log).with_resources(batches_of(64).with_shots(4)));
    assert_eq!(
        listed(&faults_listed(refused(
            runner(chain).run(&ShotSet::ids(0..3), SEED)
        ))),
        listed(&[
            fault(part::<Program>(0), part::<Stating>(1), short_shots(3, 2)),
            fault(part::<Stating>(2), part::<Bits>(3), short_shots(6, 4)),
        ]),
    );
}
