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

use crate::instruction_set::ReceiverRequirement;
use crate::{Instruction, InstructionFault, InstructionSet};

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
    /// The limits of the batches sent to a receiver that provides for
    /// `max_instructions` per batch, by a part whose shots require
    /// `blocks_of` each of the receiver's block types, in its set's order,
    /// and use readout parities when `readout_parity`.
    pub(crate) fn new(
        max_instructions: Option<u32>,
        blocks_of: Vec<u32>,
        readout_parity: bool,
    ) -> Self {
        Self {
            max_instructions,
            required: ReceiverRequirement {
                blocks_of,
                readout_parity,
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

/// Why a batch is refused: too many instructions, the first refused
/// instruction, at its position in the batch, more readout slots than a
/// `u32` counts, from the instruction at `at` on, or readout slots other
/// than its sender stated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BatchFault {
    TooManyInstructions { max: u32, got: usize },
    Instruction { at: usize, fault: InstructionFault },
    TooManyReadoutSlots { at: usize },
    ReadoutSlotsMisstated { stated: u32, counted: u32 },
}
