// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kinds of block an instruction acts on ([`BlockType`]).
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
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
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
