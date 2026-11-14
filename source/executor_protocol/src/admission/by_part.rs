// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Each part's needs and offer, which the admission compares and reports:
//! [`ByPart`].
//!
//! The traits' hidden `needs_by_part` and `offers_by_part` take one, and
//! code outside the crate cannot name it, because this module is private:
//! so those methods cannot be overridden outside the crate, and every part
//! is listed as the crate lists it. The `>>` that makes a runner lists
//! every part, since the admission compares, at each boundary, the needs
//! of the part above with the offer of the part below; a part that only
//! asks its parts' needs or offer lists nothing.

use crate::{Capabilities, ChainPart, Needs};

/// Each part's needs, top to bottom, and offer, bottom to top, each with
/// the part's position and name; or nothing, when the caller wants only
/// the chain's own needs and offer.
pub struct ByPart {
    keep: bool,
    /// The needs listed, in the order they were.
    pub(crate) needs: Vec<(ChainPart, Needs)>,
    /// The offers listed, in the order they were.
    pub(crate) offers: Vec<(ChainPart, Capabilities)>,
    /// For each offer listed, the part's offer on its own: given an offer
    /// below that meets exactly what it needs below, for an adapter; its
    /// offer, for the executor.
    pub(crate) own_offers: Vec<Capabilities>,
}

impl ByPart {
    /// A list that keeps what it is told.
    pub(crate) fn new() -> Self {
        Self {
            keep: true,
            needs: Vec::new(),
            offers: Vec::new(),
            own_offers: Vec::new(),
        }
    }

    /// A list that keeps nothing, for a part that only asks its parts'
    /// needs or offer.
    pub(crate) fn ignored() -> Self {
        Self {
            keep: false,
            ..Self::new()
        }
    }

    /// Lists `needs` as those of the part `name`, the next one down: the
    /// driver first, then each adapter.
    pub(crate) fn note_needs(&mut self, name: &str, needs: &Needs) {
        if self.keep {
            let position = self.needs.len();
            self.needs
                .push((ChainPart::new(position, name), needs.clone()));
        }
    }

    /// Lists `offer` as that of the executor `name`, once every part's
    /// needs are listed.
    pub(crate) fn note_executor_offer(&mut self, name: &str, offer: &Capabilities) {
        if self.keep {
            self.note_offer(name, offer, offer.clone());
        }
    }

    /// Lists `above` as the offer of the adapter `name`, the next one up,
    /// given `below`, once every part's needs are listed; and as its own
    /// offer, what `offers_above` gives for an offer below that meets
    /// exactly what the adapter needs below: `below`, with each block type
    /// it needs at its count and no other, and readout parities resolved
    /// only when it uses them.
    pub(crate) fn note_adapter_offer(
        &mut self,
        name: &str,
        above: &Capabilities,
        below: &Capabilities,
        offers_above: impl FnOnce(Capabilities) -> Capabilities,
    ) {
        if self.keep {
            let (_, needs_below) = &self.needs[self.next_offer_position()];
            let exactly = Capabilities {
                max_blocks: needs_below
                    .blocks
                    .iter()
                    .map(|(block_type, &n)| (block_type.clone(), Some(n)))
                    .collect(),
                resolves_readout_parity: needs_below.uses_readout_parity,
                ..below.clone()
            };
            self.note_offer(name, above, offers_above(exactly));
        }
    }

    /// The position of the part whose offer is listed next: the executor's,
    /// below every part whose needs are listed, then each adapter's, up.
    fn next_offer_position(&self) -> usize {
        self.needs.len() - self.offers.len()
    }

    fn note_offer(&mut self, name: &str, offer: &Capabilities, own: Capabilities) {
        let part = ChainPart::new(self.next_offer_position(), name);
        self.offers.push((part, offer.clone()));
        self.own_offers.push(own);
    }
}
