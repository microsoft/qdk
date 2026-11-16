// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The failures that end a run: a part's error at the shot it names,
//! readouts misshapen for their batch, and a part's panic, from its call,
//! its future's poll or drop, or its end of the shots. Each is named after
//! the right part wherever the brackets go, and the run's shots are ended.

use std::any::{Any, type_name};
use std::cell::Cell;
use std::fmt::Debug;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::task::{Context, Waker};

use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, EndsBelow, Impatient, Later, Log, Misbehaving, Misbehaviour, Misshape,
    Misshapen, Panics, PanicsEnding, PanicsInDrop, PanicsWhen, Profiler, Program, Renamed, Trap,
    in_each_bracketing,
};
use crate::{
    Adapter, ChainPart, Drive, Driver, Execute, Executor, Failure, RunError, Runner, ShapeFault,
    ShotId, ShotSet,
};

use super::{SEED, failed, runner};

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

/// The program measuring qubit 0 in one batch, checked if `checked`.
fn measure(checked: bool) -> Driver<Program> {
    let program = Program::new(&[("mz", 0)]);
    Driver::new(if checked {
        program.with_checked_batches()
    } else {
        program
    })
}

/// An executor answering readouts misshapen for the run's one batch, of
/// one `mz` on shots 6, 1 and 2: the run fails naming the batch's first
/// shot, 6, with the shape fault; the count of readouts and of rows is
/// checked whether the batch is checked or not, the values per row only
/// if it is.
#[test]
fn an_answer_misshapen_for_its_batch_is_a_failure() {
    let both: &[bool] = &[false, true];
    let checked: &[bool] = &[true];
    for (row, misshape, sendings, expected) in [
        (
            "a readouts more than the batches",
            Misshape::ExtraReadouts,
            both,
            ShapeFault::Batches {
                expected: 1,
                got: 2,
            },
        ),
        (
            "no readouts for the one batch",
            Misshape::MissingReadouts,
            both,
            ShapeFault::Batches {
                expected: 1,
                got: 0,
            },
        ),
        (
            "a row fewer than the batch's shots",
            Misshape::MissingRow,
            both,
            ShapeFault::Rows {
                batch: 0,
                expected: 3,
                got: 2,
            },
        ),
        (
            "a fourth row for 3 shots",
            Misshape::ExtraRow,
            both,
            ShapeFault::Rows {
                batch: 0,
                expected: 3,
                got: 4,
            },
        ),
        (
            "a value more than the batch's readout slots",
            Misshape::ExtraValue,
            checked,
            ShapeFault::Slots {
                batch: 0,
                expected: 1,
                got: 2,
            },
        ),
        (
            "no value for the batch's one readout slot",
            Misshape::MissingValue,
            checked,
            ShapeFault::Slots {
                batch: 0,
                expected: 1,
                got: 0,
            },
        ),
    ] {
        for &checked in sendings {
            let row = format!("{row}, checked: {checked}");
            let mut runner = runner(measure(checked) >> Executor::new(Misshapen(misshape)));
            let run = runner.run(&ShotSet::ids([6, 1, 2]), SEED);
            assert_eq!(shape_fault(&row, run, 6), expected, "{row}");
        }
    }
}

/// Two checked batches in one `execute`, shots 4 and 2 then 9 and 7, each
/// measuring qubit 0, on an executor misshaping its answer: the run fails
/// naming the first shot of the batch at fault, 9 for the second batch's
/// rows, or, for a count of readouts, which no one batch is at
/// fault for, the first batch's, 4.
#[test]
fn a_misshapen_answer_to_two_batches_names_the_batch_at_fault() {
    for (row, misshape, shot, expected) in [
        (
            "a row fewer than the second batch's shots",
            Misshape::MissingRow,
            9,
            ShapeFault::Rows {
                batch: 1,
                expected: 2,
                got: 1,
            },
        ),
        (
            "no readouts for the second batch",
            Misshape::MissingReadouts,
            4,
            ShapeFault::Batches {
                expected: 2,
                got: 1,
            },
        ),
    ] {
        let mut runner = runner(
            Driver::new(Program::halves(&[("mz", 0)], &[("mz", 0)]).with_checked_batches())
                >> Executor::new(Misshapen(misshape)),
        );
        let run = runner.run(&ShotSet::ids([4, 2, 9, 7]), SEED);
        assert_eq!(shape_fault(row, run, shot), expected, "{row}");
    }
}

/// The shape fault of the failure `run` ended in, checking it names `shot`.
fn shape_fault<T: Debug>(row: &str, run: Result<T, RunError>, shot: u32) -> ShapeFault {
    let error = failed(run);
    assert_eq!(error.shot, ShotId::new(shot), "{row}");
    let Failure::Part(fault) = error.failure else {
        panic!("{row}: the part's error, not {:?}", error.failure);
    };
    *fault.downcast_ref::<ShapeFault>().expect("a shape fault")
}

/// Coin copies under a noise, above a trap on shot 5, over bits, in each
/// bracketing: a run's shots are ended on the noise, then the trap, then
/// the bits, both when the run succeeds, by the driver and then by the
/// runner, and when it fails, by the runner.
#[test]
fn a_runs_shots_are_ended_top_to_bottom_even_when_it_fails() {
    /// The ends `log` holds, in order.
    fn ended(log: &Log) -> Vec<String> {
        log.entries()
            .into_iter()
            .filter(|entry| entry.contains("end_shots"))
            .collect()
    }
    let top_to_bottom = [
        "BitFlip end_shots, holding 0",
        "Trap end_shots",
        "Bits end_shots, holding 0",
    ];
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = Driver::new(CoinCopy::new());
            let a = Adapter::new(BitFlip::new(300, &log));
            let b = Adapter::new(Trap::on(5).logging(&log));
            let e = Executor::new(Bits::new(&log));
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
                "{brackets}, failed"
            );
        }
    );
}

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
            let b = Adapter::new(BitFlip::new(300, &log));
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
    fn names<T: Debug>(row: &str, run: Result<T, RunError>, part: &ChainPart) {
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
    let driver = ChainPart::new(0, type_name::<CoinCopy>());
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = Driver::new(CoinCopy::new());
            let a = Adapter::new(PanicsEnding::once(&log));
            let b = Adapter::new(BitFlip::new(300, &log));
            let e = Executor::new(Bits::new(&log));
        },
        |chain, brackets| {
            let row = format!("ended by the driver, {brackets}");
            names(&row, runner(chain).run(&shots, SEED), &driver);
        }
    );
    let adapter = ChainPart::new(1, type_name::<EndsBelow>());
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
            let b = Adapter::new(BitFlip::new(300, &log));
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

/// A part that panics, as a driver, an executor or an adapter, alone or
/// composed, wherever the brackets go: the run of shots 3 and 1 fails
/// naming shot 3, the first of the batch or the run the panic hit, the
/// part at its position, not a part composed with it, and the panic's
/// message, or none if it is not text.
#[test]
fn a_parts_panic_is_a_failure_naming_it_with_its_message() {
    let shots = ShotSet::ids([3, 1]);
    let program = || Driver::new(Program::new(&[("mz", 0)]));
    let panics = ChainPart::new(1, type_name::<Panics>());
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
        let driver = ChainPart::new(0, type_name::<Panics>());
        names_the_panic("a driver", run, &driver, message);
    }
    let executor = ChainPart::new(2, type_name::<Panics>());
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
    let driver = ChainPart::new(0, type_name::<Panics>());
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
    let adapter = ChainPart::new(1, type_name::<Misbehaving>());
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
    part: &ChainPart,
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
    for when in [PanicsWhen::MakingItsFuture, PanicsWhen::Polled] {
        in_each_bracketing!(
            [d, a, e] from {
                let log = Log::default();
                let d = Driver::new(Panics(when));
                let a = Adapter::new(BitFlip::new(0, &log));
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
    let parts = [
        type_name::<Program>(),
        type_name::<Profiler>(),
        type_name::<BitFlip>(),
        type_name::<Bits>(),
    ];
    for (armed, name) in parts.into_iter().enumerate() {
        let named = ChainPart::new(armed, name);
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
                assert!(
                    log.count("Bits end_shots, holding 0") >= 1,
                    "{named}, {brackets}"
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
    for brackets in ["left", "right"] {
        let log = Log::default();
        let d = Driver::new(Program::new(&[("mz", 0)]));
        let a = Adapter::new(Impatient::new(&log));
        let e = Executor::new(PanicsInDrop::new(Later(Bits::new(&log)), true));
        let shots = ShotSet::ids([3, 1]);
        let error = if brackets == "left" {
            failed(runner((d >> a) >> e).run(&shots, SEED))
        } else {
            failed(runner(d >> (a >> e)).run(&shots, SEED))
        };
        assert_eq!(error.shot, ShotId::new(3), "{brackets}");
        let adapter = ChainPart::new(1, type_name::<Impatient>());
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked { part, message: Some(message) }
                    if *part == adapter && message == "the part's future broke when dropped"
            ),
            "{brackets}: {error:?}"
        );
    }
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

/// The parts of a [`CompoundChain`], whose part at position `armed`
/// panics when its futures are polled, and again when they are dropped.
fn compound_chain(armed: usize, log: &Log) -> CompoundChain {
    let counted = Rc::new(Cell::new(0));
    (
        Driver::new(PanicsInDrop::polled_too(
            Program::new(&[("mz", 0)]),
            armed == 0,
        )),
        Adapter::new(PanicsInDrop::polled_too(
            Profiler::new(&counted, log),
            armed == 1,
        )),
        Adapter::new(PanicsInDrop::polled_too(BitFlip::new(0, log), armed == 2)),
        Adapter::new(PanicsInDrop::polled_too(BitFlip::new(0, log), armed == 3)),
        Executor::new(PanicsInDrop::polled_too(Bits::new(log), armed == 4)),
    )
}

/// A program, over a profiler, over two noises, over bits, in each of
/// the 14 bracketings, one of them panicking while its future is polled,
/// and again when that future is dropped: the run fails naming that
/// part, at its position, with the message of its first panic, and the
/// second is dropped; the process does not abort, as it would if a
/// composed part's future dropped the part's while the first unwinds.
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
        let named = ChainPart::new(armed, name);
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
                let a = Adapter::new(PanicsInDrop::in_ready(BitFlip::new(0, &log), armed == 1));
                let b = Adapter::new(PanicsInDrop::in_ready(BitFlip::new(0, &log), armed == 2));
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
            }
        );
    }
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

/// The same chain, whose run's future a caller's panic drops while the
/// bits' future is pending: the bits' panic in its destructor, a second
/// panic while the caller's unwinds, is dropped rather than aborting the
/// process, so the caller's panic leaves, and the shots are ended on the
/// bits.
#[test]
fn a_run_dropped_by_a_panic_keeps_that_panic_when_a_parts_future_panics_in_drop() {
    let log = Log::default();
    let mut runner = runner(
        Driver::new(CoinCopy::new())
            >> Executor::new(PanicsInDrop::new(Later(Bits::new(&log)), true)),
    );
    let shots = ShotSet::ids(0..4);
    let caught = catch_unwind(AssertUnwindSafe(|| {
        let mut run = std::pin::pin!(runner.run_async(&shots, SEED));
        let poll = run.as_mut().poll(&mut Context::from_waker(Waker::noop()));
        assert!(poll.is_pending(), "the bits answer later");
        panic!("the caller broke");
    }));
    let panic = caught.expect_err("the caller's panic leaves");
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"the caller broke"));
    assert_eq!(log.count("Bits end_shots, holding 0"), 1);
}
