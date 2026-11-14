// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kinds of block an instruction acts on ([`BlockType`]), and an
//! instruction's operand of one ([`BlockOperand`]).
//!
//! A shot's quantum state is held in blocks, as Qodec calls them: a qubit
//! for QIR, a code's block for a code's logical instructions. An
//! instruction names the blocks it acts on by their index among the
//! shot's blocks of their type, so a part tells how many of each type a
//! shot uses, and an executor how many it can hold.

use std::fmt;

/// A kind of unit of a shot's quantum state that an instruction acts on:
/// [`BlockType::qubit`] for QIR, or a code's block, such as Qodec's
/// `c422`. Blocks are made by an instruction's `out` and consumed by its
/// `in`; how many qubits one block encodes is declared by the set that
/// uses the type.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockType(String);

impl BlockType {
    /// The block type named `name`, as an instruction set's `blocks`
    /// declares it: `BlockType::new("carbon")` for `blocks: {carbon: 2}`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The block that is one qubit, as QIR's and Stim's gates act on:
    /// named `qubit`, as Qodec's sets of physical gates name it.
    #[must_use]
    pub fn qubit() -> Self {
        Self::new("qubit")
    }

    /// Its name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BlockType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for BlockType {
    /// Its name, quoted, so a fault reads as the set declares it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

/// One entry of an instruction's `in` or `out`: a block of its type, or,
/// when `variadic`, any positive number of them, as Qodec's `[[carbon]]`
/// (Stim's `MPP` on any number of qubits).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockOperand {
    /// The type of its blocks.
    pub block_type: BlockType,
    /// Whether it stands for any positive number of blocks.
    pub variadic: bool,
}

impl BlockOperand {
    /// Any positive number of blocks of `block_type`, Qodec's
    /// `[[block_type]]`.
    #[must_use]
    pub fn variadic(block_type: BlockType) -> Self {
        Self {
            block_type,
            variadic: true,
        }
    }
}

impl From<BlockType> for BlockOperand {
    /// One block of that type, Qodec's `[block_type]` entry.
    fn from(block_type: BlockType) -> Self {
        Self {
            block_type,
            variadic: false,
        }
    }
}
