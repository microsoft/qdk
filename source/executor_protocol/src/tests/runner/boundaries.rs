// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The checks at each boundary of a chain, wherever the brackets go: a
//! refusal, a shape fault or a panic at a boundary inside a composed part
//! ends the run as it does at the executor's, and each boundary is checked
//! once.

use std::cell::Cell;
use std::rc::Rc;

use super::{SEED, failed, runner};
use crate::checked::BATCHES_CHECKED;
use crate::tests::fake_parts::{
    Bits, Log, Misbehaving, Misbehaviour, Misshape, Misshapen, Panics, PanicsWhen, Profiler,
    Program,
};
use crate::{
    Adapt, Adapter, Capabilities, CompositionFaults, Drive, DriveError, Driver, Execute,
    ExecuteRefusal, Executor, Failure, InstructionFault, Needs, Runner, ShapeFault, ShotId,
    ShotSet,
};

/// A run of a chain in one bracketing: how it failed, what its parts
/// logged, and how many instructions its profiler counted.
struct Failed {
    brackets: &'static str,
    error: DriveError,
    log: Log,
    counted: Rc<Cell<usize>>,
}

/// The failures of the run of `shots` on the chain `parts` makes, with the
/// adapter bracketed with the driver, then with the executor; each
/// bracketing gets new parts, a new log and a new count.
fn failed_in_both_brackets<D: Drive<Output: std::fmt::Debug>, A: Adapt, E: Execute>(
    shots: &ShotSet,
    parts: impl Fn(&Log, &Rc<Cell<usize>>) -> (Driver<D>, Adapter<A>, Executor<E>),
) -> [Failed; 2] {
    let new = || (Log::default(), Rc::new(Cell::new(0)));
    let (log, counted) = new();
    let (driver, adapter, executor) = parts(&log, &counted);
    let left = Failed {
        brackets: "the adapter with the driver",
        error: failed(runner((driver >> adapter) >> executor).run(shots, SEED)),
        log,
        counted,
    };
    let (log, counted) = new();
    let (driver, adapter, executor) = parts(&log, &counted);
    let right = Failed {
        brackets: "the adapter with the executor",
        error: failed(runner(driver >> (adapter >> executor)).run(shots, SEED)),
        log,
        counted,
    };
    [left, right]
}

/// Measures qubit 0 after flipping it: one batch of two instructions,
/// needing one qubit.
fn flip_and_measure() -> Driver<Program> {
    Driver::new(Program::new(&[("x", 0), ("mz", 0)]))
}

/// A driver flipping and measuring qubit 0, under an adapter that adds an
/// `x` on qubit 9 while passing the driver's need of one qubit down: the
/// adapter's batch is refused, naming its third instruction, and nothing
/// of it runs.
#[test]
fn an_adapters_instruction_beyond_the_needs_is_refused_before_it_runs() {
    for run in failed_in_both_brackets(&ShotSet::ids([4, 2]), |log, _| {
        (
            flip_and_measure(),
            Adapter::new(Misbehaving::new(Misbehaviour::SendsBeyondTheNeeds)),
            Executor::new(Bits::new(log)),
        )
    }) {
        let Failed {
            brackets, error, ..
        } = &run;
        assert_eq!(error.shot, ShotId::new(4), "{brackets}");
        assert!(
            matches!(
                &error.failure,
                Failure::Refused(ExecuteRefusal::Instruction {
                    batch: 0,
                    at: 2,
                    fault: InstructionFault::BlockBeyondNeeds {
                        block: 9,
                        needs: 1,
                        ..
                    },
                    ..
                })
            ),
            "{brackets}: {error:?}"
        );
        assert_eq!(run.log.count("Bits execute"), 0, "{brackets}: nothing ran");
    }
}

/// A driver's batch of two instructions, tripled by an adapter, above an
/// executor taking 4 at most, with a profiler between them or not: the
/// adapter's batch of 6 is refused, and nothing below the adapter runs.
#[test]
fn an_adapters_batch_longer_than_the_executor_takes_is_refused_before_it_runs() {
    let offer = Capabilities::unlimited().max_qubits(64).max_instructions(4);
    let triples = || Adapter::new(Misbehaving::new(Misbehaviour::TriplesEachBatch));
    let alone = failed_in_both_brackets(&ShotSet::ids(0..2), |log, _| {
        (
            flip_and_measure(),
            triples(),
            Executor::new(Bits::new(log).offering(offer.clone())),
        )
    });
    let profiled = failed_in_both_brackets(&ShotSet::ids(0..2), |log, counted| {
        (
            flip_and_measure(),
            triples() >> Adapter::new(Profiler::new(counted, log)),
            Executor::new(Bits::new(log).offering(offer.clone())),
        )
    });
    for (run, profiler) in alone
        .iter()
        .map(|run| (run, "no profiler"))
        .chain(profiled.iter().map(|run| (run, "a profiler")))
    {
        let Failed {
            brackets, error, ..
        } = run;
        assert!(
            matches!(
                error.failure,
                Failure::Refused(ExecuteRefusal::TooManyInstructions {
                    batch: 0,
                    max: 4,
                    got: 6,
                    ..
                })
            ),
            "{brackets}, {profiler}: {error:?}"
        );
        assert_eq!(run.counted.get(), 0, "{brackets}, {profiler}");
        assert_eq!(run.log.count("Bits execute"), 0, "{brackets}, {profiler}");
    }
}

/// A driver's batch of two instructions, through a profiler, above an
/// executor taking one at most: refused at the driver's boundary, before
/// the profiler counts any.
#[test]
fn a_drivers_batch_longer_than_the_offer_below_an_adapter_is_refused_above_it() {
    let offer = Capabilities::unlimited().max_qubits(64).max_instructions(1);
    for run in failed_in_both_brackets(&ShotSet::ids(0..2), |log, counted| {
        (
            flip_and_measure(),
            Adapter::new(Profiler::new(counted, log)),
            Executor::new(Bits::new(log).offering(offer.clone())),
        )
    }) {
        let Failed {
            brackets, error, ..
        } = &run;
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
            "{brackets}: {error:?}"
        );
        assert_eq!(
            run.counted.get(),
            0,
            "{brackets}: the profiler counted none"
        );
    }
}

/// A driver flipping qubit 1 while needing one qubit, through a profiler:
/// refused at the driver's boundary, before the profiler counts any.
#[test]
fn a_drivers_instruction_beyond_its_needs_is_refused_above_an_adapter() {
    for run in failed_in_both_brackets(&ShotSet::ids([5, 3]), |log, counted| {
        (
            Driver::new(Program::new(&[("mz", 0), ("x", 1)]).needing(Needs::qubits(1))),
            Adapter::new(Profiler::new(counted, log)),
            Executor::new(Bits::new(log)),
        )
    }) {
        let Failed {
            brackets, error, ..
        } = &run;
        assert_eq!(error.shot, ShotId::new(5), "{brackets}");
        assert!(
            matches!(
                &error.failure,
                Failure::Refused(ExecuteRefusal::Instruction {
                    batch: 0,
                    at: 1,
                    fault: InstructionFault::BlockBeyondNeeds {
                        block: 1,
                        needs: 1,
                        ..
                    },
                    ..
                })
            ),
            "{brackets}: {error:?}"
        );
        assert_eq!(
            run.counted.get(),
            0,
            "{brackets}: the profiler counted none"
        );
    }
}

/// An executor answering a row fewer than the batch's two shots, below a
/// profiler: a failure naming the batch's first shot and the shape fault.
#[test]
fn a_misshapen_answer_below_an_adapter_is_a_failure() {
    for run in failed_in_both_brackets(&ShotSet::ids([6, 1]), |log, counted| {
        (
            Driver::new(Program::new(&[("mz", 0)])),
            Adapter::new(Profiler::new(counted, log)),
            Executor::new(Misshapen(Misshape::MissingRow)),
        )
    }) {
        let Failed {
            brackets, error, ..
        } = run;
        assert_eq!(error.shot, ShotId::new(6), "{brackets}");
        let Failure::Part(fault) = error.failure else {
            panic!("{brackets}: the part's error, not {:?}", error.failure);
        };
        assert_eq!(
            fault.downcast_ref::<ShapeFault>(),
            Some(&ShapeFault::Rows {
                batch: 0,
                expected: 2,
                got: 1
            }),
            "{brackets}"
        );
    }
}

/// An executor panicking while polled, below a profiler: a failure naming
/// the batch's first shot and the message.
#[test]
fn an_executors_panic_below_an_adapter_is_a_failure_of_its_batch() {
    for run in failed_in_both_brackets(&ShotSet::ids([6, 1]), |log, counted| {
        (
            Driver::new(Program::new(&[("mz", 0)])),
            Adapter::new(Profiler::new(counted, log)),
            Executor::new(Panics(PanicsWhen::Polled)),
        )
    }) {
        let Failed {
            brackets, error, ..
        } = &run;
        assert_eq!(error.shot, ShotId::new(6), "{brackets}");
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked(Some(message)) if message == "the part broke while it was polled"
            ),
            "{brackets}: {error:?}"
        );
    }
}

/// An adapter panicking with a formatted message, a `String`: the failure
/// carries the message.
#[test]
fn an_adapters_panic_with_a_formatted_message_carries_it() {
    for run in failed_in_both_brackets(&ShotSet::ids([3, 1]), |log, _| {
        (
            Driver::new(Program::new(&[("mz", 0)])),
            Adapter::new(Misbehaving::new(Misbehaviour::PanicsWithAString)),
            Executor::new(Bits::new(log)),
        )
    }) {
        let Failed {
            brackets, error, ..
        } = &run;
        assert_eq!(error.shot, ShotId::new(3), "{brackets}");
        assert!(
            matches!(
                &error.failure,
                Failure::Panicked(Some(message)) if message == "the adapter broke on 1 batches"
            ),
            "{brackets}: {error:?}"
        );
    }
}

/// An adapter panicking with a number, a payload that is not text: a
/// failure with no message.
#[test]
fn an_adapters_panic_with_no_text_is_a_failure_with_no_message() {
    for run in failed_in_both_brackets(&ShotSet::ids([3, 1]), |log, _| {
        (
            Driver::new(Program::new(&[("mz", 0)])),
            Adapter::new(Misbehaving::new(Misbehaviour::PanicsWithANumber)),
            Executor::new(Bits::new(log)),
        )
    }) {
        let Failed {
            brackets, error, ..
        } = &run;
        assert_eq!(error.shot, ShotId::new(3), "{brackets}");
        assert!(
            matches!(&error.failure, Failure::Panicked(None)),
            "{brackets}: {error:?}"
        );
    }
}

/// How many batches the `Checked`s of the chain `composed` checked in a
/// run of two shots of one batch.
fn batches_checked<D: Drive, E: Execute>(
    composed: Result<Runner<D, E>, CompositionFaults>,
) -> usize {
    let mut runner = runner(composed);
    BATCHES_CHECKED.with(|checked| checked.set(0));
    runner
        .run(&ShotSet::ids(0..2), SEED)
        .expect("the run succeeds");
    BATCHES_CHECKED.with(Cell::get)
}

/// A driver, two profilers at most and bits, in every bracketing: each
/// boundary's batch is checked once, so a chain of `n` boundaries checks
/// `n` batches.
#[test]
fn each_boundary_is_checked_once_wherever_the_brackets_go() {
    let counted = Rc::new(Cell::new(0));
    let log = Log::default();
    let d = flip_and_measure;
    let p = || Adapter::new(Profiler::new(&counted, &log));
    let e = || Executor::new(Bits::new(&log));
    let checked = [
        batches_checked(d() >> e()),
        batches_checked((d() >> p()) >> e()),
        batches_checked(d() >> (p() >> e())),
        batches_checked(((d() >> p()) >> p()) >> e()),
        batches_checked(d() >> (p() >> (p() >> e()))),
        batches_checked((d() >> (p() >> p())) >> e()),
        batches_checked(d() >> ((p() >> p()) >> e())),
        batches_checked((d() >> p()) >> (p() >> e())),
    ];
    assert_eq!(checked, [1, 2, 2, 3, 3, 3, 3, 3]);
}
