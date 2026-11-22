// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An instruction's name in a batch: its place in its set ([`Opcode`]).

/// An instruction's place in its set. Its index is private: a part gets
/// its opcodes from [`instruction_set!`](crate::instruction_set)'s
/// constants or from
/// [`BlockInstructionSet::opcode`](crate::BlockInstructionSet::opcode), and
/// `match`es on them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Opcode(u32);

impl Opcode {
    /// The opcode at `index`, for the constants of
    /// [`instruction_set!`](crate::instruction_set) only: elsewhere it would
    /// name an instruction no set has resolved.
    #[doc(hidden)]
    #[must_use]
    pub const fn __from_macro(index: u32) -> Self {
        Opcode(index)
    }

    /// The opcode at `index`, for its set when it is built.
    pub(super) fn at(index: u32) -> Self {
        Opcode(index)
    }

    /// Its index in its set.
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}
