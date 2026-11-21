// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The check of a whole chain, made by the [`on`](crate::DriverJoins::on)
//! that makes its runner, before any shot runs: [`ChainCheck`], to which
//! each part adds its facts, and which concludes with the run's limits or
//! every fault of the chain ([`CompositionFaults`]): the fit faults of each
//! boundary, and the admission's ([`AdmissionFaults`]). The `on` then
//! builds the runner with what crosses the boundary between the driver and
//! the executor, as the check found it.
//!
//! The roles' hidden `check_chain` methods take one, and code outside the
//! crate cannot name it, because no public path reaches it: so those
//! methods cannot be called or overridden outside the crate, and every part
//! adds its facts as the crate adds them. The check crosses every boundary
//! of the chain, top to bottom, adding the boundary's fit faults, which the
//! join that made it kept; each part adds its needs on the way down and
//! its offer on the way up, so that the admission compares, at each
//! boundary, the needs of the part above with the offer of the part below.

mod admission;
mod composition_faults;

use crate::composition::FitFaults;
use crate::composition::boundary_crossing::Crossing;
use crate::composition::runner::RunLimits;
use crate::{Driver, Executor, InstructionSet, Needs, Offer, PartId};

pub use admission::{AdmissionFaults, ShortOffer};
pub use composition_faults::CompositionFaults;

/// The facts of a chain's parts, as each adds them: each part's needs, top
/// to bottom, and offer, bottom to top, each with the part's position and
/// name; and the fit faults of each boundary that has some, top to bottom.
pub struct ChainCheck {
    /// The needs added, in the order they were.
    needs: Vec<(PartId, Needs)>,
    /// The offers added, in the order they were.
    offers: Vec<(PartId, Offer)>,
    /// For each offer added, the part's offer on its own: given an offer
    /// below that meets exactly what it needs below, for an adapter; its
    /// offer, for the executor.
    own_offers: Vec<Offer>,
    /// The fit faults of each boundary that has some, top to bottom.
    fit_faults: Vec<FitFaults>,
}

impl ChainCheck {
    /// The check of the chain of `driver` on `executor`, whose boundary's
    /// join found `fit_faults`, if it does not fit, once every part has
    /// added its facts; and what crosses that boundary.
    pub(crate) fn of(
        driver: &mut impl Driver,
        mut fit_faults: Option<FitFaults>,
        executor: &mut impl Executor,
    ) -> (Self, Crossing) {
        let mut chain = Self::new();
        let mut crossing = None;
        driver.check_chain(
            &mut |needs, chain| {
                let receiver = executor.accepts().clone();
                let crossed = chain.cross(fit_faults.take(), needs, receiver, |needs, chain| {
                    executor.check_chain(needs, chain)
                });
                let offer = crossed.offer.clone();
                crossing = Some(crossed);
                offer
            },
            &mut chain,
        );
        let crossing = crossing.expect("the driver's `check_chain` crosses to the executor");
        (chain, crossing)
    }

    /// A check to which no part has added its facts yet.
    fn new() -> Self {
        Self {
            needs: Vec::new(),
            offers: Vec::new(),
            own_offers: Vec::new(),
            fit_faults: Vec::new(),
        }
    }

    /// The run's limits, the most shots and the seedability every part's
    /// offer meets, once every part has added its facts; or, if a boundary
    /// does not fit or an offer falls short at one, every fault of the
    /// chain: the fit faults of each boundary, top to bottom, and the
    /// admission's.
    pub(crate) fn conclude(self) -> Result<RunLimits, CompositionFaults> {
        let run_limits = RunLimits::of(self.offers.iter().map(|(_, offer)| offer));
        let admission = AdmissionFaults::of(self.needs, self.offers, &self.own_offers);
        if self.fit_faults.is_empty() && admission.is_none() {
            Ok(run_limits)
        } else {
            Err(CompositionFaults::new(self.fit_faults, admission))
        }
    }

    /// Crosses the next boundary down, to a receiver of `receiver`'s
    /// instructions, whose join found `fit_faults`, if it does not fit:
    /// adds them, then has the chain below it add its facts for `needs`,
    /// with `check_below`, which gives the offer that crosses the boundary
    /// up. The check crosses the boundaries top to bottom, so the fit
    /// faults are added in that order.
    pub(crate) fn cross(
        &mut self,
        fit_faults: Option<FitFaults>,
        needs: Needs,
        receiver: InstructionSet,
        check_below: impl FnOnce(Needs, &mut Self) -> Offer,
    ) -> Crossing {
        self.fit_faults.extend(fit_faults);
        let offer = check_below(needs.clone(), self);
        let (part, _) = self
            .offers
            .last()
            .expect("the part below the boundary added its offer");
        Crossing::new(part.clone(), needs, offer, receiver)
    }

    /// Adds `needs` as those of the part `name`, the next one down: the
    /// driver first, then each adapter.
    pub(crate) fn add_needs(&mut self, name: &str, needs: &Needs) {
        let position = self.needs.len();
        self.needs
            .push((PartId::new(position, name), needs.clone()));
    }

    /// Adds `offer` as that of the executor `name`, once every part has
    /// added its needs.
    pub(crate) fn add_executor_offer(&mut self, name: &str, offer: &Offer) {
        self.add_offer(name, offer, offer.clone());
    }

    /// Adds `above` as the offer of the adapter `name`, the next one up,
    /// given `below`, once every part has added its needs; and as its own
    /// offer, what `offers_above` gives for an offer below that meets
    /// exactly what the adapter needs below, as
    /// [`ShortOffer::offered`] says.
    pub(crate) fn add_adapter_offer(
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
        self.add_offer(name, above, offers_above(exactly));
    }

    /// The part that added its needs first: the top part of the chain
    /// checked, the driver for a runner's chain; `None` if none did.
    pub(crate) fn first_needing(&self) -> Option<PartId> {
        self.needs.first().map(|(part, _)| part.clone())
    }

    /// The position of the part that adds its offer next: the executor's,
    /// below every part that added its needs, then each adapter's, up.
    fn next_offer_position(&self) -> usize {
        self.needs.len() - self.offers.len()
    }

    fn add_offer(&mut self, name: &str, offer: &Offer, own: Offer) {
        let part = PartId::new(self.next_offer_position(), name);
        self.offers.push((part, offer.clone()));
        self.own_offers.push(own);
    }
}
