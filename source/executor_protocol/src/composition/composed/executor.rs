// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An adapter on an executor: a [`Composed`] in the executor's role.

use super::{Composed, KEPT};
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::chain_check::ChainCheck;
use crate::composition::part_panics::{await_part, end_both};
use crate::{
    Adapter, Batch, BatchReadouts, ExecuteError, Executor, InstructionSet, Needs, NotReady, Offer,
    Seed, ShotSet,
};

/// An adapter on an executor: an executor.
impl<U: Adapter, L: Executor> Executor for Composed<U, L> {
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

    /// The part below first, with the needs kept for it, then the adapter
    /// with `needs`, as for an adapter; a refusal from below names the
    /// part that answered it.
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        let crossing = self.crossing.as_ref().expect(KEPT);
        await_part(self.lower.ready(&crossing.needs))
            .await
            .map_err(|not_ready| not_ready.answered_by(&crossing.part))?;
        await_part(self.upper.ready(needs)).await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let crossing = self.crossing.as_ref().expect(KEPT);
        await_part(self.upper.adapt(
            batches,
            seed,
            &mut BoundaryCrossing::new(&mut self.lower, crossing),
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

    fn check_chain(&mut self, above: Needs, chain: &mut ChainCheck) -> Offer {
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
                chain.cross(
                    crossing,
                    fit_faults.as_ref(),
                    middle,
                    receiver,
                    |needs, chain| lower.check_chain(needs, chain),
                )
            },
            chain,
        )
    }
}
