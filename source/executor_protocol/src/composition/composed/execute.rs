// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Adapter `>>` executor: a [`Composed`] in the executor's role.

use super::{Composed, KEPT, limits_below};
use crate::composition::admission::by_part::ByPart;
use crate::part_panics::{await_part, end_both};
use crate::{
    Adapt, AdapterSetup, Batch, BatchReadouts, Execute, ExecuteError, ExecutorSetup, FinalRow,
    Instruction, InstructionSet, InstructionStream, Needs, NotReady, Offer, Seed, ShotSet,
};

/// Adapter `>>` executor: an executor, at composition time.
impl<U: AdapterSetup, L: ExecutorSetup> ExecutorSetup for Composed<U, L> {
    /// The adapter's, which receives its calls.
    fn name(&self) -> &str {
        self.upper.name()
    }

    fn accepts(&self) -> &InstructionSet {
        self.upper.accepts()
    }

    fn offers(&self) -> Offer {
        self.upper.offers_above(self.lower.offers())
    }

    fn needs_by_part(&mut self, above: Needs, by_part: &mut ByPart) -> Needs {
        let below = self.upper.needs_by_part(above, by_part);
        self.below_needs = Some(below.clone());
        self.lower.needs_by_part(below, by_part)
    }

    fn offers_by_part(&mut self, by_part: &mut ByPart) -> Offer {
        let below = self.lower.offers_by_part(by_part);
        self.below_part = by_part.last_offered();
        self.batch_limits = Some(limits_below(
            &below,
            self.lower.accepts(),
            self.below_needs.as_ref(),
        ));
        self.below_offer = Some(below.clone());
        self.upper.offers_by_part(below, by_part)
    }
}

/// Adapter `>>` executor: an executor, at run time.
// Every method stated, for the reason `composed`'s module doc gives.
#[deny(clippy::missing_trait_methods)]
impl<U: Adapt, L: Execute> Execute for Composed<U, L> {
    /// The part below first, with the needs kept for it, then the adapter
    /// with `needs`, as for an adapter; a refusal from below names the
    /// part that answered it.
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        let below = self.below_needs.as_ref().expect(KEPT);
        let below_part = self.below_part.as_ref().expect(KEPT);
        await_part(self.lower.ready(below))
            .await
            .map_err(|not_ready| not_ready.answered_by(below_part))?;
        await_part(self.upper.ready(needs)).await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let (upper, lower, boundary) = self.split_at_boundary();
        await_part(upper.adapt(batches, seed, &mut boundary.crossing_to(lower), readouts)).await
    }

    /// The adapter's rows, as it gives them over the executor, through
    /// the boundary between them.
    async fn execute_each(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
        each: &mut impl FnMut(FinalRow<'_>),
    ) -> Result<(), ExecuteError> {
        let (upper, lower, boundary) = self.split_at_boundary();
        await_part(upper.adapt_each(
            batches,
            seed,
            &mut boundary.crossing_to(lower),
            readouts,
            each,
        ))
        .await
    }

    /// The adapter's answer to the stream, over the executor, through the
    /// boundary between them.
    async fn execute_stream(
        &mut self,
        stream: InstructionStream<'_, impl Iterator<Item = Instruction>>,
        seed: Seed,
        readouts: &mut BatchReadouts,
    ) -> Result<(), ExecuteError> {
        let (upper, lower, boundary) = self.split_at_boundary();
        await_part(upper.adapt_stream(stream, seed, &mut boundary.crossing_to(lower), readouts))
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
