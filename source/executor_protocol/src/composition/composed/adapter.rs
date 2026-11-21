// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An adapter through an adapter: a [`Composed`] in the adapter's role.

use super::{ChainBelow, Composed, KEPT};
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::chain_check::ChainCheck;
use crate::composition::part_panics::{await_part, end_both};
use crate::{
    Adapter, Batch, BatchReadouts, ExecuteError, Executor, InstructionSet, Needs, NotReady, Offer,
    OpcodeMap, Seed, ShotSet,
};

/// An adapter through an adapter: an adapter.
impl<U: Adapter, L: Adapter> Adapter for Composed<U, L> {
    /// The upper part's, which receives its calls.
    fn name(&self) -> &str {
        self.upper.name()
    }

    fn accepts(&self) -> &InstructionSet {
        self.upper.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.lower.sends()
    }

    fn needs_below(&self, above: Needs) -> Needs {
        self.lower.needs_below(self.upper.needs_below(above))
    }

    fn offers_above(&self, below: Offer) -> Offer {
        self.upper.offers_above(self.lower.offers_above(below))
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.lower.bind(below);
    }

    /// The part below first, with the needs kept for it, then the upper
    /// part with `above`; a refusal from below names the part that
    /// answered it.
    async fn ready(&mut self, above: &Needs) -> Result<(), NotReady> {
        let crossing = self.crossing.as_ref().expect(KEPT);
        await_part(self.lower.ready(&crossing.needs))
            .await
            .map_err(|not_ready| not_ready.answered_by(&crossing.part))?;
        await_part(self.upper.ready(above)).await
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let crossing = self.crossing.as_ref().expect(KEPT);
        let mut rest = ChainBelow {
            adapter: &mut self.lower,
            below,
        };
        await_part(self.upper.adapt(
            batches,
            seed,
            &mut BoundaryCrossing::new(&mut rest, crossing),
            readouts,
        ))
        .await
    }

    /// The upper part, then the lower one, ended even if the upper part
    /// panics.
    fn end_shots(&mut self, shots: &ShotSet) {
        end_both(
            || self.upper.end_shots(shots),
            || self.lower.end_shots(shots),
        );
    }

    fn check_chain(
        &mut self,
        above: Needs,
        below: &mut dyn FnMut(Needs, &mut ChainCheck) -> Offer,
        chain: &mut ChainCheck,
    ) -> Offer {
        let Self {
            upper,
            lower,
            fit_faults,
            crossing,
        } = self;
        upper.check_chain(
            above,
            &mut |middle, chain| {
                let receiver = lower.accepts().clone();
                let crossed = chain.cross(fit_faults.take(), middle, receiver, |needs, chain| {
                    lower.check_chain(needs, below, chain)
                });
                let offer = crossed.offer.clone();
                *crossing = Some(crossed);
                offer
            },
            chain,
        )
    }
}
