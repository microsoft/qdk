// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Amounts a part requires of the part below, or provides the part above:
//! [`Resources`], an amount per [`Resource`].

use std::fmt;

use super::{StatementMismatch, Statements};
use crate::BlockType;

/// Amounts: the most blocks of each type a shot holds at once, the most
/// shots a run holds, and the most instructions a batch holds.
///
/// Required, each is the most the part's shots, runs or batches reach;
/// provided, the most the part takes. A part that does not state one
/// neither needs nor limits it. A checked batch, though, may use no block of a
/// type the requirement arriving at its receiver does not state.
///
/// Built from [`unconstrained`](Self::unconstrained), which states
/// nothing, with the builders below.
pub type Resources = Statements<Resource, u32>;

/// An amount required above the one provided.
pub type ResourceMismatch = StatementMismatch<Resource, u32>;

/// What an amount counts, in the order mismatches are listed.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Resource {
    /// The blocks of a type a shot holds at once.
    Blocks(BlockType),
    /// The shots a run holds.
    Shots,
    /// The instructions a batch holds.
    InstructionsPerBatch,
}

impl Resources {
    /// No amount stated: required, no need of any; provided, no limit on
    /// any. What a part leaves unstated, composition cannot refuse a chain
    /// on: a part that needs more blocks, shots or instructions a batch
    /// than this one holds is joined with it all the same.
    #[must_use]
    pub const fn unconstrained() -> Self {
        Self::new()
    }

    /// These resources, with `n` qubits.
    #[must_use]
    pub fn with_qubits(self, n: u32) -> Self {
        self.with_blocks(BlockType::qubit(), n)
    }

    /// These resources, with `n` blocks of `block_type`. A provider states
    /// 0 for a type it does not take.
    #[must_use]
    pub fn with_blocks(self, block_type: BlockType, n: u32) -> Self {
        self.with(Resource::Blocks(block_type), n)
    }

    /// These resources, with `n` shots per run.
    #[must_use]
    pub fn with_shots(self, n: u32) -> Self {
        self.with(Resource::Shots, n)
    }

    /// These resources, with `n` instructions per batch.
    #[must_use]
    pub fn with_instructions_per_batch(self, n: u32) -> Self {
        self.with(Resource::InstructionsPerBatch, n)
    }

    /// The blocks of `block_type` stated, if any.
    #[must_use]
    pub fn blocks(&self, block_type: &BlockType) -> Option<u32> {
        self.get(&Resource::Blocks(block_type.clone()))
    }
}

/// `` blocks of type `qubit`: 9 required, 8 provided ``.
impl fmt::Display for ResourceMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            key,
            required,
            provided,
        } = self;
        write!(f, "{key}: {required} required, {provided} provided")
    }
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Resource::Blocks(block_type) => write!(f, "blocks of type `{block_type}`"),
            Resource::Shots => f.write_str("shots per run"),
            Resource::InstructionsPerBatch => f.write_str("instructions per batch"),
        }
    }
}
