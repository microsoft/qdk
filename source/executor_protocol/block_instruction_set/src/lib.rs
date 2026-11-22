// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An instruction set in Qodec's model, [`BlockInstructionSet`]: one
//! implementation of the executor protocol's instruction set.

mod block_instruction_set;

pub use block_instruction_set::*;

#[cfg(test)]
mod tests;
