// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The check of a batch's instructions at an `execute` ([`check_batch`]):
//! what the protocol runs on every batch an executor receives, so that no
//! executor writes it, and none runs an instruction its set does not
//! declare, a block its shots do not have, or a readout parity its shots
//! were not provisioned for.
//!
//! It takes plain data, so that it depends on no later concept: the
//! instructions, and [`BatchLimits`], built once from the receiver's set,
//! its capabilities and the shots' needs. It compares integers only, and
//! stops at the first fault: it runs on the hot path.

use super::super::{BlockType, InstructionSet};
use super::{Instruction, InstructionFault};

/// The limits a batch is checked against, per receiver: built once, not
/// per batch.
#[derive(Clone, Debug)]
pub(crate) struct BatchLimits {
    /// The receiver's set, as read when the limits were built, before the
    /// receiver is provisioned (by the runner's `>>`, or by the provision of
    /// the `Composed` holding the boundary), whatever the receiver answers
    /// later.
    set: InstructionSet,
    /// The most instructions one batch may hold, if the receiver says.
    max_instructions: Option<u32>,
    /// What each of the batch's instructions is checked against.
    pub(crate) provisioned: Provisioned,
}

/// What the shots were provisioned for, as an instruction's check reads
/// it: their blocks of each of the receiver's block types, and whether
/// they may hold readout parities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Provisioned {
    /// The shot's blocks of each of the set's block types, in the set's
    /// order.
    pub(crate) blocks_of: Vec<u32>,
    /// Whether an instruction may hold a readout parity
    /// ([`Bit::Parity`](crate::Bit::Parity)).
    pub(crate) readout_parity: bool,
}

impl BatchLimits {
    /// The receiver's set, as it was read when the limits were built.
    pub(crate) fn set(&self) -> &InstructionSet {
        &self.set
    }

    /// The limits of a receiver of `set` that runs at most
    /// `max_instructions` per batch, on shots whose blocks of each type
    /// `blocks_of` gives, and which may hold readout parities if
    /// `readout_parity`.
    pub(crate) fn new(
        set: &InstructionSet,
        max_instructions: Option<u32>,
        blocks_of: impl Fn(&BlockType) -> u32,
        readout_parity: bool,
    ) -> Self {
        Self {
            set: set.clone(),
            max_instructions,
            provisioned: Provisioned {
                blocks_of: set.block_types().map(blocks_of).collect(),
                readout_parity,
            },
        }
    }
}

/// Why a batch is refused: too many instructions, the first refused
/// instruction, at its position in the batch, or more readout slots than a
/// `u32` counts, from the instruction at `at` on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BatchFault {
    TooManyInstructions { max: u32, got: usize },
    Instruction { at: usize, fault: InstructionFault },
    TooManyReadoutSlots { at: usize },
}

/// Checks `instructions`, in the opcodes of the receiver's set that
/// `limits` holds, against that set and `limits`: their count, then each
/// instruction as [`InstructionSet::instruction`] checks one, plus the
/// arguments past its parameters, its `select` (some pattern, over the
/// flags it has), each block below the shot's of its type, and no readout
/// parity unless the shots were provisioned for them; then that a `u32`
/// counts the batch's readout slots. Those slots, the sum of its
/// instructions', so that its answer's width can be checked; or the first
/// fault.
pub(crate) fn check_batch(
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
        let its_slots = limits
            .set
            .check(instruction, Some(&limits.provisioned))
            .map_err(|fault| BatchFault::Instruction { at, fault })?;
        slots = slots
            .checked_add(its_slots)
            .ok_or(BatchFault::TooManyReadoutSlots { at })?;
    }
    Ok(slots)
}
