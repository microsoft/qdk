// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One entry of an instruction's `in` or `out` ([`BlockOperand`]): a block
//! of a type, or any positive number of them.

use super::BlockType;

/// One entry of an instruction's `in` or `out`: a block of its type, or,
/// when `variadic`, any positive number of them, as Qodec's `[[carbon]]`
/// (Stim's `MPP` on any number of qubits).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
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
