// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an executor can ever serve: its [`Offer`].
//!
//! An executor states its offer once, so that the `>>` that makes a runner
//! compares it with the driver's needs, as they reach the executor, before
//! any shot runs, and the protocol keeps its limits for each run and each
//! `execute`, so that no part checks them itself. An adapter adapts the
//! offer for the part above it, as it adapts the needs for the part below,
//! and the `>>` compares its offer with the needs that reach it too. A
//! run's most shots and whether it may fix its seed are those every part's
//! offer meets: an adapter can lower them, never raise them.

use std::collections::BTreeMap;

use crate::BlockType;

/// What an executor can ever serve: the most blocks of each type a shot
/// may hold, the most shots a run may have, the most instructions a batch
/// may hold, whether a run may fix its seed, whether it resolves readout
/// parities, and whether its shots are replayable.
///
/// Built from [`unlimited`](Self::unlimited), also its `Default`, with the
/// builders below, since a struct literal is refused outside the crate.
/// Where each limit is checked: the blocks, the readout parities and the
/// replay, with the seed it needs, by the `>>` that makes a runner, against
/// the needs that reach each part; the shots and the seed by the runner, at
/// each run, against every part's offer; the instructions at each
/// `execute`, against every batch's count, checked or not.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Offer {
    /// The most blocks of each type a shot may hold at once: a type absent
    /// is not offered, and `None` has no limit.
    pub max_blocks: BTreeMap<BlockType, Option<u32>>,
    /// The most shots a run may have; `None`: no limit. A run's limit is
    /// the fewest any part of the chain offers, so an adapter can lower the
    /// executor's, never raise it.
    pub max_shots: Option<u32>,
    /// The most instructions one batch may hold; `None`: no limit. Every
    /// batch sent to the executor is refused beyond it, checked or not, so
    /// an executor that holds a batch in a buffer of fixed size relies on
    /// it.
    pub max_instructions: Option<u32>,
    /// Whether a run may fix its seed, so that a run in which no part sends
    /// an `execute` again gives the same records each time; a quantum
    /// computer's measurements cannot be seeded. A chain is seedable only
    /// when every part's offer is.
    pub seedable: bool,
    /// Whether it resolves each readout parity
    /// ([`Bit::Parity`](crate::Bit::Parity)) per shot, as the shot runs.
    pub resolves_readout_parity: bool,
    /// Whether its shots are replayable: with a fixed seed, for a run in
    /// which no part sends an `execute` again, a shot's readouts depend on
    /// the chain, the seed and the shot's id, not on the shots run beside
    /// it, so a shot run again alone gives them back. A part that draws a
    /// run's shots together is not, as the crate's
    /// `docs/writing-a-part.md` says ("Seeds"). Replay fixes the seed, so
    /// the admission refuses this offer where the needs that reach it ask
    /// for replay ([`Needs::replayable`](crate::Needs::replayable)) unless
    /// it is both replayable and [`seedable`](Self::seedable).
    pub replayable: bool,
}

impl Offer {
    /// Qubits without limit, and no other block type; any number of shots
    /// and of instructions; seedable; resolves readout parities;
    /// replayable.
    #[must_use]
    pub fn unlimited() -> Self {
        Self {
            max_blocks: BTreeMap::from([(BlockType::qubit(), None)]),
            max_shots: None,
            max_instructions: None,
            seedable: true,
            resolves_readout_parity: true,
            replayable: true,
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

    /// This offer, refusing a run that fixes its seed, and so meeting no
    /// need for replay, whatever its [`replayable`](Self::replayable) says.
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

    /// This offer, with shots that are not replayable.
    #[must_use]
    pub fn not_replayable(mut self) -> Self {
        self.replayable = false;
        self
    }
}

impl Default for Offer {
    fn default() -> Self {
        Self::unlimited()
    }
}
