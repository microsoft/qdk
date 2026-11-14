// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The check of a batch's instructions at an `execute` ([`check_batch`]):
//! what the protocol runs on every batch an executor receives, so that no
//! executor writes it, and none runs an instruction its set does not
//! declare or a block its shots do not have.
//!
//! It takes plain data, so that it depends on no later concept: the
//! receiver's set, the instructions, and [`BatchLimits`], derived once
//! from the receiver's capabilities and the shots' needs. It compares
//! integers only, and stops at the first fault: it runs on the hot path.

use super::super::{BlockType, InstructionSet};
use super::{Instruction, InstructionFault};

/// The limits a batch is checked against, per receiver: built once, not
/// per batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BatchLimits {
    /// The most instructions one batch may hold, if the receiver says.
    max_instructions: Option<u32>,
    /// The shot's blocks of each of the set's block types, in the set's
    /// order.
    blocks_of: Vec<u32>,
}

impl BatchLimits {
    /// The limits of a receiver of `set` that runs at most
    /// `max_instructions` per batch, on shots whose blocks of each type
    /// `blocks_of` gives.
    pub(crate) fn new(
        set: &InstructionSet,
        max_instructions: Option<u32>,
        blocks_of: impl Fn(&BlockType) -> u32,
    ) -> Self {
        Self {
            max_instructions,
            blocks_of: set.block_types().map(blocks_of).collect(),
        }
    }
}

/// Why a batch is refused: too many instructions, or the first refused
/// instruction, at its position in the batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BatchFault {
    TooManyInstructions { max: u32, got: usize },
    Instruction { at: usize, fault: InstructionFault },
}

/// Checks `instructions`, in the opcodes of `set`, the receiver's, against
/// `set` and `limits`: their count, then each instruction as
/// [`InstructionSet::instruction`] checks one, plus the arguments past its
/// parameters, the flags its `select` patterns name, and each block below
/// the shot's of its type. The batch's readout slots, the sum of its
/// instructions', so that its answer's width can be checked; or the first
/// fault.
pub(crate) fn check_batch(
    set: &InstructionSet,
    instructions: &[Instruction],
    limits: &BatchLimits,
) -> Result<u32, BatchFault> {
    if let Some(max) = limits.max_instructions
        && instructions.len() > max as usize
    {
        return Err(BatchFault::TooManyInstructions {
            max,
            got: instructions.len(),
        });
    }
    let mut slots: u32 = 0;
    for (at, instruction) in instructions.iter().enumerate() {
        let its_slots = set
            .check(instruction, Some(&limits.blocks_of))
            .map_err(|fault| BatchFault::Instruction { at, fault })?;
        slots = slots.saturating_add(its_slots);
    }
    Ok(slots)
}
