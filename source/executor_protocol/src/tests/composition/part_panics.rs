// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A part's panics, from its call, its future's poll or drop, or its end
//! of the shots: each is a failure named after the right part, or is
//! raised from the run, wherever the brackets go, and the run's shots are
//! ended on every other part.

use std::any::{Any, type_name};
use std::cell::Cell;
use std::fmt::Debug;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::task::{Context, Waker};

use crate::prelude::*;
use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, EndsBelow, Impatient, Later, Log, Misbehaving, Misbehaviour, Panics,
    PanicsEnding, PanicsInDrop, PanicsWhen, Profiler, Program, Renamed, in_each_bracketing,
};
use crate::{Failure, PartId, RunError, Runner, ShotId, ShotSet};

use super::runner::{SEED, failed, runner};

/// The panic `runner`'s run of shots 0 to 7 raises, which it must.
fn raised<D: Driver, E: Executor>(runner: &mut Runner<D, E>) -> Box<dyn Any + Send> {
    catch_unwind(AssertUnwindSafe(|| {
        runner
            .run(&ShotSet::ids(0..8), SEED)
            .map(|records| records.len())
    }))
    .expect_err("the run raises a panic")
}

/// Coin copies under an adapter that panics in `end_shots`, over a noise,
/// over bits, in a run with no other fault, in each bracketing: the
/// driver's end of its shots fails the run, the protocol ends them again on
/// every part, each part beside the one that panics is ended both times,
/// and `run` then raises the panic.
#[test]
fn a_panic_ending_shots_ends_every_other_part_then_leaves_the_run() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = CoinCopy::new();
            let a = PanicsEnding::new(&log);
            let b = BitFlip::new(0.3, &log);
            let e = Bits::new(&log);
        },
        |chain, brackets| {
            let panic = raised(&mut runner(chain));
            assert_eq!(
                panic.downcast_ref::<&str>(),
                Some(&"end_shots broke"),
                "{brackets}"
            );
            assert_eq!(log.count("BitFlip end_shots, holding 0"), 2, "{brackets}");
            assert_eq!(log.count("Bits end_shots, holding 0"), 2, "{brackets}");
        }
    );
}

/// An adapter whose first `end_shots` panics, under the part that ends the
/// shots, in each bracketing: the panic unwinds through that part's call,
/// so the run fails naming it, not the adapter that panicked. Coin copies,
/// over that adapter, over a noise, over bits, end each shot on the chain
/// below in `drive`: the run names the driver, part 0. A program that
/// never ends its shots, over an adapter that ends them below inside
/// `adapt`, over that adapter, over bits: the run names the ending
/// adapter, part 1, not the driver.
#[test]
fn a_panic_ending_shots_is_named_as_the_part_that_ended_them() {
    /// Checks that `run` failed naming `part` and the end's message.
    fn names<T: Debug>(row: &str, run: Result<T, RunError>, part: &PartId) {
        let error = failed(run);
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part: named, message: Some(message) }
                    if named == part && message == "end_shots broke"
            ),
            "{row}: {error:?}"
        );
    }
    let shots = ShotSet::ids(0..8);
    let driver = PartId::new(0, type_name::<CoinCopy>());
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = CoinCopy::new();
            let a = PanicsEnding::once(&log);
            let b = BitFlip::new(0.3, &log);
            let e = Bits::new(&log);
        },
        |chain, brackets| {
            let row = format!("ended by the driver, {brackets}");
            names(&row, runner(chain).run(&shots, SEED), &driver);
        }
    );
    let adapter = PartId::new(1, type_name::<EndsBelow>());
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = Program::new(&[("mz", 0)]).not_ending();
            let a = EndsBelow::new(&log);
            let b = PanicsEnding::once(&log);
            let e = Bits::new(&log);
        },
        |chain, brackets| {
            let row = format!("ended by an adapter, {brackets}");
            names(&row, runner(chain).run(&shots, SEED), &adapter);
        }
    );
}

/// A program that never ends its shots, over an adapter whose first
/// `end_shots` panics, over a noise, over bits, in each bracketing: the
/// protocol's end is the only one, so the panic is raised again from `run`,
/// once the noise and the bits are ended, and the outputs are lost, where
/// the same panic in the driver's end fails the run naming the driver.
#[test]
fn a_panic_ending_shots_the_protocol_ends_is_raised_from_the_run() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = Program::new(&[("mz", 0)]).not_ending();
            let a = PanicsEnding::once(&log);
            let b = BitFlip::new(0.3, &log);
            let e = Bits::new(&log);
        },
        |chain, brackets| {
            let panic = raised(&mut runner(chain));
            assert_eq!(
                panic.downcast_ref::<&str>(),
                Some(&"end_shots broke"),
                "{brackets}"
            );
            assert_eq!(log.count("BitFlip end_shots, holding 0"), 1, "{brackets}");
            assert_eq!(log.count("Bits end_shots, holding 0"), 1, "{brackets}");
        }
    );
}

/// The run of shots 0 to 7 on `runner`, whose future is dropped once its
/// first poll leaves it pending, by the caller's own panic if
/// `by_a_panic`: the panic that leaves the drop, which it must.
fn dropped_unfinished<D: Driver, E: Executor>(
    runner: &mut Runner<D, E>,
    by_a_panic: bool,
) -> Box<dyn Any + Send> {
    let shots = ShotSet::ids(0..8);
    let mut run = Box::pin(runner.run_async(&shots, SEED));
    let poll = run.as_mut().poll(&mut Context::from_waker(Waker::noop()));
    assert!(poll.is_pending(), "the run is unfinished");
    catch_unwind(AssertUnwindSafe(|| {
        if by_a_panic {
            std::panic::panic_any("the caller broke");
        }
        drop(run);
    }))
    .expect_err("a panic leaves the drop")
}

/// A run whose future the caller's own panic drops, unfinished, while a
/// part would panic a second time: in its `end_shots` (coin copies under
/// a noise, over an adapter that panics ending shots, over bits that
/// answer later; or coin copies over bits that answer later and panic
/// ending shots), or in the destructor of its pending future (coin copies
/// over bits that answer later, whose futures panic once dropped). The
/// run still ends its shots on the other parts, and drops the part's
/// panic rather than raising a second one, which would abort, so the
/// caller's panic leaves.
#[test]
fn a_run_dropped_by_a_panic_ends_its_shots_and_keeps_that_panic() {
    let ending = Log::default();
    let mut panics_ending = runner(
        CoinCopy::new()
            .through(BitFlip::new(0.3, &ending))
            .through(PanicsEnding::new(&ending))
            .on(Later(Bits::new(&ending))),
    );
    let executor_ending = Log::default();
    let mut executor_panics_ending =
        runner(CoinCopy::new().on(Later(Bits::new(&executor_ending).panicking_ending())));
    let dropping = Log::default();
    let mut panics_in_drop =
        runner(CoinCopy::new().on(PanicsInDrop::new(Later(Bits::new(&dropping)), true)));
    for (row, panic, log, ended) in [
        (
            "a part panicking ending shots",
            dropped_unfinished(&mut panics_ending, true),
            &ending,
            &["BitFlip end_shots, holding 0", "Bits end_shots, holding 0"][..],
        ),
        (
            "the executor panicking ending shots",
            dropped_unfinished(&mut executor_panics_ending, true),
            &executor_ending,
            &["Bits end_shots, holding 0"][..],
        ),
        (
            "a part's future panicking in its destructor",
            dropped_unfinished(&mut panics_in_drop, true),
            &dropping,
            &["Bits end_shots, holding 0"],
        ),
    ] {
        assert_eq!(
            panic.downcast_ref::<&str>(),
            Some(&"the caller broke"),
            "{row}"
        );
        for entry in ended {
            assert_eq!(log.count(entry), 1, "{row}: {entry}");
        }
    }
}

/// A run whose future is dropped unfinished, outside any panic, while a
/// part would panic: in its `end_shots` (coin copies over an adapter that
/// panics ending shots, over bits that answer later), or in the
/// destructor of its pending future (coin copies over bits that answer
/// later, whose futures panic once dropped). The run ends its shots on
/// the bits, then raises the part's panic from the drop.
#[test]
fn a_run_dropped_unfinished_raises_a_parts_panic_from_the_drop() {
    let ending = Log::default();
    let mut panics_ending = runner(
        CoinCopy::new()
            .through(PanicsEnding::new(&ending))
            .on(Later(Bits::new(&ending))),
    );
    let dropping = Log::default();
    let mut panics_in_drop =
        runner(CoinCopy::new().on(PanicsInDrop::new(Later(Bits::new(&dropping)), true)));
    for (row, panic, log, message) in [
        (
            "a part panicking ending shots",
            dropped_unfinished(&mut panics_ending, false),
            &ending,
            "end_shots broke",
        ),
        (
            "a part's future panicking in its destructor",
            dropped_unfinished(&mut panics_in_drop, false),
            &dropping,
            "the part's future broke when dropped",
        ),
    ] {
        assert_eq!(panic.downcast_ref::<&str>(), Some(&message), "{row}");
        assert_eq!(log.count("Bits end_shots, holding 0"), 1, "{row}");
    }
}

/// Coin copies over two adapters that panic in `end_shots`, "first" above
/// "second", over bits, in each bracketing: `run` raises the first panic,
/// top to bottom, and the bits are still ended.
#[test]
fn of_two_panics_ending_shots_the_first_is_raised() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = CoinCopy::new();
            let a = PanicsEnding::saying("first", &log);
            let b = PanicsEnding::saying("second", &log);
            let e = Bits::new(&log);
        },
        |chain, brackets| {
            let panic = raised(&mut runner(chain));
            assert_eq!(panic.downcast_ref::<&str>(), Some(&"first"), "{brackets}");
            assert_eq!(log.count("Bits end_shots, holding 0"), 2, "{brackets}");
        }
    );
}

/// A part that panics, as a driver, an executor or an adapter, alone or
/// composed, wherever the brackets go: the run of shots 3 and 1 fails
/// naming shot 3, the first of the batch or the run the panic hit, the
/// part at its position, not a part composed with it, and the panic's
/// message, or none if it is not text.
#[test]
fn a_parts_panic_is_a_failure_naming_it_with_its_message() {
    let shots = ShotSet::ids([3, 1]);
    let program = || Program::new(&[("mz", 0)]);
    let panics = PartId::new(1, type_name::<Panics>());
    let polled = Some("the part broke while it was polled");
    for (when, message) in [
        (
            PanicsWhen::MakingItsFuture,
            Some("the part broke while making its future"),
        ),
        (PanicsWhen::Polled, polled),
    ] {
        let run = runner(program().on(Panics(when))).run(&shots, SEED);
        names_the_panic("an executor", run, &panics, message);
        let log = Log::default();
        let run = runner(Panics(when).on(Bits::new(&log))).run(&shots, SEED);
        let driver = PartId::new(0, type_name::<Panics>());
        names_the_panic("a driver", run, &driver, message);
    }
    let executor = PartId::new(2, type_name::<Panics>());
    in_each_bracketing!(
        [d, a, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = program();
            let a = Profiler::new(&counted, &log);
            let e = Panics(PanicsWhen::Polled);
        },
        |chain, brackets| {
            let row = format!("an executor below a profiler, {brackets}");
            names_the_panic(&row, runner(chain).run(&shots, SEED), &executor, polled);
        }
    );
    let driver = PartId::new(0, type_name::<Panics>());
    in_each_bracketing!(
        [d, a, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = Panics(PanicsWhen::Polled);
            let a = Profiler::new(&counted, &log);
            let e = Bits::new(&log);
        },
        |chain, brackets| {
            let row = format!("a driver above a profiler, {brackets}");
            names_the_panic(&row, runner(chain).run(&shots, SEED), &driver, polled);
        }
    );
    let adapter = PartId::new(1, type_name::<Misbehaving>());
    let formatted = Some("the adapter broke on 1 batches");
    for (misbehaviour, message, what) in [
        (
            Misbehaviour::PanicsWithAString,
            formatted,
            "a formatted message",
        ),
        (Misbehaviour::PanicsWithANumber, None, "a number, not text"),
    ] {
        in_each_bracketing!(
            [d, a, e] from {
                let log = Log::default();
                let d = program();
                let a = Misbehaving::new(misbehaviour);
                let e = Bits::new(&log);
            },
            |chain, brackets| {
                let row = format!("an adapter panicking with {what}, {brackets}");
                names_the_panic(&row, runner(chain).run(&shots, SEED), &adapter, message);
            }
        );
    }
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = program();
            let a = Misbehaving::new(Misbehaviour::PanicsWithAString);
            let b = Profiler::new(&counted, &log);
            let e = Bits::new(&log);
        },
        |chain, brackets| {
            let row = format!("an adapter above a profiler, {brackets}");
            names_the_panic(&row, runner(chain).run(&shots, SEED), &adapter, formatted);
        }
    );
}

/// Checks that `run`, of shots 3 and 1, failed naming shot 3, `part` and
/// `message`, the panic's text if it has one.
fn names_the_panic<T: Debug>(
    row: &str,
    run: Result<T, RunError>,
    part: &PartId,
    message: Option<&str>,
) {
    let error = failed(run);
    assert_eq!(error.shot, ShotId::new(3), "{row}");
    assert!(
        matches!(
            &error.failure,
            Failure::Panicked { part: named, message: given }
                if named == part && given.as_deref() == message
        ),
        "{row}: {error:?}"
    );
}

/// A driver and an executor that panic, each named as a part built at
/// run time is, by what it was built from: the failure names the part by
/// that name, at its position. A joined part held by a part of a user's,
/// which the chain's check does not see into, with a part inside it that
/// panics: the failure names the user's part, at its position. An
/// executor, or an adapter, that panics and answers another name once
/// ready, in each bracketing: the failure of each run names it by the name
/// the `on` that made the runner read.
#[test]
fn a_panic_names_the_part_by_its_name() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let shots = ShotSet::ids([9, 1]);
    let mut executor = runner(
        Program::new(&[("mz", 0)]).on(Renamed::new(Panics(PanicsWhen::Polled), "qodec runtime")),
    );
    let mut driver =
        runner(Renamed::new(Panics(PanicsWhen::Polled), "h2 program").on(Bits::new(&log)));
    let mut held_executor = runner(Program::new(&[("mz", 0)]).on(Renamed::new(
        Profiler::new(&counted, &log).on(Panics(PanicsWhen::Polled)),
        "user's executor",
    )));
    let mut held_driver = runner(
        Renamed::new(
            Panics(PanicsWhen::Polled).through(Profiler::new(&counted, &log)),
            "user's driver",
        )
        .on(Bits::new(&log)),
    );
    for (runner_name, error, position, name) in [
        (
            "executor",
            failed(executor.run(&shots, SEED)),
            1,
            "qodec runtime",
        ),
        ("driver", failed(driver.run(&shots, SEED)), 0, "h2 program"),
        (
            "held executor",
            failed(held_executor.run(&shots, SEED)),
            1,
            "user's executor",
        ),
        (
            "held driver",
            failed(held_driver.run(&shots, SEED)),
            0,
            "user's driver",
        ),
    ] {
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, .. } if *part == PartId::new(position, name)
            ),
            "{runner_name}: {error:?}"
        );
    }
    in_each_bracketing!(
        [d, a, e] from {
            let counted = Rc::new(Cell::new(0));
            let d = Program::new(&[("mz", 0)]);
            let a = Profiler::new(&counted, &log);
            let e = Renamed::new(Panics(PanicsWhen::Polled), "qodec runtime")
                .renamed_once_ready("renamed runtime");
        },
        |chain, brackets| {
            let row = format!("an executor renamed once ready, {brackets}");
            named_as_read_at_on(&row, runner(chain), &PartId::new(2, "qodec runtime"));
        }
    );
    in_each_bracketing!(
        [d, a, e] from {
            let d = Program::new(&[("mz", 0)]);
            let a = Renamed::new(Misbehaving::new(Misbehaviour::PanicsWithAString), "decoder")
                .renamed_once_ready("renamed decoder");
            let e = Bits::new(&log);
        },
        |chain, brackets| {
            let row = format!("an adapter renamed once ready, {brackets}");
            named_as_read_at_on(&row, runner(chain), &PartId::new(1, "decoder"));
        }
    );
}

/// Checks that two runs of shots 3 and 1 on `runner` each fail naming
/// shot 3 and `part`, a row of [`a_panic_names_the_part_by_its_name`].
fn named_as_read_at_on<D: Driver<Output: Debug>, E: Executor>(
    row: &str,
    mut runner: Runner<D, E>,
    part: &PartId,
) {
    for run in ["first run", "second run"] {
        let error = failed(runner.run(&ShotSet::ids([3, 1]), SEED));
        assert_eq!(error.shot, ShotId::new(3), "{row}, {run}");
        assert!(
            matches!(&error.failure, Failure::Panicked { part: named, .. } if named == part),
            "{row}, {run}: {error:?}"
        );
    }
}

/// Two decoders of one name, a profiler above an adapter that panics, in
/// each of the five bracketings of four parts: every failure names the
/// lower one, part 2, so its position tells it from the other.
#[test]
fn a_panic_tells_two_parts_of_one_name_apart_by_position() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = Program::new(&[("mz", 0)]);
            let a = Renamed::new(Profiler::new(&counted, &log), "decoder");
            let b = Renamed::new(
                Misbehaving::new(Misbehaviour::PanicsWithAString),
                "decoder",
            );
            let e = Bits::new(&log);
        },
        |chain, brackets| {
            let error = failed(runner(chain).run(&ShotSet::ids([3, 1]), SEED));
            assert!(
                matches!(
                    &error.failure,
                    Failure::Panicked { part, .. } if *part == PartId::new(2, "decoder")
                ),
                "{brackets}: {error:?}"
            );
        }
    );
}

/// A driver that panics, over a noise, over bits, bracketed both ways:
/// the failed run ends its shots on the noise and the bits.
#[test]
fn a_run_failed_by_a_drivers_panic_ends_its_shots() {
    for when in [PanicsWhen::MakingItsFuture, PanicsWhen::Polled] {
        in_each_bracketing!(
            [d, a, e] from {
                let log = Log::default();
                let d = Panics(when);
                let a = BitFlip::new(0.0, &log);
                let e = Bits::new(&log);
            },
            |chain, brackets| {
                failed(runner(chain).run(&ShotSet::ids([9, 1]), SEED));
                assert_eq!(log.count("BitFlip end_shots, holding 0"), 1, "{brackets}");
                assert_eq!(log.count("Bits end_shots, holding 0"), 1, "{brackets}");
            }
        );
    }
}

/// A program, over a profiler, over a noise, over bits, each a part whose
/// futures may panic once dropped.
type DropChain = (
    PanicsInDrop<Program>,
    PanicsInDrop<Profiler>,
    PanicsInDrop<BitFlip>,
    PanicsInDrop<Bits>,
);

/// The parts of a [`DropChain`], whose part at position `armed` panics in
/// its futures' destructor.
fn drop_chain(armed: usize, log: &Log) -> DropChain {
    let counted = Rc::new(Cell::new(0));
    (
        PanicsInDrop::new(Program::new(&[("mz", 0)]), armed == 0),
        PanicsInDrop::new(Profiler::new(&counted, log), armed == 1),
        PanicsInDrop::new(BitFlip::new(0.0, log), armed == 2),
        PanicsInDrop::new(Bits::new(log), armed == 3),
    )
}

/// A program, over a profiler, over a noise, over bits, in each
/// bracketing, one of them panicking in the destructor of each future it
/// returns, once its answer is given: the run fails naming that part, at
/// its position, with its message, as for a panic while it is polled.
#[test]
fn a_panic_dropping_a_parts_future_is_a_failure_naming_that_part() {
    let parts = [
        type_name::<Program>(),
        type_name::<Profiler>(),
        type_name::<BitFlip>(),
        type_name::<Bits>(),
    ];
    for (armed, name) in parts.into_iter().enumerate() {
        let named = PartId::new(armed, name);
        in_each_bracketing!(
            [d, a, b, e] from {
                let log = Log::default();
                let (d, a, b, e) = drop_chain(armed, &log);
            },
            |chain, brackets| {
                let error = failed(runner(chain).run(&ShotSet::ids([9, 1]), SEED));
                assert_eq!(error.shot, ShotId::new(9), "{named}, {brackets}");
                assert!(
                    matches!(
                        &error.failure,
                        Failure::Panicked { part, message: Some(message) }
                            if *part == named && message == "the part's future broke when dropped"
                    ),
                    "{named}, {brackets}: {error:?}"
                );
            }
        );
    }
}

/// A program over an adapter that stops waiting for its first answer from
/// below, over bits whose futures panic when dropped, bracketed both ways:
/// the bits' future is dropped unfinished by the adapter, not by the
/// protocol, so the panic unwinds through the adapter's call and the run
/// fails naming the adapter, with the bits' message.
#[test]
fn a_panic_dropping_a_future_a_part_above_stopped_waiting_for_names_that_part() {
    let adapter = PartId::new(1, type_name::<Impatient>());
    in_each_bracketing!(
        [d, a, e] from {
            let log = Log::default();
            let d = Program::new(&[("mz", 0)]);
            let a = Impatient::new(&log);
            let e = PanicsInDrop::new(Later(Bits::new(&log)), true);
        },
        |chain, brackets| {
            let error = failed(runner(chain).run(&ShotSet::ids([3, 1]), SEED));
            assert_eq!(error.shot, ShotId::new(3), "{brackets}");
            assert!(
                matches!(
                    &error.failure,
                    Failure::Panicked { part, message: Some(message) }
                        if *part == adapter && message == "the part's future broke when dropped"
                ),
                "{brackets}: {error:?}"
            );
        }
    );
}

/// A program, over a profiler, over two noises, over bits, each a part
/// whose futures may panic when polled, and again when dropped.
type CompoundChain = (
    PanicsInDrop<Program>,
    PanicsInDrop<Profiler>,
    PanicsInDrop<BitFlip>,
    PanicsInDrop<BitFlip>,
    PanicsInDrop<Bits>,
);

/// The parts of a [`CompoundChain`], whose part at position `armed`
/// panics when its futures are polled, and again when they are dropped.
fn compound_chain(armed: usize, log: &Log) -> CompoundChain {
    let counted = Rc::new(Cell::new(0));
    (
        PanicsInDrop::polled_too(Program::new(&[("mz", 0)]), armed == 0),
        PanicsInDrop::polled_too(Profiler::new(&counted, log), armed == 1),
        PanicsInDrop::polled_too(BitFlip::new(0.0, log), armed == 2),
        PanicsInDrop::polled_too(BitFlip::new(0.0, log), armed == 3),
        PanicsInDrop::polled_too(Bits::new(log), armed == 4),
    )
}

/// A program, over a profiler, over two noises, over bits, in each of
/// the 14 bracketings, one of them panicking while its future is polled,
/// and again when that future is dropped: the run fails naming that
/// part, at its position, with the message of its first panic, and the
/// second is dropped; the process does not abort, as it would if a
/// composition's future dropped the part's while the first unwinds.
#[test]
fn a_part_panicking_polled_then_dropped_is_named_with_its_first_panic() {
    let parts = [
        type_name::<Program>(),
        type_name::<Profiler>(),
        type_name::<BitFlip>(),
        type_name::<BitFlip>(),
        type_name::<Bits>(),
    ];
    for (armed, name) in parts.into_iter().enumerate() {
        let named = PartId::new(armed, name);
        in_each_bracketing!(
            [program, profiler, noise, more_noise, bits] from {
                let log = Log::default();
                let (program, profiler, noise, more_noise, bits) = compound_chain(armed, &log);
            },
            |chain, brackets| {
                let error = failed(runner(chain).run(&ShotSet::ids([9, 1]), SEED));
                assert!(
                    matches!(
                        &error.failure,
                        Failure::Panicked { part, message: Some(message) }
                            if *part == named && message == "the part's future broke when polled"
                    ),
                    "{named}, {brackets}: {error:?}"
                );
            }
        );
    }
}

/// A program, over two noises, over bits, in each bracketing, one of them
/// panicking while the future of its `ready` is polled, and again when
/// it is dropped: the run raises the first panic, as it raises any panic
/// in `ready`, and the second is dropped, so the process does not abort.
/// No shot is ended on any part, as no drive started.
#[test]
fn a_panic_in_ready_polled_then_dropped_is_raised_with_its_first_message() {
    for armed in 0..4 {
        in_each_bracketing!(
            [d, a, b, e] from {
                let log = Log::default();
                let d = PanicsInDrop::in_ready(
                    Program::new(&[("mz", 0)]),
                    armed == 0,
                );
                let a = PanicsInDrop::in_ready(BitFlip::new(0.0, &log), armed == 1);
                let b = PanicsInDrop::in_ready(BitFlip::new(0.0, &log), armed == 2);
                let e = PanicsInDrop::in_ready(Bits::new(&log), armed == 3);
            },
            |chain, brackets| {
                let mut runner = runner(chain);
                let raised = catch_unwind(AssertUnwindSafe(|| {
                    runner.run(&ShotSet::ids([3, 1]), SEED).map(|_| ())
                }));
                let payload = raised.expect_err("a panic in `ready` is raised from the run");
                assert_eq!(
                    payload.downcast_ref::<&str>().copied(),
                    Some("the part's future broke when polled"),
                    "part {armed}, {brackets}"
                );
                let ended: Vec<String> = log
                    .entries()
                    .into_iter()
                    .filter(|entry| entry.contains("end_shots"))
                    .collect();
                assert!(ended.is_empty(), "part {armed}, {brackets}: {ended:?}");
            }
        );
    }
}
