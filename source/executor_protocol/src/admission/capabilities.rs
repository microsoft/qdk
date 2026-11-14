// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an executor can ever serve: its [`Capabilities`].
//!
//! An executor states its offer once, so that the `>>` that makes a runner
//! compares it with the driver's needs, as they reach the executor, before
//! any shot runs, and the protocol keeps its limits for each run and each
//! `execute`, so that no part checks them itself. An adapter adapts the
//! offer for the part above it, as it adapts the needs for the part below.

use std::collections::BTreeMap;

use crate::BlockType;

/// What an executor can ever serve: the most blocks of each type a shot
/// may hold, the most shots a run may have, the most instructions a batch
/// may hold, whether a run may fix its seed, and whether it resolves
/// readout parities.
///
/// Built from [`unlimited`](Self::unlimited), also its `Default`, with the
/// builders below, since a struct literal is refused outside the crate.
/// Where each limit is checked: the blocks and the readout parities by the
/// `>>` that makes a runner, against the driver's needs; the shots and the
/// seed by the runner, at each run; the instructions at each `execute`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Capabilities {
    /// The most blocks of each type a shot may hold at once: a type absent
    /// is not offered, and `None` has no limit.
    pub max_blocks: BTreeMap<BlockType, Option<u32>>,
    /// The most shots a run may have; `None`: no limit.
    pub max_shots: Option<u32>,
    /// The most instructions one batch may hold; `None`: no limit.
    pub max_instructions: Option<u32>,
    /// Whether a run may fix its seed, so that it gives the same records
    /// each time; a quantum computer's measurements cannot be seeded.
    pub seedable: bool,
    /// Whether it resolves each readout parity
    /// ([`Bit::Parity`](crate::Bit::Parity)) per shot, as the shot runs.
    pub resolves_readout_parity: bool,
}

impl Capabilities {
    /// Qubits without limit, and no other block type; any number of shots
    /// and of instructions; seedable; resolves readout parities.
    #[must_use]
    pub fn unlimited() -> Self {
        Self {
            max_blocks: BTreeMap::from([(BlockType::qubit(), None)]),
            max_shots: None,
            max_instructions: None,
            seedable: true,
            resolves_readout_parity: true,
        }
    }

    /// This offer, with at most `n` qubits.
    #[must_use]
    pub fn max_qubits(self, n: u32) -> Self {
        self.max_blocks(BlockType::qubit(), n)
    }

    /// This offer, with at most `n` blocks of `block_type`, offered even if
    /// it was not.
    #[must_use]
    pub fn max_blocks(mut self, block_type: BlockType, n: u32) -> Self {
        self.max_blocks.insert(block_type, Some(n));
        self
    }

    /// This offer, with at most `n` shots per run.
    #[must_use]
    pub fn max_shots(mut self, n: u32) -> Self {
        self.max_shots = Some(n);
        self
    }

    /// This offer, with at most `n` instructions per batch.
    #[must_use]
    pub fn max_instructions(mut self, n: u32) -> Self {
        self.max_instructions = Some(n);
        self
    }

    /// This offer, refusing a run that fixes its seed.
    #[must_use]
    pub fn not_seedable(mut self) -> Self {
        self.seedable = false;
        self
    }

    /// This offer, without resolving readout parities.
    #[must_use]
    pub fn without_readout_parity(mut self) -> Self {
        self.resolves_readout_parity = false;
        self
    }
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::unlimited()
    }
}
