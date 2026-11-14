// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a chain's shots need of the executor that runs them: [`Needs`].
//!
//! A driver states its needs once, before any shot runs, so that the `>>`
//! that makes a runner refuses an executor too small for them, and every
//! part is provisioned for them: an adapter adapts them for the part below
//! it, such as a code turning one logical qubit into seven physical ones,
//! and the executor sizes each shot's state from what reaches it.

use std::collections::BTreeMap;

use crate::BlockType;

/// What a chain's shots need: the most blocks of each type a shot holds at
/// once, and whether its instructions use readout parities.
///
/// A driver states it ([`Drive::needs`](crate::Drive::needs)); each
/// adapter adapts it for the part below
/// ([`Adapt::needs_below`](crate::Adapt::needs_below)); and the executor
/// is provisioned with what reaches it. A shot's block indices of each
/// type stay below its count here, which each `execute` checks. Its
/// `Default` needs nothing.
///
/// Built from [`qubits`](Self::qubits) or its `Default`, with the builders
/// below, since a struct literal is refused outside the crate: it is
/// `#[non_exhaustive]`, so that a new need can come as a field whose
/// default states nothing, without breaking the parts that build one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Needs {
    /// The most blocks of each type alive at once in a shot; a type absent
    /// is needed 0 times.
    pub blocks: BTreeMap<BlockType, u32>,
    /// Whether its instructions hold a readout parity
    /// ([`Bit::Parity`](crate::Bit::Parity)), which the executor must
    /// resolve per shot, as the shot runs. An `execute` holding one is
    /// refused at a boundary whose needs, as they reach it, say `false`
    /// here
    /// ([`ParityNotProvisioned`](crate::InstructionFault::ParityNotProvisioned)).
    pub uses_readout_parity: bool,
}

impl Needs {
    /// `n` qubits, and nothing else: what a QIR program needs.
    #[must_use]
    pub fn qubits(n: u32) -> Self {
        Self::default().blocks(BlockType::qubit(), n)
    }

    /// These needs, with `n` blocks of `block_type` in place of their own.
    #[must_use]
    pub fn blocks(mut self, block_type: BlockType, n: u32) -> Self {
        self.blocks.insert(block_type, n);
        self
    }

    /// These needs, with readout parities used.
    #[must_use]
    pub fn with_readout_parity(mut self) -> Self {
        self.uses_readout_parity = true;
        self
    }

    /// How many blocks of `block_type` a shot holds at most at once; 0 when
    /// none.
    #[must_use]
    pub fn blocks_of(&self, block_type: &BlockType) -> u32 {
        self.blocks.get(block_type).copied().unwrap_or(0)
    }
}
