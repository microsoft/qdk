// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The limits a boundary crossing checks each batch against
//! ([`BatchLimits`]), and its checks, which the protocol runs so that no
//! executor writes them: the count of a batch's instructions
//! ([`BatchLimits::check_count`]), on every batch an executor receives, so
//! that none receives more than it provides for; and its instructions
//! ([`BatchLimits::check`]), on every batch its sender marked checked, so
//! that none runs such a batch's instruction its set does not declare, a
//! block beyond those required, or a readout parity not required, and
//! that the readout slots its sender stated, which the batch's readouts
//! are shaped with, are its instructions'. An unchecked batch's readouts
//! are shaped with the slots its sender stated, unread.
//!
//! The limits are built once per run of a boundary, from what is required
//! and provided across it. The checks compare integers only, and stop at
//! the first fault: they run on the hot path.

use std::error::Error;
use std::fmt;

use crate::instruction_set::ReceiverRequirement;
use crate::{Capabilities, Feature, Instruction, InstructionFault, InstructionSet, Resource};

/// The limits a batch is checked against, per boundary: built once per
/// run, not per batch.
#[derive(Clone, Debug)]
pub(crate) struct BatchLimits {
    /// The most instructions one batch may hold, if the receiver says.
    max_instructions: Option<u32>,
    /// What the shots require, as each of the batch's instructions is
    /// checked against it.
    required: ReceiverRequirement,
}

impl BatchLimits {
    /// The limits of the batches sent across a boundary where `required`
    /// meets `provided`: the most instructions per batch provided; the
    /// blocks required of each block type provided, in its set's order;
    /// and whether readout parities are required.
    pub(crate) fn new(required: &Capabilities, provided: &Capabilities) -> Self {
        let blocks_of = (provided.instructions().block_types())
            .map(|block_type| required.resources().blocks(block_type).unwrap_or(0))
            .collect();
        let readout_parity = required.features().get(&Feature::ReadoutParity);
        Self {
            max_instructions: provided.resources().get(&Resource::InstructionsPerBatch),
            required: ReceiverRequirement {
                blocks_of,
                readout_parity: readout_parity == Some(true),
            },
        }
    }

    /// Checks that `instructions` are no more than the receiver's most:
    /// one comparison, whether the batch is checked or not.
    pub(crate) fn check_count(&self, instructions: &[Instruction]) -> Result<(), BatchFault> {
        match self.max_instructions {
            Some(max) if instructions.len() > max as usize => {
                Err(BatchFault::TooManyInstructions {
                    max,
                    got: instructions.len(),
                })
            }
            _ => Ok(()),
        }
    }

    /// Checks `instructions`, in the opcodes of `set`, the receiver's,
    /// against that set and these limits, in order: each instruction as
    /// [`InstructionSet::instruction`] checks one, plus the arguments past
    /// its parameters, its `select` (some pattern, over the flags it has),
    /// each block below those required of its type, and no readout parity
    /// unless required, and that a `u32` still
    /// counts the batch's readout slots up to it; then that those slots,
    /// the sum of its instructions', are the `stated` ones the readouts are
    /// shaped with. Returns the first fault. The count of instructions is
    /// checked by [`check_count`](Self::check_count).
    pub(crate) fn check(
        &self,
        set: &InstructionSet,
        instructions: &[Instruction],
        stated: u32,
    ) -> Result<(), BatchFault> {
        let mut slots: u32 = 0;
        for (at, instruction) in instructions.iter().enumerate() {
            let its_slots = set
                .check(instruction, Some(&self.required))
                .map_err(|fault| BatchFault::Instruction { at, fault })?;
            slots = slots
                .checked_add(its_slots)
                .ok_or(BatchFault::TooManyReadoutSlots { at })?;
        }
        if slots == stated {
            Ok(())
        } else {
            Err(BatchFault::ReadoutSlotsMisstated {
                stated,
                counted: slots,
            })
        }
    }
}

/// Why a boundary refused a batch, in an
/// [`ExecuteRefusal::Batch`](crate::ExecuteRefusal::Batch).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BatchFault {
    /// The batch holds more instructions than the receiver's most
    /// ([`Resource::InstructionsPerBatch`]). Checked for every batch,
    /// [`checked`](crate::Batch::checked) or not.
    TooManyInstructions {
        /// The most instructions a batch may hold.
        max: u32,
        /// The batch's instructions.
        got: usize,
    },
    /// The batch's instruction at `at` is not one the receiver accepts,
    /// for `fault`. Checked for a [`checked`](crate::Batch::checked) batch
    /// only, as are the faults below.
    Instruction {
        /// The instruction's position in its batch.
        at: usize,
        /// Why it is refused.
        fault: InstructionFault,
    },
    /// The batch's instructions together read more readout slots than a
    /// `u32` counts, from the instruction at `at` on, so no batch's
    /// readouts could hold them.
    TooManyReadoutSlots {
        /// The position, in its batch, of the instruction whose slots no
        /// longer fit.
        at: usize,
    },
    /// The batch's instructions read `counted` readout slots, and its
    /// sender stated `stated`
    /// ([`Batch::readout_slots`](crate::Batch::readout_slots)), which its
    /// readouts would have been shaped with.
    ReadoutSlotsMisstated {
        /// The readout slots its sender stated.
        stated: u32,
        /// The readout slots of its instructions.
        counted: u32,
    },
}

impl fmt::Display for BatchFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BatchFault::TooManyInstructions { max, got } => {
                write!(
                    f,
                    "it holds {got} instructions, and at most {max} are accepted"
                )
            }
            BatchFault::Instruction { at, fault } => write!(f, "instruction {at}: {fault}"),
            BatchFault::TooManyReadoutSlots { at } => write!(
                f,
                "it reads more readout slots than a `u32` counts, from instruction {at} on"
            ),
            BatchFault::ReadoutSlotsMisstated { stated, counted } => write!(
                f,
                "it states {stated} readout slots, and its instructions read {counted}"
            ),
        }
    }
}

impl Error for BatchFault {}
