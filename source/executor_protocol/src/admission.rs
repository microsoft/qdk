// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The admission: whether a chain can serve its driver's shots, decided
//! once, by the `>>` that makes a runner, before any shot runs.
//!
//! A driver states what its shots [`Needs`], an executor what it
//! [`Capabilities`] can ever serve, and each adapter between them adapts
//! both: the needs for the part below it, the offer for the part above.
//! The `>>` checks every boundary of the chain: the needs of the part above
//! it, as the adapters above adapt them, against the offer of the part
//! below, as the adapters below adapt it ([`Needs::fit`]). So an adapter
//! refuses what it cannot carry, such as readout parities, and the
//! executor what it cannot hold. Both are listed the same way wherever the
//! brackets go, so the verdict and its numbers are too. It refuses the
//! chain with each offer that falls short ([`ShortOffer`]), naming its
//! part, and each part's needs and offer ([`AdmissionFaults`]).
//!
//! At the executor's boundary, the executor's own offer meets the needs
//! that reach it, which no adapted offer replaces: an offer is a most per
//! block type, one type at a time, which cannot state what an adapter that
//! draws two block types from one pool below can serve, while the needs it
//! adapts down can. Such an adapter's offer may admit what the executor's
//! boundary then refuses.

pub(crate) mod by_part;
mod capabilities;
mod needs;

use std::fmt;

use crate::BlockType;
use by_part::ByPart;

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
                "needs {needed} of block type `{block_type}`, where the offer allows at most {offered}"
            ),
            UnmetNeed::ReadoutParity => {
                f.write_str("uses readout parities, and the offer does not resolve them")
            }
        }
    }
}

/// Why the `>>` that makes a runner refused its chain's admission: each
/// offer that does not meet the needs that reach it, and each part's needs
/// and offer, so that a user sees which part asks too much or offers too
/// little.
///
/// Each part is named by its type (`std::any::type_name`). Every field is
/// the same whatever the brackets of the `>>`s. Its message names each
/// part whose offer falls short and every need it does not meet; each
/// part's needs and offer are in the fields only.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AdmissionFaults {
    /// Each offer that falls short, bottom to top, in the order of
    /// `offers_by_part`: the executor's first, if it does.
    pub short_offers: Vec<ShortOffer>,
    /// The driver's needs, then each adapter's needs for the part below it,
    /// top to bottom.
    pub needs_by_part: Vec<(&'static str, Needs)>,
    /// The executor's offer, then each adapter's offer to the part above
    /// it, bottom to top.
    pub offers_by_part: Vec<(&'static str, Capabilities)>,
}

impl AdmissionFaults {
    /// The faults of the chain whose parts' needs and offers `by_part`
    /// lists, if an offer falls short: at each boundary, the needs of the
    /// part above against the offer of the part below.
    pub(crate) fn of(by_part: ByPart) -> Option<Self> {
        let ByPart { needs, offers, .. } = by_part;
        debug_assert_eq!(
            needs.len(),
            offers.len(),
            "a need and an offer per boundary"
        );
        let short_offers: Vec<ShortOffer> = offers
            .iter()
            .zip(needs.iter().rev())
            .filter_map(|((part, offered), (_, needed))| {
                needed.fit(offered).err().map(|unmet| ShortOffer {
                    part,
                    needed: needed.clone(),
                    offered: offered.clone(),
                    unmet,
                })
            })
            .collect();
        (!short_offers.is_empty()).then_some(Self {
            short_offers,
            needs_by_part: needs,
            offers_by_part: offers,
        })
    }
}

impl fmt::Display for AdmissionFaults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the chain is not admitted: ")?;
        for (i, short) in self.short_offers.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{short}")?;
        }
        Ok(())
    }
}

impl std::error::Error for AdmissionFaults {}

/// An offer that does not meet the needs that reach it, at one boundary of
/// a chain: the part below the boundary offers too little for the part
/// above.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShortOffer {
    /// The part whose offer falls short, named by its type: the executor or
    /// an adapter.
    pub part: &'static str,
    /// The needs that reach it: the driver's, adapted by each adapter above
    /// it.
    pub needed: Needs,
    /// Its offer to the part above: the executor's own, or an adapter's,
    /// given the offer below it.
    pub offered: Capabilities,
    /// Every need of `needed` that `offered` does not meet.
    pub unmet: Vec<UnmetNeed>,
}

impl fmt::Display for ShortOffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` cannot serve the needs that reach it (", self.part)?;
        for (i, unmet) in self.unmet.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{unmet}")?;
        }
        f.write_str(")")
    }
}
