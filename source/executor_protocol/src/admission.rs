// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The admission: whether an executor can serve a driver's shots, decided
//! once, by the `>>` that makes a runner, before any shot runs.
//!
//! A driver states what its shots [`Needs`], an executor what it
//! [`Capabilities`] can ever serve, and each adapter between them adapts
//! both: the needs for the part below it, the offer for the part above. The
//! `>>` compares the needs that reach the executor, adapted by every
//! adapter, with the executor's own offer ([`Needs::fit`]), so that the
//! verdict and its numbers are the same wherever the brackets go. It
//! refuses the chain with every [`UnmetNeed`] and each part's needs and
//! offer ([`AdmissionFaults`]), so a user sees which part asks too much or
//! offers too little.
//!
//! The admission uses no adapted offer: an offer is a most per block type,
//! one type at a time, which cannot state what an adapter that draws two
//! block types from one pool below can serve, while the needs it adapts
//! down can.

mod capabilities;
mod needs;
pub(crate) mod part_list;

use std::fmt;

use crate::BlockType;

pub use capabilities::Capabilities;
pub use needs::Needs;

impl Needs {
    /// Every need `offered` does not meet, in the order of the block types
    /// and then the readout parities; `Ok` when it meets them all.
    pub fn fit(&self, offered: &Capabilities) -> Result<(), Vec<UnmetNeed>> {
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
                "needs {needed} blocks of type `{block_type}`, and {offered} are offered"
            ),
            UnmetNeed::ReadoutParity => {
                f.write_str("uses readout parities, which are not resolved")
            }
        }
    }
}

/// Why the `>>` that makes a runner refused its chain's admission: the
/// needs that reach the executor and the executor's own offer, every need
/// unmet there, and each part's needs and offer, so that a user sees which
/// part asks too much or offers too little.
///
/// Each part is named by its type (`std::any::type_name`). Every field is
/// the same whatever the brackets of the `>>`s.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct AdmissionFaults {
    /// The needs that reach the executor: the driver's, adapted by every
    /// adapter of the chain, top to bottom.
    pub needed: Needs,
    /// The executor's own offer, before any adapter adapts it.
    pub offered: Capabilities,
    /// Every need of `needed` that `offered` does not meet.
    pub unmet: Vec<UnmetNeed>,
    /// The driver's needs, then each adapter's needs for the part below it,
    /// top to bottom.
    pub needs_by_part: Vec<(&'static str, Needs)>,
    /// The executor's offer, then each adapter's offer to the part above
    /// it, bottom to top.
    pub offers_by_part: Vec<(&'static str, Capabilities)>,
}

impl fmt::Display for AdmissionFaults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the executor cannot serve the driver's shots:")?;
        for unmet in &self.unmet {
            write!(f, " {unmet};")?;
        }
        Ok(())
    }
}

impl std::error::Error for AdmissionFaults {}
