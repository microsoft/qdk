// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Adapter `>>` executor: a [`Composed`] in the executor's role.

use super::{Composed, kept, limits_below};
use crate::admission::by_part::ByPart;
use crate::checked::Checked;
use crate::panics::{await_part, end_both};
use crate::{
    Adapt, Batch, Capabilities, Execute, ExecuteError, InstructionSet, Needs, Readouts, RunRefusal,
    Seed, ShotSet,
};

/// Adapter `>>` executor: an executor.
impl<U: Adapt, L: Execute> Execute for Composed<U, L> {
    /// The adapter's, which receives its calls.
    fn name(&self) -> &str {
        self.upper.name()
    }

    fn accepts(&self) -> &InstructionSet {
        self.upper.accepts()
    }

    fn offers(&self) -> Capabilities {
        self.upper.offers_above(self.lower.offers())
    }

    /// Top down, as for an adapter.
    fn provision(&mut self, needs: Needs) {
        let below = self.upper.needs_below(needs.clone());
        self.batch_limits = Some(limits_below(
            self.below_offer.as_ref(),
            self.lower.accepts(),
            &below,
        ));
        self.upper.provision(needs);
        self.lower.provision(below);
    }

    fn needs_by_part(&self, above: Needs, by_part: &mut ByPart) -> Needs {
        let below = self.upper.needs_by_part(above, by_part);
        self.lower.needs_by_part(below, by_part)
    }

    fn offers_by_part(&mut self, by_part: &mut ByPart) -> Capabilities {
        let below = self.lower.offers_by_part(by_part);
        self.below_part = by_part.last_offered();
        self.below_offer = Some(below.clone());
        self.upper.offers_by_part(below, by_part)
    }

    /// The part below first.
    async fn ready(&mut self) -> Result<(), RunRefusal> {
        await_part(self.lower.ready()).await?;
        await_part(self.upper.ready()).await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut Readouts,
    ) -> Result<(), ExecuteError> {
        let (part, offer, batch_limits) = kept(
            self.below_part.as_ref(),
            self.below_offer.as_ref(),
            self.batch_limits.as_ref(),
        );
        await_part(self.upper.adapt(
            batches,
            seed,
            &mut Checked::new(&mut self.lower, part, offer, batch_limits),
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
