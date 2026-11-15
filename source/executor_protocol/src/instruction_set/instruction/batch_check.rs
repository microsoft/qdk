// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The checks of a batch at an `execute`, which the protocol runs so that
//! no executor writes them: the count of its instructions
//! ([`check_instruction_count`]), on every batch an executor receives, so
//! that none receives more than it says it takes; and its instructions
//! ([`check_instructions`]), on every batch its sender marked checked, so that
//! none runs such a batch's instruction its set does not declare, a block
//! its shots do not have, or a readout parity its shots were not
//! provisioned for. The second also gives the batch's readout slots, which
//! the answer's shape is checked against.
//!
//! They take plain data, so that they depend on no later concept: the
//! instructions, and [`BatchLimits`], built once from the receiver's set,
//! its capabilities and the shots' needs. They compare integers only, and
//! stop at the first fault: they run on the hot path.

use super::super::{BlockType, InstructionSet};
use super::{Instruction, InstructionFault};

/// Checks that `instructions` are no more than the receiver's most, which
/// `limits` holds: one comparison, whether the batch is checked or not.
pub(crate) fn check_instruction_count(
    instructions: &[Instruction],
    limits: &BatchLimits,
) -> Result<(), BatchFault> {
    match limits.max_instructions {
        Some(max) if instructions.len() > max as usize => Err(BatchFault::TooManyInstructions {
            max,
            got: instructions.len(),
        }),
        _ => Ok(()),
    }
}

/// Checks `instructions`, in the opcodes of the receiver's set that
/// `limits` holds, against that set and `limits`, in order: each
/// instruction as [`InstructionSet::instruction`] checks one, plus the
/// arguments past its parameters, its `select` (some pattern, over the
/// flags it has), each block below the shot's of its type, and no readout
/// parity unless the shots were provisioned for them, and that a `u32`
/// still counts the batch's readout slots up to it. Returns those slots,
/// the sum of its instructions', so that the answer's width can be
/// checked, or the first fault. Their count is
/// [`check_instruction_count`]'s.
pub(crate) fn check_instructions(
    instructions: &[Instruction],
    limits: &BatchLimits,
) -> Result<u32, BatchFault> {
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

/// Why a batch is refused: too many instructions, the first refused
/// instruction, at its position in the batch, or more readout slots than a
/// `u32` counts, from the instruction at `at` on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BatchFault {
    TooManyInstructions { max: u32, got: usize },
    Instruction { at: usize, fault: InstructionFault },
    TooManyReadoutSlots { at: usize },
}

/// The limits a batch is checked against, per receiver: built once, not
/// per batch.
#[derive(Clone, Debug)]
pub(crate) struct BatchLimits {
    /// The receiver's set, as read when the limits were built, before the
    /// receiver is provisioned, whatever the receiver answers later.
    set: InstructionSet,
    /// The most instructions one batch may hold, if the receiver says.
    max_instructions: Option<u32>,
    /// What each of the batch's instructions is checked against.
    provisioned: Provisioned,
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
