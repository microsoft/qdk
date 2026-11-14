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
//! below, as the adapters below adapt it ([`Needs::fit`]). It refuses the
//! chain when any of them falls short.
//!
//! An adapter's offer is the one below it, adapted, so it mostly falls
//! short where the part below does. So a refusal names each part that is
//! short on its own ([`ShortOffer`]): the executor, when its offer falls
//! short of the needs that reach it; an adapter, when its offer would fall
//! short even if the part below gave exactly what the adapter needs below.
//! An adapter is named for what it cannot carry or hold itself, such as
//! readout parities, or more logical qubits than its decoder takes, even
//! while the executor is short too; an adapter that only passes a shortage
//! up, such as a profiler, is not. A fallback names an adapter that is not
//! short on its own, but whose offer falls short over a larger offer below
//! (one that shrinks as the offer below grows): where its offer falls short
//! while the part below meets what it needs below. So the fallback does not
//! name it while the part below is short, of any kind of need. Every
//! boundary is checked the same way wherever the brackets go, so the
//! verdict, the parts named and their numbers are too. The refusal also
//! lists each part's needs and offer ([`AdmissionFaults`]), and names each
//! part by its position in the chain and its name ([`ChainPart`]).
//!
//! The verdict comes from the boundaries alone: the chain is refused when
//! the needs at a boundary exceed the offer there, whichever parts the
//! refusal names. An adapter that asks below less than it needs, against
//! [`Adapt::needs_below`](crate::Adapt::needs_below)'s rule, may be short
//! on its own though every boundary fits: it refuses no chain, but a
//! refusal another part causes may name it.
//!
//! At the executor's boundary, the executor's own offer meets the needs
//! that reach it, which no adapted offer replaces: an offer is a most per
//! block type, one type at a time, which cannot state what an adapter that
//! draws two block types from one pool below can serve, while the needs it
//! adapts down can. Such an adapter's offer may admit what the executor's
//! boundary then refuses.

pub(crate) mod by_part;
mod capabilities;
mod chain_part;
mod needs;

use std::fmt;

use crate::BlockType;
use by_part::ByPart;

pub use capabilities::Capabilities;
pub use chain_part::ChainPart;
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
/// part short on its own, and each part's needs and offer, so that a user
/// sees which part asks too much or offers too little.
///
/// Each part is named by its position in the chain and its name
/// ([`ChainPart`]). Every field is the same whatever the brackets of the
/// `>>`s. Its message names each part short on its own and every need it
/// falls short of; each part's needs and offer are in the fields only.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AdmissionFaults {
    /// Each part short on its own, or named by the fallback
    /// ([`ShortOffer`]), bottom to top, in the order of `offers_by_part`:
    /// the executor first, if it falls short. A part whose offer falls
    /// short only because the part below falls short, such as a profiler
    /// over a short executor, is not listed.
    pub short_offers: Vec<ShortOffer>,
    /// The driver's needs, then each adapter's needs for the part below it,
    /// top to bottom.
    pub needs_by_part: Vec<(ChainPart, Needs)>,
    /// The executor's offer, then each adapter's offer to the part above
    /// it, given the offer below it, bottom to top.
    pub offers_by_part: Vec<(ChainPart, Capabilities)>,
}

impl AdmissionFaults {
    /// The faults of the chain whose parts' needs and offers `by_part`
    /// lists, if an offer falls short at a boundary, the needs of the part
    /// above against the offer of the part below: each part short on its
    /// own, by its own offer, or by the fallback ([`ShortOffer::offered`]).
    pub(crate) fn of(by_part: ByPart) -> Option<Self> {
        let ByPart {
            needs,
            offers,
            own_offers,
            ..
        } = by_part;
        debug_assert_eq!(
            needs.len(),
            offers.len(),
            "a need and an offer per boundary"
        );
        let mut refused = false;
        let mut short_offers = Vec::new();
        let mut below_fits = true;
        for (((part, offered), own), (_, needed)) in
            offers.iter().zip(&own_offers).zip(needs.iter().rev())
        {
            let fit = needed.fit(offered);
            refused |= fit.is_err();
            let short = match (needed.fit(own), &fit) {
                (Err(unmet), _) => Some((own, unmet)),
                (Ok(()), Err(unmet)) if below_fits => Some((offered, unmet.clone())),
                _ => None,
            };
            if let Some((offered, unmet)) = short {
                short_offers.push(ShortOffer {
                    part: part.clone(),
                    needed: needed.clone(),
                    offered: offered.clone(),
                    unmet,
                });
            }
            below_fits = fit.is_ok();
        }
        refused.then_some(Self {
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

/// A part a refusal names, below one boundary of a chain: its offer on its
/// own falls short of the needs that reach it, so not only because the part
/// below it falls short. Or, by the fallback, an adapter that is not short
/// on its own, but whose offer, given the real offer below, falls short
/// where the part below meets what the adapter needs below; so the fallback
/// does not name it while the part below is short.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShortOffer {
    /// The part: the executor or an adapter.
    pub part: ChainPart,
    /// The needs that reach it: the driver's, adapted by each adapter above
    /// it.
    pub needed: Needs,
    /// The offer that falls short. For the executor, its offer. For an
    /// adapter, its offer on its own: its offer given exactly what it needs
    /// below ([`Adapt::offers_above`](crate::Adapt::offers_above)), the
    /// real offer below with each block type it needs below at its count
    /// and no other, and readout parities resolved only if it uses them
    /// below. When that offer meets `needed` (the fallback), its offer
    /// given the real offer below.
    pub offered: Capabilities,
    /// Each need of `needed` that `offered` does not meet.
    pub unmet: Vec<UnmetNeed>,
}

impl fmt::Display for ShortOffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} cannot serve the needs that reach it (", self.part)?;
        for (i, unmet) in self.unmet.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{unmet}")?;
        }
        f.write_str(")")
    }
}
