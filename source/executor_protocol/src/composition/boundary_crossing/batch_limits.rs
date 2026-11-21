// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The limits a boundary crossing checks each batch against
//! ([`BatchLimits`]), and its checks, which the protocol runs so that no
//! executor writes them: the count of a batch's instructions
//! ([`BatchLimits::check_count`]), on every batch an executor receives, so
//! that none receives more than it says it takes; and its instructions
//! ([`BatchLimits::check`]), on every batch its sender marked checked, so
//! that none runs such a batch's instruction its set does not declare, a
//! block its shots do not have, or a readout parity its shots' needs do
//! not use, and that the readout slots its sender stated, which
//! the batch's readouts are shaped with, are its instructions'. An
//! unchecked batch's readouts are shaped with the slots its sender stated,
//! unread.
//!
//! The limits are built once per boundary, from the receiver's set, its
//! offer and the shots' needs. The checks compare integers only, and stop
//! at the first fault: they run on the hot path.

use crate::instruction_set::ReceiverNeeds;
use crate::{Instruction, InstructionFault, InstructionSet, Needs, Offer};

/// The limits a batch is checked against, per receiver: built once, not
/// per batch.
#[derive(Clone, Debug)]
pub(crate) struct BatchLimits {
    /// The receiver's set, as read when the limits were built, by the
    /// `on` that makes the runner, whatever the receiver answers later.
    set: InstructionSet,
    /// The most instructions one batch may hold, if the receiver says.
    max_instructions: Option<u32>,
    /// The shots' needs, as each of the batch's instructions is checked
    /// against them.
    needs: ReceiverNeeds,
}

impl BatchLimits {
    /// The limits of the batches sent to a receiver of `receiver`'s set,
    /// whose offer is `offer`, by a part whose shots need `needs`: the
    /// receiver's most instructions, the blocks of each of its block types,
    /// and whether readout parities may be sent.
    pub(crate) fn new(receiver: InstructionSet, offer: &Offer, needs: &Needs) -> Self {
        let blocks_of = receiver
            .block_types()
            .map(|block_type| needs.blocks_of(block_type))
            .collect();
        Self {
            set: receiver,
            max_instructions: offer.max_instructions,
            needs: ReceiverNeeds {
                blocks_of,
                readout_parity: needs.uses_readout_parity,
            },
        }
    }

    /// The receiver's set, as it was read when the limits were built.
    pub(crate) fn set(&self) -> &InstructionSet {
        &self.set
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

    /// Checks `instructions`, in the opcodes of the receiver's set, against
    /// that set and these limits, in order: each instruction as
    /// [`InstructionSet::instruction`] checks one, plus the arguments past
    /// its parameters, its `select` (some pattern, over the flags it has),
    /// each block below the shot's of its type, and no readout parity
    /// unless the shots' needs use them, and that a `u32` still
    /// counts the batch's readout slots up to it; then that those slots,
    /// the sum of its instructions', are the `stated` ones the readouts are
    /// shaped with. Returns the first fault. The count of instructions is
    /// checked by [`check_count`](Self::check_count).
    pub(crate) fn check(
        &self,
        instructions: &[Instruction],
        stated: u32,
    ) -> Result<(), BatchFault> {
        let mut slots: u32 = 0;
        for (at, instruction) in instructions.iter().enumerate() {
            let its_slots = self
                .set
                .check(instruction, Some(&self.needs))
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
