// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The block set's own capabilities ([`BlockCapabilities`]): the blocks of
//! each type a shot holds, and readout parities; and why they do not fit
//! ([`BlockCapabilitiesMismatch`]).

use std::collections::BTreeMap;
use std::fmt;

use executor_protocol_core::BoundaryCompatible;

use crate::BlockType;

/// The most blocks of each type a shot holds at once, and whether shots
/// use readout parities, resolved per shot as it runs
/// ([`Bit::Parity`](crate::Bit::Parity)).
///
/// Required, a count is the most the part's shots reach, and `true` says
/// they use readout parities, `false` that they do not; provided, a count
/// is the most the part takes, and `false` says it cannot take readout
/// parities, `true` that it can. A part that does not state one neither
/// needs nor limits it. A checked batch, though, may use no block of a
/// type the requirement arriving at its receiver does not state.
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
    /// Nothing stated: required, no need of any; provided, no limit on
    /// any. What a part leaves unstated, composition cannot refuse a chain
    /// on.
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

/// Each count and feature required beyond what is provided, every one at
/// once: block counts in their types' order, then readout parities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockCapabilitiesMismatch {
    /// The block counts required above those provided.
    pub blocks: Vec<BlockCountMismatch>,
    /// Whether readout parities are required and not provided.
    pub readout_parity: bool,
}

/// A count of blocks of one type required above the one provided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockCountMismatch {
    /// The blocks' type.
    pub block_type: BlockType,
    /// The count required.
    pub required: u32,
    /// The count provided, less than `required`.
    pub provided: u32,
}

/// Each mismatch joined by `; `, as `` blocks of type `qubit`: 9 required,
/// 8 provided; readout parity required, not provided ``.
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
