// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Adapter `>>` adapter: a [`Composed`] in the adapter's role.

use super::{ChainBelow, Composed, KEPT, kept, limits_below};
use crate::composition::admission::by_part::ByPart;
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::part_panics::{await_part, end_both};
use crate::{
    Adapt, AdapterSetup, Batch, BatchReadouts, Execute, ExecuteError, FinalRow, Instruction,
    InstructionSet, InstructionStream, Needs, NotReady, Offer, OpcodeMap, Seed, ShotSet,
};

/// Adapter `>>` adapter: an adapter, at composition time.
impl<U: AdapterSetup, L: AdapterSetup> AdapterSetup for Composed<U, L> {
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

    fn bind(&mut self, below: OpcodeMap) {
        self.lower.bind(below);
    }

    fn needs_below(&self, above: Needs) -> Needs {
        self.lower.needs_below(self.upper.needs_below(above))
    }

    fn offers_above(&self, below: Offer) -> Offer {
        self.upper.offers_above(self.lower.offers_above(below))
    }

    fn needs_by_part(&mut self, above: Needs, by_part: &mut ByPart) -> Needs {
        let middle = self.upper.needs_by_part(above, by_part);
        self.below_needs = Some(middle.clone());
        self.lower.needs_by_part(middle, by_part)
    }

    fn offers_by_part(&mut self, below: Offer, by_part: &mut ByPart) -> Offer {
        let middle = self.lower.offers_by_part(below, by_part);
        self.below_part = by_part.last_offered();
        self.batch_limits = Some(limits_below(
            &middle,
            self.lower.accepts(),
            self.below_needs.as_ref(),
        ));
        self.below_offer = Some(middle.clone());
        self.upper.offers_by_part(middle, by_part)
    }
}

/// Adapter `>>` adapter: an adapter, at run time.
// Every method stated, for the reason `composed`'s module doc gives.
#[deny(clippy::missing_trait_methods)]
impl<U: Adapt, L: Adapt> Adapt for Composed<U, L> {
    /// The part below first, with the needs kept for it, then the upper
    /// part with `above`; a refusal from below names the part that
    /// answered it.
    async fn ready(&mut self, above: &Needs) -> Result<(), NotReady> {
        let below = self.below_needs.as_ref().expect(KEPT);
        let below_part = self.below_part.as_ref().expect(KEPT);
        await_part(self.lower.ready(below))
            .await
            .map_err(|not_ready| not_ready.answered_by(below_part))?;
        await_part(self.upper.ready(above)).await
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let (part, offer, batch_limits) = kept(
            self.below_part.as_ref(),
            self.below_offer.as_ref(),
            self.batch_limits.as_ref(),
        );
        let mut rest = ChainBelow {
            adapter: &mut self.lower,
            below,
        };
        await_part(self.upper.adapt(
            batches,
            seed,
            &mut BoundaryCrossing::new(&mut rest, part, offer, batch_limits),
            readouts,
        ))
        .await
    }

    /// The upper adapter's rows, as it gives them over the lower one and
    /// the chain below, through the boundary between them.
    async fn adapt_each(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
        each: &mut impl FnMut(FinalRow<'_>),
    ) -> Result<(), ExecuteError> {
        let (part, offer, batch_limits) = kept(
            self.below_part.as_ref(),
            self.below_offer.as_ref(),
            self.batch_limits.as_ref(),
        );
        let mut rest = ChainBelow {
            adapter: &mut self.lower,
            below,
        };
        await_part(self.upper.adapt_each(
            batches,
            seed,
            &mut BoundaryCrossing::new(&mut rest, part, offer, batch_limits),
            readouts,
            each,
        ))
        .await
    }

    /// The upper adapter's answer to the stream, over the lower one and
    /// the chain below, through the boundary between them.
    async fn adapt_stream(
        &mut self,
        stream: InstructionStream<'_, impl Iterator<Item = Instruction>>,
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut BatchReadouts,
    ) -> Result<(), ExecuteError> {
        let (part, offer, batch_limits) = kept(
            self.below_part.as_ref(),
            self.below_offer.as_ref(),
            self.batch_limits.as_ref(),
        );
        let mut rest = ChainBelow {
            adapter: &mut self.lower,
            below,
        };
        await_part(self.upper.adapt_stream(
            stream,
            seed,
            &mut BoundaryCrossing::new(&mut rest, part, offer, batch_limits),
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
}
