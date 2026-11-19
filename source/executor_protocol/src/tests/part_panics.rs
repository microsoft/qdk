// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A part's panics, from its call, its future's poll or drop, or its end
//! of the shots: each is a failure named after the right part, or is
//! raised from the run, wherever the brackets go, and the run's shots are
//! ended on every other part. A panic in a caller's code that a part
//! runs, the `each` it gives to an `_each` form, the one given to
//! `run_each`, or the instructions of the stream it sends, is the
//! caller's.

use std::any::{Any, type_name};
use std::cell::Cell;
use std::fmt::Debug;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::task::{Context, Waker};

use crate::tests::composition::runner::{SEED, failed, runner};
use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, EndsBelow, Impatient, Later, Log, Misbehaving, Misbehaviour,
    OutputsAsRowsCome, Panics, PanicsEnding, PanicsInDrop, PanicsWhen, Profiler, Program, Renamed,
    RowsFromThread, RowsGiven, Sending, in_each_bracketing,
};
use crate::tests::roles::execute::bits;
use crate::{
    Adapter, Drive, DriveError, Driver, Execute, Executor, Failure, Needs, PartId, RunError,
    Runner, ShotId, ShotSet,
};

/// The panic `runner`'s run of shots 0 to 7 raises, which it must.
fn raised<D: Drive, E: Execute>(runner: &mut Runner<D, E>) -> Box<dyn Any + Send> {
    catch_unwind(AssertUnwindSafe(|| {
        runner
            .run(&ShotSet::ids(0..8), SEED)
            .map(|records| records.len())
    }))
    .expect_err("the run raises a panic")
}

/// Coin copies under an adapter that panics in `end_shots`, over a noise,
/// over bits, in a run with no other fault, in each bracketing: the
/// driver's end of its shots fails the run, the runner ends them again on
/// every part, each part beside the one that panics is ended both times,
/// and `run` then raises the panic.
#[test]
fn a_panic_ending_shots_ends_every_other_part_then_leaves_the_run() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = Driver::new(CoinCopy::new());
            let a = Adapter::new(PanicsEnding::new(&log));
            let b = Adapter::new(BitFlip::new(0.3, &log));
            let e = Executor::new(Bits::new(&log));
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
            let d = Driver::new(CoinCopy::new());
            let a = Adapter::new(PanicsEnding::once(&log));
            let b = Adapter::new(BitFlip::new(0.3, &log));
            let e = Executor::new(Bits::new(&log));
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
            let d = Driver::new(Program::new(&[("mz", 0)]).not_ending());
            let a = Adapter::new(EndsBelow::new(&log));
            let b = Adapter::new(PanicsEnding::once(&log));
            let e = Executor::new(Bits::new(&log));
        },
        |chain, brackets| {
            let row = format!("ended by an adapter, {brackets}");
            names(&row, runner(chain).run(&shots, SEED), &adapter);
        }
    );
}

/// A program that never ends its shots, over an adapter whose first
/// `end_shots` panics, over a noise, over bits, in each bracketing: the
/// runner's end is the only one, so the panic is raised again from `run`,
/// once the noise and the bits are ended, and the outputs are lost, where
/// the same panic in the driver's end fails the run naming the driver.
#[test]
fn a_panic_ending_shots_the_runner_ends_is_raised_from_the_run() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = Driver::new(Program::new(&[("mz", 0)]).not_ending());
            let a = Adapter::new(PanicsEnding::once(&log));
            let b = Adapter::new(BitFlip::new(0.3, &log));
            let e = Executor::new(Bits::new(&log));
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
fn dropped_unfinished<D: Drive, E: Execute>(
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
/// answer later), or in the destructor of its pending future (coin copies
/// over bits that answer later, whose futures panic once dropped). The
/// run still ends its shots on the other parts, and drops the part's
/// panic rather than raising a second one, which would abort, so the
/// caller's panic leaves.
#[test]
fn a_run_dropped_by_a_panic_ends_its_shots_and_keeps_that_panic() {
    let ending = Log::default();
    let mut panics_ending = runner(
        Driver::new(CoinCopy::new())
            >> Adapter::new(BitFlip::new(0.3, &ending))
            >> Adapter::new(PanicsEnding::new(&ending))
            >> Executor::new(Later(Bits::new(&ending))),
    );
    let dropping = Log::default();
    let mut panics_in_drop = runner(
        Driver::new(CoinCopy::new())
            >> Executor::new(PanicsInDrop::new(Later(Bits::new(&dropping)), true)),
    );
    for (row, panic, log, ended) in [
        (
            "a part panicking ending shots",
            dropped_unfinished(&mut panics_ending, true),
            &ending,
            &["BitFlip end_shots, holding 0", "Bits end_shots, holding 0"][..],
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
        Driver::new(CoinCopy::new())
            >> Adapter::new(PanicsEnding::new(&ending))
            >> Executor::new(Later(Bits::new(&ending))),
    );
    let dropping = Log::default();
    let mut panics_in_drop = runner(
        Driver::new(CoinCopy::new())
            >> Executor::new(PanicsInDrop::new(Later(Bits::new(&dropping)), true)),
    );
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
            let d = Driver::new(CoinCopy::new());
            let a = Adapter::new(PanicsEnding::saying("first", &log));
            let b = Adapter::new(PanicsEnding::saying("second", &log));
            let e = Executor::new(Bits::new(&log));
        },
        |chain, brackets| {
            let panic = raised(&mut runner(chain));
            assert_eq!(panic.downcast_ref::<&str>(), Some(&"first"), "{brackets}");
            assert_eq!(log.count("Bits end_shots, holding 0"), 2, "{brackets}");
        }
    );
}

/// A part that panics, as a driver, an executor, in an `execute` or an
/// `execute_each`, or an adapter, alone or composed, wherever the brackets
/// go: the run of shots 3 and 1 fails naming shot 3, the first of the
/// batch or the run the panic hit, the part at its position, not a part
/// composed with it, and the panic's message, or none if it is not text.
#[test]
fn a_parts_panic_is_a_failure_naming_it_with_its_message() {
    let shots = ShotSet::ids([3, 1]);
    let program = || Driver::new(Program::new(&[("mz", 0)]));
    let panics = PartId::new(1, type_name::<Panics>());
    let polled = Some("the part broke while it was polled");
    for (when, message) in [
        (
            PanicsWhen::MakingItsFuture,
            Some("the part broke while making its future"),
        ),
        (PanicsWhen::Polled, polled),
    ] {
        let run = runner(program() >> Executor::new(Panics(when))).run(&shots, SEED);
        names_the_panic("an executor", run, &panics, message);
        let log = Log::default();
        let run =
            runner(Driver::new(Panics(when)) >> Executor::new(Bits::new(&log))).run(&shots, SEED);
        let driver = PartId::new(0, type_name::<Panics>());
        names_the_panic("a driver", run, &driver, message);
        let (given, log) = (RowsGiven::default(), Log::default());
        let run = runner(
            Driver::new(OutputsAsRowsCome::new(&given, &log)) >> Executor::new(Panics(when)),
        )
        .run(&shots, SEED);
        names_the_panic("an executor, by `execute_each`", run, &panics, message);
    }
    let executor = PartId::new(2, type_name::<Panics>());
    in_each_bracketing!(
        [d, a, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = program();
            let a = Adapter::new(Profiler::new(&counted, &log));
            let e = Executor::new(Panics(PanicsWhen::Polled));
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
            let d = Driver::new(Panics(PanicsWhen::Polled));
            let a = Adapter::new(Profiler::new(&counted, &log));
            let e = Executor::new(Bits::new(&log));
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
                let a = Adapter::new(Misbehaving::new(misbehaviour));
                let e = Executor::new(Bits::new(&log));
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
            let a = Adapter::new(Misbehaving::new(Misbehaviour::PanicsWithAString));
            let b = Adapter::new(Profiler::new(&counted, &log));
            let e = Executor::new(Bits::new(&log));
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
                Failure::Panicked { part, .. } if *part == PartId::new(position, name)
            ),
            "{runner_name}: {error:?}"
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
            let d = Driver::new(Program::new(&[("mz", 0)]));
            let a = Adapter::new(Renamed::new(Profiler::new(&counted, &log), "decoder"));
            let b = Adapter::new(Renamed::new(
                Misbehaving::new(Misbehaviour::PanicsWithAString),
                "decoder",
            ));
            let e = Executor::new(Bits::new(&log));
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
                let d = Driver::new(Panics(when));
                let a = Adapter::new(BitFlip::new(0.0, &log));
                let e = Executor::new(Bits::new(&log));
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
        Adapter::new(PanicsInDrop::new(BitFlip::new(0.0, log), armed == 2)),
        Executor::new(PanicsInDrop::new(Bits::new(log), armed == 3)),
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
            let d = Driver::new(Program::new(&[("mz", 0)]));
            let a = Adapter::new(Impatient::new(&log));
            let e = Executor::new(PanicsInDrop::new(Later(Bits::new(&log)), true));
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
    Driver<PanicsInDrop<Program>>,
    Adapter<PanicsInDrop<Profiler>>,
    Adapter<PanicsInDrop<BitFlip>>,
    Adapter<PanicsInDrop<BitFlip>>,
    Executor<PanicsInDrop<Bits>>,
);

/// A [`CompoundChain`] whose parts are each [`Renamed`] for the type it
/// wraps, so that their `_each` forms are the defaults. The default
/// `adapt_each` calls `execute` below, so the parts below the profiler
/// are reached by their plain forms.
type CompoundChainByDefaults = (
    Driver<Renamed<PanicsInDrop<Program>>>,
    Adapter<Renamed<PanicsInDrop<Profiler>>>,
    Adapter<Renamed<PanicsInDrop<BitFlip>>>,
    Adapter<Renamed<PanicsInDrop<BitFlip>>>,
    Executor<Renamed<PanicsInDrop<Bits>>>,
);

/// The parts of a [`CompoundChain`], before each takes its role.
type CompoundParts = (
    PanicsInDrop<Program>,
    PanicsInDrop<Profiler>,
    PanicsInDrop<BitFlip>,
    PanicsInDrop<BitFlip>,
    PanicsInDrop<Bits>,
);

/// The [`CompoundParts`], whose part at position `armed` panics when its
/// futures are polled, and again when they are dropped; the program sends
/// as `sending` says.
fn compound_parts(armed: usize, sending: Sending, log: &Log) -> CompoundParts {
    let counted = Rc::new(Cell::new(0));
    (
        PanicsInDrop::polled_too(Program::new(&[("mz", 0)]).sending_as(sending), armed == 0),
        PanicsInDrop::polled_too(Profiler::new(&counted, log), armed == 1),
        PanicsInDrop::polled_too(BitFlip::new(0.0, log), armed == 2),
        PanicsInDrop::polled_too(BitFlip::new(0.0, log), armed == 3),
        PanicsInDrop::polled_too(Bits::new(log), armed == 4),
    )
}

/// The [`CompoundChain`] of [`compound_parts`].
fn compound_chain(armed: usize, sending: Sending, log: &Log) -> CompoundChain {
    let (program, profiler, noise, more_noise, bits) = compound_parts(armed, sending, log);
    (
        Driver::new(program),
        Adapter::new(profiler),
        Adapter::new(noise),
        Adapter::new(more_noise),
        Executor::new(bits),
    )
}

/// The [`CompoundChainByDefaults`] of [`compound_parts`], whose program
/// calls `execute_each`.
fn compound_chain_by_defaults(armed: usize, log: &Log) -> CompoundChainByDefaults {
    let (program, profiler, noise, more_noise, bits) = compound_parts(armed, Sending::Each, log);
    (
        Driver::new(Renamed::new(program, type_name::<Program>())),
        Adapter::new(Renamed::new(profiler, type_name::<Profiler>())),
        Adapter::new(Renamed::new(noise, type_name::<BitFlip>())),
        Adapter::new(Renamed::new(more_noise, type_name::<BitFlip>())),
        Executor::new(Renamed::new(bits, type_name::<Bits>())),
    )
}

/// A program, over a profiler, over two noises, over bits, in each of
/// the 14 bracketings, one of them panicking while its future is polled,
/// and again when that future is dropped; in a run whose program calls
/// `execute`, or `execute_stream` (cut into pieces by the defaults), and
/// in a `run_each` whose program calls `execute_each`, whose parts give
/// their `_each` forms themselves, or by the defaults: every part's, or,
/// in a program over a profiler over bits, the bits' alone, below parts
/// that give rows as they are final (`BitFlip` does not). The run fails
/// naming that part, at its position, with the message of its first
/// panic, and the second is dropped. The process does not abort, as it
/// would if the protocol (the runner, a composition's future, or a
/// default `_each` or `_stream` form) dropped the part's future while the
/// first panic unwinds; such an abort kills the test binary (`SIGABRT`),
/// which is this test failing.
#[test]
fn a_part_panicking_polled_then_dropped_is_named_with_its_first_panic() {
    let parts = [
        type_name::<Program>(),
        type_name::<Profiler>(),
        type_name::<BitFlip>(),
        type_name::<BitFlip>(),
        type_name::<Bits>(),
    ];
    let shots = ShotSet::ids([9, 1]);
    let is_first_panic = |error: DriveError, named: &PartId, run: &str| {
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if part == named && message == "the part's future broke when polled"
            ),
            "{named}, {run}: {error:?}"
        );
    };
    for (armed, name) in parts.into_iter().enumerate() {
        let named = PartId::new(armed, name);
        for (by, sending) in [
            ("execute", Sending::Whole),
            ("execute_each", Sending::Each),
            ("execute_stream", Sending::stream()),
        ] {
            in_each_bracketing!(
                [program, profiler, noise, more_noise, bits] from {
                    let log = Log::default();
                    let (program, profiler, noise, more_noise, bits) =
                        compound_chain(armed, sending.clone(), &log);
                },
                |chain, brackets| {
                    let mut chain = runner(chain);
                    let error = if matches!(sending, Sending::Each) {
                        failed(chain.run_each(&shots, SEED, &mut |_, _| {}))
                    } else {
                        failed(chain.run(&shots, SEED))
                    };
                    is_first_panic(error, &named, &format!("{brackets}, by {by}"));
                }
            );
        }
        in_each_bracketing!(
            [program, profiler, noise, more_noise, bits] from {
                let log = Log::default();
                let (program, profiler, noise, more_noise, bits) =
                    compound_chain_by_defaults(armed, &log);
            },
            |chain, brackets| {
                let error = failed(runner(chain).run_each(&shots, SEED, &mut |_, _| {}));
                is_first_panic(error, &named, &format!("{brackets}, by the defaults"));
            }
        );
    }
    let parts = [
        type_name::<Program>(),
        type_name::<Profiler>(),
        type_name::<Bits>(),
    ];
    for (armed, name) in parts.into_iter().enumerate() {
        let named = PartId::new(armed, name);
        in_each_bracketing!(
            [program, profiler, bits] from {
                let log = Log::default();
                let counted = Rc::new(Cell::new(0));
                let program = Driver::new(PanicsInDrop::polled_too(
                    Program::new(&[("mz", 0)]).sending_as(Sending::Each),
                    armed == 0,
                ));
                let profiler = Adapter::new(PanicsInDrop::polled_too(
                    Profiler::new(&counted, &log),
                    armed == 1,
                ));
                let bits = Executor::new(Renamed::new(
                    PanicsInDrop::polled_too(Bits::new(&log), armed == 2),
                    type_name::<Bits>(),
                ));
            },
            |chain, brackets| {
                let error = failed(runner(chain).run_each(&shots, SEED, &mut |_, _| {}));
                is_first_panic(error, &named, &format!("{brackets}, over the bits' default"));
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
                let d = Driver::new(PanicsInDrop::in_ready(
                    Program::new(&[("mz", 0)]),
                    armed == 0,
                ));
                let a = Adapter::new(PanicsInDrop::in_ready(BitFlip::new(0.0, &log), armed == 1));
                let b = Adapter::new(PanicsInDrop::in_ready(BitFlip::new(0.0, &log), armed == 2));
                let e = Executor::new(PanicsInDrop::in_ready(Bits::new(&log), armed == 3));
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

/// A driver whose `each`, given to its `execute_each`, panics on the first
/// row, through a profiler, wherever the brackets go: the run fails naming
/// the driver, whose code panicked, not the bits that called `each`; `each`
/// is not called again, and the bits still end every shot. On an executor
/// that panics too, after giving that row, or that fails after it, the run
/// still names the driver. A profiler whose own `each`, given to the bits'
/// `execute_each`, panics: the run fails naming the profiler.
#[test]
fn a_panic_in_a_parts_each_names_that_part() {
    let driver = PartId::new(0, type_name::<OutputsAsRowsCome>());
    let counted = Rc::new(Cell::new(0));
    in_each_bracketing!([d, a, e] from {
        let (given, log) = (RowsGiven::default(), Log::default());
        let d = Driver::new(OutputsAsRowsCome::new(&given, &log).panicking_in_each());
        let a = Adapter::new(Profiler::new(&counted, &log));
        let e = Executor::new(Bits::new(&log));
    }, |chain, brackets| {
        let error = failed(runner(chain).run(&ShotSet::ids(0..4), SEED));
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if *part == driver && message == "the driver's `each` panicked"
            ),
            "{brackets}: {error:?}"
        );
        assert_eq!(given.borrow().len(), 1, "{brackets}: `each` called once");
        assert_eq!(
            log.count("Bits end_shots, holding 0"),
            1,
            "{brackets}: every shot ended"
        );
    });
    let names_the_driver = |error: &DriveError| {
        matches!(
            &error.failure,
            Failure::Panicked { part, message: Some(message) }
                if *part == driver && message == "the driver's `each` panicked"
        )
    };
    let (given, log) = (RowsGiven::default(), Log::default());
    let error = failed(
        runner(
            Driver::new(OutputsAsRowsCome::new(&given, &log).panicking_in_each())
                >> Executor::new(Panics(PanicsWhen::PolledAfterARow)),
        )
        .run(&ShotSet::ids(0..4), SEED),
    );
    assert!(
        names_the_driver(&error),
        "an executor panicking too: {error:?}"
    );
    let error = failed(
        runner(
            Driver::new(OutputsAsRowsCome::new(&given, &log).panicking_in_each())
                >> Executor::new(RowsFromThread::new(Bits::new(&log), &log).stopping_after(1)),
        )
        .run(&ShotSet::ids(0..4), SEED),
    );
    assert!(
        names_the_driver(&error),
        "an executor failing too: {error:?}"
    );
    let adapter = PartId::new(1, type_name::<Profiler>());
    in_each_bracketing!([d, a, e] from {
        let log = Log::default();
        let d = Driver::new(CoinCopy::new());
        let a = Adapter::new(Profiler::new(&counted, &log).panicking_in_its_each());
        let e = Executor::new(Bits::new(&log));
    }, |chain, brackets| {
        let error = failed(runner(chain).run(&ShotSet::ids(0..4), SEED));
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if *part == adapter && message == "the adapter's `each` panicked"
            ),
            "an adapter's `each`, {brackets}: {error:?}"
        );
    });
}

/// A driver whose stream of instructions panics when pulled for one of
/// its instructions, sent straight to bits that consume it themselves, or
/// through a profiler that maps the stream on, over such bits or over bits
/// behind a renaming whose stream is cut into pieces by the default,
/// wherever the brackets go: the run fails naming the driver, whose code
/// panicked, not the part that pulled the instruction, and the bits still
/// end every shot. So it does when the bits then fail, or panic, on the
/// stream cut short, as a receiver may: their answer does not hide the
/// sender's panic. And so it does when the stream is checked, even when
/// the panic ends a piece that is refused, the first or a later one (bits
/// taking 2 instructions a batch, the first piece clean): the refusal does
/// not hide it either.
#[test]
fn a_panic_in_a_senders_stream_names_the_sender() {
    let driver = PartId::new(0, type_name::<Program>());
    let counted = Rc::new(Cell::new(0));
    let names_the_driver = |error: &DriveError, log: &Log, case: &str| {
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if *part == driver && message.contains("stream")
            ),
            "{case}: {error:?}"
        );
        assert_eq!(
            log.count("Bits end_shots, holding 0"),
            1,
            "{case}: every shot ended"
        );
    };
    let refused_first_piece = [("x", 0), ("x", 5), ("mz", 0)];
    let refused_second_piece = [("x", 0), ("x", 0), ("x", 5), ("mz", 0)];
    let answering_ok: fn(Bits) -> Bits = |bits| bits;
    let program_text = &[("x", 0), ("mz", 0)][..];
    for (case, text, checked, panics_at, most, answering) in [
        ("unchecked", program_text, false, 1, None, answering_ok),
        (
            "unchecked, the bits then failing",
            program_text,
            false,
            1,
            None,
            Bits::failing,
        ),
        (
            "unchecked, the bits then panicking",
            program_text,
            false,
            1,
            None,
            Bits::panicking,
        ),
        ("checked", program_text, true, 1, None, answering_ok),
        (
            "checked, its first piece refused",
            &refused_first_piece,
            true,
            2,
            None,
            answering_ok,
        ),
        (
            "checked, its second piece refused",
            &refused_second_piece,
            true,
            3,
            Some(2),
            answering_ok,
        ),
    ] {
        let bits = |log: &Log| answering(bits(log, most));
        let program = || streaming_until_it_panics(text, panics_at, checked);
        let log = Log::default();
        let error = failed(
            runner(Driver::new(program()) >> Executor::new(bits(&log)))
                .run(&ShotSet::ids(0..4), SEED),
        );
        names_the_driver(&error, &log, &format!("{case}, straight to the bits"));
        in_each_bracketing!([d, a, e] from {
            let log = Log::default();
            let d = Driver::new(program());
            let a = Adapter::new(Profiler::new(&counted, &log));
            let e = Executor::new(bits(&log));
        }, |chain, brackets| {
            let error = failed(runner(chain).run(&ShotSet::ids(0..4), SEED));
            names_the_driver(&error, &log, &format!("{case}, bits, {brackets}"));
        });
        in_each_bracketing!([d, a, e] from {
            let log = Log::default();
            let d = Driver::new(program());
            let a = Adapter::new(Profiler::new(&counted, &log));
            let e = Executor::new(Renamed::new(bits(&log), type_name::<Bits>()));
        }, |chain, brackets| {
            let error = failed(runner(chain).run(&ShotSet::ids(0..4), SEED));
            names_the_driver(&error, &log, &format!("{case}, bits by the default, {brackets}"));
        });
    }
}

/// A program of `text` on one qubit, sent as one stream, checked if
/// `checked`, that panics when pulled for its instruction at `panics_at`.
fn streaming_until_it_panics(text: &[(&str, u32)], panics_at: usize, checked: bool) -> Program {
    let program = Program::new(text)
        .needing(Needs::qubits(1))
        .sending_as(Sending::Stream {
            produced: Rc::default(),
            panics_at: Some(panics_at),
        });
    if checked {
        program.with_checked_batches()
    } else {
        program
    }
}

/// A driver whose `each` panics on the first row a worker thread hands
/// over, and that drops its `execute_each` once that row was given: the
/// call never answers, so the panic kept for it is dropped with it, and
/// the run answers `Ok`, with `each` called once.
#[test]
fn a_panic_in_a_parts_each_is_dropped_with_a_call_dropped_before_it_answers() {
    let (given, log) = (RowsGiven::default(), Log::default());
    let records = runner(
        Driver::new(
            OutputsAsRowsCome::new(&given, &log)
                .panicking_in_each()
                .dropping_its_call(),
        ) >> Executor::new(RowsFromThread::new(Bits::new(&log), &log)),
    )
    .run(&ShotSet::ids(0..4), SEED)
    .expect("the run answers `Ok`");
    assert_eq!(records.len(), 4, "a record per shot");
    assert_eq!(given.borrow().len(), 1, "`each` called once");
}

/// `run_each` with an `each` that panics on the first output, in a run
/// that goes on to its end, in one whose executor fails after that output,
/// and in one whose driver panics on the next row: the panic is raised from
/// the run, not taken for the driver's nor replaced by the failure or the
/// driver's panic; `each` is not called again, and every shot is ended
/// before the panic is raised.
#[test]
fn a_panic_in_the_runs_each_is_raised_once_the_shots_end() {
    let (given, log) = (RowsGiven::default(), Log::default());
    let ends =
        runner(Driver::new(OutputsAsRowsCome::new(&given, &log)) >> Executor::new(Bits::new(&log)));
    raises_the_panic_in_each("a run that ends", ends, (&given, 6), &log);
    let (given, log) = (RowsGiven::default(), Log::default());
    let fails = runner(
        Driver::new(OutputsAsRowsCome::new(&given, &log))
            >> Executor::new(RowsFromThread::new(Bits::new(&log), &log).stopping_after(1)),
    );
    raises_the_panic_in_each("a run that fails", fails, (&given, 1), &log);
    let (given, log) = (RowsGiven::default(), Log::default());
    let driver_panics = runner(
        Driver::new(OutputsAsRowsCome::new(&given, &log).panicking_in_each_after(1))
            >> Executor::new(Bits::new(&log)),
    );
    let case = "a run whose driver panics";
    raises_the_panic_in_each(case, driver_panics, (&given, 2), &log);
}

/// `run_each` with an `each` that panics on the first output, over an
/// adapter whose first `end_shots` panics, in a run that ends (of a
/// program that never ends its shots) and in one whose executor fails
/// after that output (of a driver that ends its shots only once its call
/// answers): the runner's end of the shots comes first, so the end's panic
/// is raised from the run, and `each`'s is dropped.
#[test]
fn a_panic_in_the_runners_end_is_raised_over_one_in_the_runs_each() {
    let log = Log::default();
    let ends = runner(
        Driver::new(Program::new(&[("mz", 0)]).not_ending())
            >> Adapter::new(PanicsEnding::once(&log))
            >> Executor::new(Bits::new(&log)),
    );
    raises_the_ends_panic("a run that ends", ends, &log);
    let (given, log) = (RowsGiven::default(), Log::default());
    let fails = runner(
        Driver::new(OutputsAsRowsCome::new(&given, &log))
            >> Adapter::new(PanicsEnding::once(&log))
            >> Executor::new(RowsFromThread::new(Bits::new(&log), &log).stopping_after(1)),
    );
    raises_the_ends_panic("a run that fails", fails, &log);
}

/// Checks that `runner`'s `run_each` of shots 0 to 3, whose `each` panics
/// on the first output, raises the panic of the runner's end of the shots
/// instead, once every shot was ended.
fn raises_the_ends_panic<D: Drive, E: Execute>(case: &str, mut runner: Runner<D, E>, log: &Log) {
    let calls = Cell::new(0);
    let raised = catch_unwind(AssertUnwindSafe(|| {
        runner.run_each(&ShotSet::ids(0..4), SEED, &mut |_, _| {
            calls.set(calls.get() + 1);
            panic!("the run's `each` panicked");
        })
    }));
    let payload = raised.expect_err("a panic is raised from the run");
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"end_shots broke"),
        "{case}"
    );
    assert_eq!(calls.get(), 1, "{case}: `each` called once");
    assert_eq!(
        log.count("Bits end_shots, holding 0"),
        1,
        "{case}: every shot ended"
    );
}

/// Checks that `runner`'s `run_each` of shots 0 to 5, whose `each` panics
/// on the first output, raises that panic after its driver was given
/// `rows` rows and every shot was ended.
fn raises_the_panic_in_each<D: Drive, E: Execute>(
    case: &str,
    mut runner: Runner<D, E>,
    (given, rows): (&RowsGiven, usize),
    log: &Log,
) {
    let calls = Cell::new(0);
    let raised = catch_unwind(AssertUnwindSafe(|| {
        runner.run_each(&ShotSet::ids(0..6), SEED, &mut |_, _| {
            calls.set(calls.get() + 1);
            panic!("the run's `each` panicked");
        })
    }));
    let payload = raised.expect_err("the panic is raised from the run");
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"the run's `each` panicked"),
        "{case}"
    );
    assert_eq!(calls.get(), 1, "{case}: `each` called once");
    assert_eq!(given.borrow().len(), rows, "{case}: the rows given");
    assert_eq!(
        log.entries().last().map(String::as_str),
        Some("Bits end_shots, holding 0"),
        "{case}: every shot ended before the panic was raised"
    );
}
