// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The block set's own capabilities ([`BlockCapabilities`]): the blocks of
//! each type a shot holds, and readout parities; and why they do not fit
//! ([`BlockCapabilitiesMismatch`]).

use std::collections::BTreeMap;
use std::fmt;

use executor_protocol_core::{BoundaryCompatible, SameNeed};

use crate::{BlockInstructionSet, BlockType};

/// The most blocks of each type a shot holds at once, and whether shots
/// use readout parities, resolved per shot as it runs
/// ([`Bit::Parity`](crate::Bit::Parity)).
///
/// A value reads per side, as the core's `docs/capabilities.md` says. As
/// a requirement, what arrives at a part from above, a count is the most
/// the shots reach, and `true` says they use readout parities, `false`
/// that they do not; a field left unstated is no need. As a provision,
/// the limits a part states (`Some` of it), a count is the most the part
/// takes, and `false` says it cannot take readout parities, `true` that
/// it can; a field left unstated is no limit. A part that limits nothing
/// provides `None`, not a value. A checked batch, though, may use no
/// block of a type the requirement arriving at its receiver does not
/// state.
///
/// Built from [`NONE`](Self::NONE), which states nothing, with the
/// builders below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockCapabilities {
    /// The blocks stated, by type: in the types' order, the order
    /// mismatches are listed.
    blocks: BTreeMap<BlockType, u32>,
    /// Readout parities, if stated.
    readout_parity: Option<bool>,
}

impl BlockCapabilities {
    /// Nothing stated: as a requirement, no need of any, which fits every
    /// provision (the set's `needs_nothing`); inside a provision, no limit
    /// on any. What a part leaves unstated, composition cannot refuse a
    /// chain on.
    pub const NONE: Self = Self {
        blocks: BTreeMap::new(),
        readout_parity: None,
    };

    /// These capabilities, with `n` qubits.
    #[must_use]
    pub fn with_qubits(self, n: u32) -> Self {
        self.with_blocks(BlockType::qubit(), n)
    }

    /// These capabilities, with `n` blocks of `block_type`, in place of any
    /// stated before. A provider states 0 for a type it does not take.
    #[must_use]
    pub fn with_blocks(mut self, block_type: BlockType, n: u32) -> Self {
        self.blocks.insert(block_type, n);
        self
    }

    /// These capabilities, with readout parities used, or taken, as `yes`
    /// says. A checked batch holding one is refused across a boundary
    /// whose required capabilities do not say `true`
    /// ([`ParityNotRequired`](crate::InstructionFault::ParityNotRequired)).
    #[must_use]
    pub fn with_readout_parity(mut self, yes: bool) -> Self {
        self.readout_parity = Some(yes);
        self
    }

    /// The blocks of `block_type` stated, if any.
    #[must_use]
    pub fn blocks(&self, block_type: &BlockType) -> Option<u32> {
        self.blocks.get(block_type).copied()
    }

    /// Readout parities, if stated.
    #[must_use]
    pub fn readout_parity(&self) -> Option<bool> {
        self.readout_parity
    }
}

/// Each block count required beyond the one provided, and readout parities
/// required and not provided, every one at once: block counts in their
/// types' order, then readout parities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockCapabilitiesMismatch {
    blocks: Vec<BlockCountMismatch>,
    readout_parity: bool,
}

impl BlockCapabilitiesMismatch {
    /// The block counts required above those provided, in their types'
    /// order.
    #[must_use]
    pub fn blocks(&self) -> &[BlockCountMismatch] {
        &self.blocks
    }

    /// Whether readout parities are required and not provided.
    #[must_use]
    pub fn readout_parity(&self) -> bool {
        self.readout_parity
    }

    /// Its pieces, as [`BlockInstructionSet`]'s `split_mismatch` gives
    /// them.
    pub(crate) fn split(self) -> Vec<(Self, SameNeed<BlockInstructionSet>)> {
        let blocks = self.blocks.into_iter().map(|count| {
            let block_type = count.block_type.clone();
            let same: SameNeed<BlockInstructionSet> =
                Box::new(move |a, b| a.blocks(&block_type) == b.blocks(&block_type));
            let piece = Self {
                blocks: vec![count],
                readout_parity: false,
            };
            (piece, same)
        });
        let readout_parity = self.readout_parity.then(|| {
            let same: SameNeed<BlockInstructionSet> =
                Box::new(|a, b| a.readout_parity == b.readout_parity);
            let piece = Self {
                blocks: Vec::new(),
                readout_parity: true,
            };
            (piece, same)
        });
        blocks.chain(readout_parity).collect()
    }
}

/// A count of blocks of one type required above the one provided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockCountMismatch {
    block_type: BlockType,
    required: u32,
    provided: u32,
}

impl BlockCountMismatch {
    /// The blocks' type.
    #[must_use]
    pub fn block_type(&self) -> &BlockType {
        &self.block_type
    }

    /// The count required.
    #[must_use]
    pub fn required(&self) -> u32 {
        self.required
    }

    /// The count provided, less than [`required`](Self::required).
    #[must_use]
    pub fn provided(&self) -> u32 {
        self.provided
    }
}

/// Each mismatch, joined by `; `: each short count with its block type and
/// the counts required and provided, then readout parities, when required
/// and not provided.
impl fmt::Display for BlockCapabilitiesMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut separator = "";
        for BlockCountMismatch {
            block_type,
            required,
            provided,
        } in &self.blocks
        {
            write!(
                f,
                "{separator}blocks of type `{block_type}`: {required} required, {provided} provided"
            )?;
            separator = "; ";
        }
        if self.readout_parity {
            write!(f, "{separator}readout parity required, not provided")?;
        }
        Ok(())
    }
}

impl std::error::Error for BlockCapabilitiesMismatch {}

/// Required capabilities fit provided ones when no count stated on both
/// sides is required above the one provided, and readout parities are not
/// required where provided says `false`.
impl BoundaryCompatible for BlockCapabilities {
    type Fit = ();
    type Mismatch = BlockCapabilitiesMismatch;

    fn fits(&self, provided: &Self) -> Result<(), BlockCapabilitiesMismatch> {
        let blocks: Vec<BlockCountMismatch> = self
            .blocks
            .iter()
            .filter_map(|(block_type, &required)| {
                let provided = provided.blocks(block_type)?;
                (required > provided).then(|| BlockCountMismatch {
                    block_type: block_type.clone(),
                    required,
                    provided,
                })
            })
            .collect();
        let readout_parity =
            self.readout_parity == Some(true) && provided.readout_parity == Some(false);
        if blocks.is_empty() && !readout_parity {
            Ok(())
        } else {
            Err(BlockCapabilitiesMismatch {
                blocks,
                readout_parity,
            })
        }
    }
}
