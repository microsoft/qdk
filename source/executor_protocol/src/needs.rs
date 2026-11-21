// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a chain's shots need of the executor that runs them, [`Needs`],
//! and each need an offer does not meet ([`UnmetNeed`]).
//!
//! A driver states its needs once, before any shot runs, so that the
//! [`on`](crate::DriverJoins::on) that makes a runner refuses an executor
//! too small for them, and every
//! part gets ready for them on each run: an adapter adapts them for the
//! part below it, such as a code turning one logical qubit into seven
//! physical ones, and the executor sizes each shot's state from what
//! reaches it.

use std::collections::BTreeMap;
use std::fmt;

use crate::{BlockType, Offer};

/// What a chain's shots need: the most blocks of each type a shot holds at
/// once, whether its instructions use readout parities, and whether a shot
/// is run again alone.
///
/// A driver states it ([`Driver::needs`](crate::Driver::needs));
/// each adapter adapts it for the part below
/// ([`Adapter::needs_below`](crate::Adapter::needs_below)); and
/// the executor gets ready for what reaches it
/// ([`Executor::ready`](crate::Executor::ready)). A shot's block indices
/// of each type stay below its count here, which each `execute` checks for
/// a [`checked`](crate::Batch::checked) batch. Its `Default` needs nothing.
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
    /// resolve per shot, as the shot runs. A checked batch holding one is
    /// refused at a boundary whose needs, as they reach it, say `false`
    /// here
    /// ([`ParityNotNeeded`](crate::InstructionFault::ParityNotNeeded)).
    pub uses_readout_parity: bool,
    /// Whether a shot is run again alone and must give its readouts back,
    /// so that the offer that meets these needs must be
    /// [`replayable`](crate::Offer::replayable) and, since the shot is run
    /// with the run's seed, [`seedable`](crate::Offer::seedable). A driver
    /// states it when it runs one shot of an earlier run again, such as a
    /// debugger stepping into the shot that failed.
    pub replayable: bool,
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

    /// These needs, with shots that are run again alone.
    #[must_use]
    pub fn with_replay(mut self) -> Self {
        self.replayable = true;
        self
    }

    /// How many blocks of `block_type` a shot holds at most at once; 0 when
    /// none.
    #[must_use]
    pub fn blocks_of(&self, block_type: &BlockType) -> u32 {
        self.blocks.get(block_type).copied().unwrap_or(0)
    }

    /// Every need `offered` does not meet, in the order of the block types,
    /// then the readout parities, then the replay; `Ok` when it meets them
    /// all.
    pub fn fit(&self, offered: &Offer) -> Result<(), Vec<UnmetNeed>> {
        let mut unmet = Vec::new();
        for (block_type, &needed) in &self.blocks {
            if needed == 0 {
                continue;
            }
            match offered.max_blocks.get(block_type) {
                Some(None) => {}
                Some(&Some(most)) if needed <= most => {}
                most => unmet.push(UnmetNeed::Blocks {
                    block_type: block_type.clone(),
                    needed,
                    offered: most.copied().flatten().unwrap_or(0),
                }),
            }
        }
        if self.uses_readout_parity && !offered.resolves_readout_parity {
            unmet.push(UnmetNeed::ReadoutParity);
        }
        if self.replayable && !(offered.replayable && offered.seedable) {
            unmet.push(UnmetNeed::Replayable);
        }
        if unmet.is_empty() { Ok(()) } else { Err(unmet) }
    }
}

/// A need an offer does not meet.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnmetNeed {
    /// More blocks of `block_type` than the offer's most; `offered` is 0
    /// when it offers none.
    Blocks {
        /// The block type.
        block_type: BlockType,
        /// The most a shot holds at once.
        needed: u32,
        /// The most the offer allows.
        offered: u32,
    },
    /// The instructions hold readout parities, and the offer does not
    /// resolve them.
    ReadoutParity,
    /// A shot is run again alone, with the run's seed, and the offer's
    /// shots are not [`replayable`](Offer::replayable), or a run may not fix
    /// its seed ([`seedable`](Offer::seedable)): replay needs both.
    Replayable,
}

impl fmt::Display for UnmetNeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnmetNeed::Blocks {
                block_type,
                needed,
                offered,
            } => write!(
                f,
                "needs {needed} of block type `{block_type}`, where the offer allows at most {offered}"
            ),
            UnmetNeed::ReadoutParity => {
                f.write_str("uses readout parities, and the offer does not resolve them")
            }
            UnmetNeed::Replayable => f.write_str(
                "runs a shot again alone, and the offer's shots are not replayable with a fixed \
                 seed",
            ),
        }
    }
}
