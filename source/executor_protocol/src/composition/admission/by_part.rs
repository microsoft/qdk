// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Each part's needs and offer, which the admission compares and reports:
//! [`ByPart`].
//!
//! The traits' hidden `needs_by_part` and `offers_by_part` take one, and
//! code outside the crate cannot name it, because no public path reaches
//! it:
//! so those methods cannot be overridden outside the crate, and every part
//! is listed as the crate lists it. The `>>` that makes a runner lists
//! every part, since the admission compares, at each boundary, the needs
//! of the part above with the offer of the part below.

use crate::{Needs, Offer, PartId};

/// Each part's needs, top to bottom, and offer, bottom to top, each with
/// the part's position and name.
pub struct ByPart {
    /// The needs listed, in the order they were.
    pub(crate) needs: Vec<(PartId, Needs)>,
    /// The offers listed, in the order they were.
    pub(crate) offers: Vec<(PartId, Offer)>,
    /// For each offer listed, the part's offer on its own: given an offer
    /// below that meets exactly what it needs below, for an adapter; its
    /// offer, for the executor.
    pub(crate) own_offers: Vec<Offer>,
}

impl ByPart {
    /// An empty list.
    pub(crate) fn new() -> Self {
        Self {
            needs: Vec::new(),
            offers: Vec::new(),
            own_offers: Vec::new(),
        }
    }

    /// Lists `needs` as those of the part `name`, the next one down: the
    /// driver first, then each adapter.
    pub(crate) fn note_needs(&mut self, name: &str, needs: &Needs) {
        let position = self.needs.len();
        self.needs
            .push((PartId::new(position, name), needs.clone()));
    }

    /// Lists `offer` as that of the executor `name`, once every part's
    /// needs are listed.
    pub(crate) fn note_executor_offer(&mut self, name: &str, offer: &Offer) {
        self.note_offer(name, offer, offer.clone());
    }

    /// Lists `above` as the offer of the adapter `name`, the next one up,
    /// given `below`, once every part's needs are listed; and as its own
    /// offer, what `offers_above` gives for an offer below that meets
    /// exactly what the adapter needs below, as
    /// [`ShortOffer::offered`](crate::ShortOffer::offered) says.
    pub(crate) fn note_adapter_offer(
        &mut self,
        name: &str,
        above: &Offer,
        below: &Offer,
        offers_above: impl FnOnce(Offer) -> Offer,
    ) {
        let (_, needs_below) = &self.needs[self.next_offer_position()];
        let exactly = Offer {
            max_blocks: needs_below
                .blocks
                .iter()
                .map(|(block_type, &n)| (block_type.clone(), Some(n)))
                .collect(),
            resolves_readout_parity: needs_below.uses_readout_parity,
            replayable: needs_below.replayable,
            seedable: below.seedable || needs_below.replayable,
            ..below.clone()
        };
        self.note_offer(name, above, offers_above(exactly));
    }

    /// The part whose needs were listed first: the top part of the chain
    /// whose needs were listed, the driver for a runner's chain; `None` if
    /// none was.
    pub(crate) fn first_needing(&self) -> Option<PartId> {
        self.needs.first().map(|(part, _)| part.clone())
    }

    /// The part whose offer was listed last: the top part of the chain
    /// whose offers were just listed; `None` if none was.
    pub(crate) fn last_offered(&self) -> Option<PartId> {
        self.offers.last().map(|(part, _)| part.clone())
    }

    /// The position of the part whose offer is listed next: the executor's,
    /// below every part whose needs are listed, then each adapter's, up.
    fn next_offer_position(&self) -> usize {
        self.needs.len() - self.offers.len()
    }

    fn note_offer(&mut self, name: &str, offer: &Offer, own: Offer) {
        let part = PartId::new(self.next_offer_position(), name);
        self.offers.push((part, offer.clone()));
        self.own_offers.push(own);
    }
}
