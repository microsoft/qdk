// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A part's future, awaited by the protocol: a panic while it is polled
//! or dropped leaves no live future of the part, so it is a failure naming
//! the part whose call it unwound through, and of two panics, one while
//! polled and one while dropped, only the first leaves, rather than
//! aborting the process. Each scene runs through the runner, the
//! compositions and the roles' default forms, which await a part's future
//! alike.

use std::any::type_name;
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use crate::tests::composition::runner::{SEED, failed, runner};
use crate::tests::fake_parts::{
    BitFlip, Bits, Impatient, Later, Log, PanicsInDrop, Profiler, Program, Renamed, Sending,
    in_each_bracketing,
};
use crate::{Adapter, DriveError, Driver, Executor, Failure, PartId, ShotId, ShotSet};

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
