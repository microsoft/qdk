// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The checks of a batch at an `execute`: its count of instructions, on
//! every batch, and, on a checked batch, each instruction against the
//! receiver's set and the capabilities required, and its stated readout
//! slots against its instructions'. One batch, sent by a driver straight
//! to an executor.

use std::error::Error;
use std::fmt;

use crate::prelude::*;
use crate::tests::example_sets::{
    Gate, Measuring, OnQubit, QubitBeyond, QubitLimited, UnknownGate, on_qubits,
};
use crate::{
    Batch, BatchFault, BatchReadouts, Binding, BoundaryCompatible, Capabilities, DriveError,
    ExecuteError, ExecuteRefusal, Failure, Features, InstructionSet, ReadoutValue, Readouts,
    Resources, RunError, Seed, ShotId, ShotSet,
};

/// How a sender marks a new batch before it sends it.
type Mark<S> = for<'a> fn(Batch<'a, S>) -> Batch<'a, S>;

/// A driver of `sends` sending `instructions` as one batch to every shot,
/// marked by `mark`, stating `stated` readout slots, or else theirs (0 if
/// a `u32` does not count them), and requiring `requires`. Each shot's
/// record is the readout slots of the batch's readouts.
struct Literal<S: InstructionSet> {
    sends: S,
    instructions: Vec<S::Instruction>,
    stated: Option<u32>,
    requires: Capabilities<S>,
    mark: Mark<S>,
}

impl<S: InstructionSet> Driver for Literal<S> {
    type Sends = S;
    type Output = u32;

    fn sends(&self) -> &S {
        &self.sends
    }

    fn requires(&self, _run: &Capabilities<S>) -> Capabilities<S> {
        self.requires.clone()
    }

    /// Sends in its receiver's own set, which it binds to itself.
    fn bind(&mut self, _itself: Binding<S>) {}

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = S>,
    ) -> Result<Vec<u32>, DriveError> {
        let theirs = (self.instructions.iter())
            .try_fold(0_u32, |slots, instruction| {
                slots.checked_add(self.sends.readout_slots(instruction))
            })
            .unwrap_or(0);
        let stated = self.stated.unwrap_or(theirs);
        let batches = [(self.mark)(Batch::new(shots, &self.instructions, stated))];
        let mut kept = Readouts::new();
        let readouts = kept.readouts_for(&batches);
        below.execute(&batches, seed, readouts).await?;
        Ok(vec![readouts[0].readout_slots(); shots.len()])
    }
}

/// An executor of `accepts` answering `Zero` for every readout, whatever
/// the batch.
struct Zeros<S: InstructionSet> {
    accepts: S,
    provides: Capabilities<S>,
}

impl<S: InstructionSet> Executor for Zeros<S> {
    type Accepts = S;

    fn accepts(&self) -> &S {
        &self.accepts
    }

    fn provides(&self) -> &Capabilities<S> {
        &self.provides
    }

    async fn execute(
        &mut self,
        _batches: &[Batch<'_, S>],
        _seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        for batch_readouts in readouts {
            for position in 0..batch_readouts.shots() {
                batch_readouts.row_mut(position).fill(ReadoutValue::Zero);
            }
        }
        Ok(())
    }
}

/// The run of shots 0 and 1 of `driver` over an executor of its set
/// providing `provided`: the readout slots of the readouts it filled, or
/// the refusal of its batch.
fn run<S: InstructionSet>(
    driver: Literal<S>,
    provided: Capabilities<S>,
) -> Result<u32, ExecuteRefusal> {
    let executor = Zeros {
        accepts: driver.sends.clone(),
        provides: provided,
    };
    let Ok(mut runner) = driver.on(executor) else {
        panic!("the chain composes");
    };
    match runner.run(&ShotSet::ids(0..2), 7) {
        Ok(slots) => Ok(slots[0]),
        Err(RunError::Failed(DriveError {
            failure: Failure::Refused(refusal),
            ..
        })) => Err(refusal),
        Err(other) => panic!("a run or a refusal, not {other:?}"),
    }
}

/// The refusal of the batch, for `fault`, as its `Debug` reads, which
/// compares refusals holding a set's boxed fault.
fn refused(fault: BatchFault) -> Result<u32, String> {
    Err(format!(
        "{:?}",
        ExecuteRefusal::Batch {
            shot: ShotId::new(0),
            batch: 0,
            fault,
        }
    ))
}

/// The refusal of the batch, naming the instruction at `at` and `fault`.
fn refused_at(at: usize, fault: impl Error + Send + Sync + 'static) -> Result<u32, String> {
    refused(BatchFault::Instruction {
        at,
        fault: Box::new(fault),
    })
}

/// `result` with its refusal as its `Debug` reads.
fn debug(result: Result<u32, ExecuteRefusal>) -> Result<u32, String> {
    result.map_err(|refusal| format!("{refusal:?}"))
}

/// The gates `h`, `x` and `mz`.
const GATES: Measuring = Measuring::new(&["h", "x", "mz"]);

/// A driver of [`GATES`] sending `gates`, requiring nothing, its batch
/// marked by `mark` and stating `stated` readout slots, if some.
fn measuring(gates: Vec<Gate>, stated: Option<u32>, mark: Mark<Measuring>) -> Literal<Measuring> {
    Literal {
        sends: GATES,
        instructions: gates,
        stated,
        requires: Measuring::unconstrained().clone(),
        mark,
    }
}

/// `count` `h` gates.
fn hs(count: usize) -> Vec<Gate> {
    vec![GATES.gate("h", 0); count]
}

/// A gate beyond [`GATES`], as a sender writing past its set would send.
const BEYOND: Gate = Gate { at: 99, qubit: 0 };

/// A batch of as many instructions as the receiver takes at most runs, a
/// longer one is refused, checked or not, so that an executor with a
/// buffer of that size never receives more; and a receiver with no most
/// takes any count.
#[test]
fn a_batch_of_more_than_the_receivers_most_instructions_is_refused() {
    let too_many = || refused(BatchFault::TooManyInstructions { max: 2, got: 3 });
    for (most, count, sent_checked, expected) in [
        (Some(2), 2, true, Ok(0)),
        (Some(2), 2, false, Ok(0)),
        (Some(2), 3, true, too_many()),
        (Some(2), 3, false, too_many()),
        (None, 1000, true, Ok(0)),
    ] {
        let mark: Mark<Measuring> = if sent_checked {
            |batch| batch.checked()
        } else {
            |batch| batch
        };
        let resources = Resources {
            instructions_per_batch: most,
            ..Resources::unconstrained()
        };
        let provided = Measuring::unconstrained()
            .clone()
            .map_resources(|_| resources);
        assert_eq!(
            debug(run(measuring(hs(count), None, mark), provided)),
            expected,
            "{count} instructions, at most {most:?}, checked: {sent_checked}"
        );
    }
}

/// A batch is checked as the last mark its sender gave it says: one an
/// adapter marks `checked_if(false)`, as it does to pass on that the batch
/// it adapts is unchecked, is unchecked even after `checked`, and one
/// marked `checked` after it is checked. Holding a gate beyond the
/// receiver's set, an unchecked batch reaches the receiver as sent,
/// uninterpreted; a checked one is refused.
#[test]
fn a_batch_is_checked_as_its_last_mark_says() {
    let marks: [(&str, Mark<Measuring>, bool); 4] = [
        ("checked_if(true)", |batch| batch.checked_if(true), true),
        ("checked_if(false)", |batch| batch.checked_if(false), false),
        (
            "checked, then checked_if(false)",
            |batch| batch.checked().checked_if(false),
            false,
        ),
        (
            "checked_if(false), then checked",
            |batch| batch.checked_if(false).checked(),
            true,
        ),
    ];
    for (marked, mark, refused) in marks {
        let expected = if refused {
            refused_at(0, UnknownGate { at: 99 })
        } else {
            Ok(0)
        };
        let run = run(
            measuring(vec![BEYOND], None, mark),
            Measuring::unconstrained().clone(),
        );
        assert_eq!(debug(run), expected, "{marked}");
    }
}

/// A checked batch stating other readout slots than its instructions read
/// is refused, naming both, so that its readouts are never shaped wrong;
/// one stating theirs, each instruction's summed, runs, its readouts
/// shaped with them.
#[test]
fn a_checked_batch_stating_other_readout_slots_is_refused() {
    let gates = vec![GATES.gate("mz", 0), GATES.gate("h", 1), GATES.gate("mz", 1)];
    let misstated = |stated| refused(BatchFault::ReadoutSlotsMisstated { stated, counted: 2 });
    let checked: Mark<Measuring> = |batch| batch.checked();
    for (stated, expected) in [(2, Ok(2)), (1, misstated(1)), (3, misstated(3))] {
        let run = run(
            measuring(gates.clone(), Some(stated), checked),
            Measuring::unconstrained().clone(),
        );
        assert_eq!(debug(run), expected, "stating {stated}");
    }
}

/// Each of a checked batch's instructions is checked against what its
/// shots require, not what the receiver provides, and the refusal names
/// the first refused one at its position, with the set's own fault, which
/// a caller downcasts to read: on shots requiring 3 qubits of a receiver
/// providing 8, a gate on qubit 3 is refused, and a later one on qubit 5
/// is not reached.
#[test]
fn the_first_instruction_beyond_what_is_required_is_refused_at_its_position() {
    let driver = Literal {
        sends: QubitLimited,
        instructions: [0, 3, 5].map(OnQubit).to_vec(),
        stated: None,
        requires: on_qubits(3),
        mark: |batch| batch.checked(),
    };
    let refusal = run(driver, on_qubits(8)).expect_err("the gate on qubit 3 is refused");
    let ExecuteRefusal::Batch {
        fault: BatchFault::Instruction { at, fault },
        ..
    } = refusal
    else {
        panic!("an instruction's fault, not {refusal:?}");
    };
    assert_eq!(at, 1, "the gate on qubit 3");
    assert_eq!(
        fault.downcast_ref::<QubitBeyond>(),
        Some(&QubitBeyond {
            qubit: 3,
            qubits: 3
        })
    );
}

/// A set whose each instruction reads the readout slots it holds.
#[derive(Clone, Debug, PartialEq)]
struct Wide;

/// No capabilities of its own.
#[derive(Clone, Debug, PartialEq)]
struct Nothing;

impl BoundaryCompatible for Nothing {
    type Fit = ();
    type Mismatch = std::convert::Infallible;

    fn fits(&self, _provided: &Self) -> Result<(), Self::Mismatch> {
        Ok(())
    }
}

impl BoundaryCompatible for Wide {
    type Fit = ();
    type Mismatch = std::convert::Infallible;

    fn fits(&self, _accepted: &Self) -> Result<(), Self::Mismatch> {
        Ok(())
    }
}

impl InstructionSet for Wide {
    type Instruction = u32;
    type Capabilities = Nothing;
    type Requirement = ();
    type Fault = Beyond;

    fn unconstrained() -> &'static Capabilities<Self> {
        static UNCONSTRAINED: Capabilities<Wide> = Capabilities::new(
            Resources::unconstrained(),
            Features::unconstrained(),
            Nothing,
        );
        &UNCONSTRAINED
    }

    fn readout_slots(&self, slots: &u32) -> u32 {
        *slots
    }

    fn requirement(&self, _required: &Nothing) {}

    fn check(&self, slots: &u32, (): &()) -> Result<u32, Beyond> {
        if *slots == u32::MAX {
            Err(Beyond)
        } else {
            Ok(*slots)
        }
    }
}

/// The instruction `u32::MAX`, which [`Wide`] refuses.
#[derive(Debug)]
struct Beyond;

impl fmt::Display for Beyond {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("beyond the set")
    }
}

impl Error for Beyond {}

/// A batch's readout slots are a `u32`: a checked batch whose
/// instructions' readout slots sum to more is refused, naming the
/// instruction from which they no longer fit, before any later instruction
/// is checked.
#[test]
fn a_batch_reading_more_readout_slots_than_a_u32_counts_is_refused() {
    let half = 1 << 31;
    for (scene, instructions, at) in [
        ("twice 2^31", vec![half, half], 1),
        (
            "twice 2^31, then an instruction beyond the set",
            vec![half, half, u32::MAX],
            1,
        ),
        ("1, 2^31, then 2^31 - 1", vec![1, half, half - 1], 2),
    ] {
        let driver = Literal {
            sends: Wide,
            instructions,
            stated: None,
            requires: Wide::unconstrained().clone(),
            mark: |batch| batch.checked(),
        };
        assert_eq!(
            debug(run(driver, Wide::unconstrained().clone())),
            refused(BatchFault::TooManyReadoutSlots { at }),
            "{scene}"
        );
    }
}
