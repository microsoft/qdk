// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Adapter `>>` adapter: a [`Composed`] in the adapter's role.

use super::{ChainBelow, Composed, kept, limits_below};
use crate::admission::by_part::ByPart;
use crate::checked::Checked;
use crate::panics::{await_part, end_both};
use crate::{
    Adapt, AppendOnlyReadouts, Batch, Capabilities, Execute, ExecuteError, InstructionSet, Needs,
    OpcodeMap, RunRefusal, Seed, ShotSet,
};

/// Adapter `>>` adapter: an adapter.
impl<U: Adapt, L: Adapt> Adapt for Composed<U, L> {
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
        self.needs_by_part(above, &mut ByPart::ignored())
    }

    fn offers_above(&self, below: Capabilities) -> Capabilities {
        self.upper.offers_above(self.lower.offers_above(below))
    }

    /// Top down: the upper part with `above`, then the lower one with what
    /// the upper part needs below.
    fn provision(&mut self, above: Needs) {
        let below = self.upper.needs_below(above.clone());
        self.batch_limits = Some(limits_below(
            self.below_offer.as_ref(),
            self.lower.accepts(),
            &below,
        ));
        self.upper.provision(above);
        self.lower.provision(below);
    }

    fn needs_by_part(&self, above: Needs, by_part: &mut ByPart) -> Needs {
        let middle = self.upper.needs_by_part(above, by_part);
        self.lower.needs_by_part(middle, by_part)
    }

    fn offers_by_part(&mut self, below: Capabilities, by_part: &mut ByPart) -> Capabilities {
        let middle = self.lower.offers_by_part(below, by_part);
        self.below_part = by_part.last_offered();
        self.below_offer = Some(middle.clone());
        self.upper.offers_by_part(middle, by_part)
    }

    /// The part below first.
    async fn ready(&mut self) -> Result<(), RunRefusal> {
        await_part(self.lower.ready()).await?;
        await_part(self.upper.ready()).await
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut AppendOnlyReadouts<'_>,
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
            &mut Checked::new(&mut rest, part, offer, batch_limits),
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
