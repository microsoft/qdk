// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An adapter through an adapter: a [`Composed`] in the adapter's role.

use super::{ChainBelow, Composed};
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

    /// Refuses the run if the halves do not fit; else the part below
    /// first, for the needs that cross to it, as the chain's check found
    /// them, or as the upper part adapts `above` now; then the upper part
    /// with `above`. A refusal from below names the part that answered
    /// it, at its position, if the check found it.
    async fn ready(&mut self, above: &Needs) -> Result<(), NotReady> {
        let (lower, needs, part) = self.start_run(|upper| upper.needs_below(above.clone()))?;
        await_part(lower.ready(needs))
            .await
            .map_err(|not_ready| not_ready.answered_by(part))?;
        await_part(self.upper.ready(above)).await
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let (upper, lower, crossing) = self
            .run_crossing(|lower| (lower.offers_above(below.offers()), lower.accepts().clone()));
        let mut rest = ChainBelow {
            adapter: lower,
            below,
        };
        await_part(upper.adapt(
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
            checked,
            ..
        } = self;
        upper.check_chain(
            above,
            &mut |middle, chain| {
                let receiver = lower.accepts().clone();
                chain.cross(
                    checked,
                    fit_faults.as_ref(),
                    middle,
                    receiver,
                    |needs, chain| lower.check_chain(needs, below, chain),
                )
            },
            chain,
        )
    }
}
