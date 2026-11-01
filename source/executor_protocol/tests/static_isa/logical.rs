// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The logical instruction set, a type ([`Logical`]), and its description
//! ([`logical_set`]), for a caller that states the instructions it needs.

use executor_protocol::{Action, InstructionSet, OperandKind, TypedInstruction};

use crate::toy;

/// The logical instruction set: one logical qubit per block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Logical {
    /// Flips the block's logical value.
    X(u32),
    /// Sets the block to a fair random logical bit.
    Coin(u32),
    /// Reads the block's logical value, and names it `tag`.
    MeasureZ { block: u32, tag: u32 },
}

/// A logical instruction names itself by its variant, so a driver of them
/// is composed with a chain with no renumbering between them, whatever the
/// order of the set it states.
impl TypedInstruction for Logical {}

impl Logical {
    pub fn block(self) -> u32 {
        match self {
            Logical::X(b) | Logical::Coin(b) => b,
            Logical::MeasureZ { block, .. } => block,
        }
    }
}

/// The description of [`Logical`], for a caller that states the
/// instructions it needs: the compiler checks that a call is a `Logical`,
/// not what a caller several adapters up will send. Written by hand, so it
/// must name each variant.
pub fn logical_set() -> InstructionSet {
    use OperandKind::{Block, Tag};
    InstructionSet::new("logical")
        .with_action("x", [Block], logical_action("x"))
        .with_action("coin", [Block], logical_action("coin"))
        .with_action("measure_z", [Block, Tag], logical_action("measure_z"))
}

/// What a logical instruction does to its logical qubit, as Qodec declares
/// it: what the toy's instruction of the same kind does to a bit
/// (`measure_z` is a `measure`).
pub fn logical_action(mnemonic: &str) -> Action {
    match mnemonic {
        "measure_z" => toy::action("measure"),
        same => toy::action(same),
    }
}
