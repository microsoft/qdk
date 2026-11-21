// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Amounts a part requires of the part below, or provides the part above:
//! [`Resources`], and each amount provided short ([`ResourceMismatch`]).

use std::collections::BTreeMap;
use std::fmt;

use super::Forwarding;
use crate::{BlockType, BoundaryCompatible, InstructionSet};

/// Amounts: the most blocks of each type a shot holds at once, the most
/// shots a run holds, and the most instructions a batch holds.
///
/// Required, each is the most the part's shots, runs or batches reach;
/// provided, the most the part serves. A required amount fits a provided
/// one when it is no larger. An amount a part does not state is forwarded,
/// as the crate's `docs/capabilities.md` says: a provider that states none
/// serves any.
///
/// Built from [`new`](Self::new), which states nothing, with the builders
/// below.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resources {
    blocks: BTreeMap<BlockType, u32>,
    shots: Option<u32>,
    instructions_per_batch: Option<u32>,
}

impl Resources {
    /// No amount stated.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// These resources, with `n` qubits.
    #[must_use]
    pub fn with_qubits(self, n: u32) -> Self {
        self.with_blocks(BlockType::qubit(), n)
    }

    /// These resources, with `n` blocks of `block_type`. A provider states
    /// 0 for a type it does not serve.
    #[must_use]
    pub fn with_blocks(mut self, block_type: BlockType, n: u32) -> Self {
        self.blocks.insert(block_type, n);
        self
    }

    /// These resources, with `n` shots per run.
    #[must_use]
    pub fn with_shots(mut self, n: u32) -> Self {
        self.shots = Some(n);
        self
    }

    /// These resources, with `n` instructions per batch.
    #[must_use]
    pub fn with_instructions_per_batch(mut self, n: u32) -> Self {
        self.instructions_per_batch = Some(n);
        self
    }

    /// The blocks of `block_type` stated, if any.
    #[must_use]
    pub fn blocks(&self, block_type: &BlockType) -> Option<u32> {
        self.blocks.get(block_type).copied()
    }

    /// The shots per run stated, if any.
    #[must_use]
    pub fn shots(&self) -> Option<u32> {
        self.shots
    }

    /// The instructions per batch stated, if any.
    #[must_use]
    pub fn instructions_per_batch(&self) -> Option<u32> {
        self.instructions_per_batch
    }

    /// The blocks stated of each block type of `set`, in its order, 0 for
    /// a type not stated: the most a shot's instructions index, as a
    /// checked batch's check reads them.
    pub(crate) fn blocks_of(&self, set: &InstructionSet) -> Vec<u32> {
        (set.block_types())
            .map(|block_type| self.blocks(block_type).unwrap_or(0))
            .collect()
    }
}

impl BoundaryCompatible for Resources {
    type Fit = ();
    type Mismatch = Vec<ResourceMismatch>;

    /// Every amount `self` requires above the one `provided` states, in
    /// the order of the block types, then the shots, then the instructions
    /// per batch.
    fn fits(&self, provided: &Self) -> Result<(), Vec<ResourceMismatch>> {
        let mut short = Vec::new();
        for (block_type, &required) in &self.blocks {
            if let Some(provided) = provided.blocks(block_type)
                && required > provided
            {
                short.push(ResourceMismatch::Blocks {
                    block_type: block_type.clone(),
                    required,
                    provided,
                });
            }
        }
        if let (Some(required), Some(provided)) = (self.shots, provided.shots)
            && required > provided
        {
            short.push(ResourceMismatch::Shots { required, provided });
        }
        if let (Some(required), Some(provided)) =
            (self.instructions_per_batch, provided.instructions_per_batch)
            && required > provided
        {
            short.push(ResourceMismatch::InstructionsPerBatch { required, provided });
        }
        if short.is_empty() { Ok(()) } else { Err(short) }
    }
}

impl Forwarding for Resources {
    fn or(&self, other: &Self) -> Self {
        let mut blocks = other.blocks.clone();
        blocks.extend(self.blocks.iter().map(|(t, &n)| (t.clone(), n)));
        Self {
            blocks,
            shots: self.shots.or(other.shots),
            instructions_per_batch: self.instructions_per_batch.or(other.instructions_per_batch),
        }
    }

    fn unanswered(&self, answered: &Self) -> Self {
        let unless = |mine: Option<u32>, theirs: Option<u32>| mine.filter(|_| theirs.is_none());
        Self {
            blocks: (self.blocks.iter())
                .filter(|(t, _)| !answered.blocks.contains_key(*t))
                .map(|(t, &n)| (t.clone(), n))
                .collect(),
            shots: unless(self.shots, answered.shots),
            instructions_per_batch: unless(
                self.instructions_per_batch,
                answered.instructions_per_batch,
            ),
        }
    }

    fn stricter(&self, other: &Self) -> Self {
        let least = |a: Option<u32>, b: Option<u32>| match (a, b) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let mut blocks = other.blocks.clone();
        for (t, &n) in &self.blocks {
            blocks
                .entry(t.clone())
                .and_modify(|m| *m = (*m).min(n))
                .or_insert(n);
        }
        Self {
            blocks,
            shots: least(self.shots, other.shots),
            instructions_per_batch: least(
                self.instructions_per_batch,
                other.instructions_per_batch,
            ),
        }
    }

    fn states_nothing(&self) -> bool {
        self.blocks.is_empty() && self.shots.is_none() && self.instructions_per_batch.is_none()
    }
}

/// An amount required above the one provided.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResourceMismatch {
    /// More blocks of `block_type` than provided.
    Blocks {
        /// The block type.
        block_type: BlockType,
        /// The blocks required.
        required: u32,
        /// The blocks provided.
        provided: u32,
    },
    /// More shots per run than provided.
    Shots {
        /// The shots required.
        required: u32,
        /// The shots provided.
        provided: u32,
    },
    /// More instructions per batch than provided.
    InstructionsPerBatch {
        /// The instructions required.
        required: u32,
        /// The instructions provided.
        provided: u32,
    },
}

impl fmt::Display for ResourceMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceMismatch::Blocks {
                block_type,
                required,
                provided,
            } => write!(
                f,
                "requires {required} blocks of type `{block_type}`, and {provided} are provided"
            ),
            ResourceMismatch::Shots { required, provided } => write!(
                f,
                "requires {required} shots per run, and {provided} are provided"
            ),
            ResourceMismatch::InstructionsPerBatch { required, provided } => write!(
                f,
                "requires {required} instructions per batch, and {provided} are provided"
            ),
        }
    }
}
