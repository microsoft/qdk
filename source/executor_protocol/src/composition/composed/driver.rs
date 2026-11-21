// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A driver through an adapter: a [`Composed`] in the driver's role.

use super::{ChainBelow, Composed, KEPT};
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::chain_check::ChainCheck;
use crate::composition::part_panics::{await_part, end_both};
use crate::roles::EndedRun;
use crate::{
    Adapter, DriveError, Driver, Executor, InstructionSet, Needs, NotReady, Offer, OpcodeMap, Seed,
    ShotSet,
};

/// A driver through an adapter: a driver.
impl<U: Driver, L: Adapter> Driver for Composed<U, L> {
    type Output = U::Output;

    /// The driver's.
    fn name(&self) -> &str {
        self.upper.name()
    }

    fn sends(&self) -> &InstructionSet {
        self.lower.sends()
    }

    fn needs(&self) -> Needs {
        self.lower.needs_below(self.upper.needs())
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.lower.bind(below);
    }

    /// The part below first, with the needs kept for it: the driver's, as
    /// the admission checked them; a refusal from below names the part
    /// that answered it.
    async fn ready(&mut self) -> Result<(), NotReady> {
        let crossing = self.crossing.as_ref().expect(KEPT);
        await_part(self.lower.ready(&crossing.needs))
            .await
            .map_err(|not_ready| not_ready.answered_by(&crossing.part))?;
        await_part(self.upper.ready()).await
    }

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> Result<Vec<U::Output>, DriveError> {
        let crossing = self.crossing.as_ref().expect(KEPT);
        let mut rest = ChainBelow {
            adapter: &mut self.lower,
            below,
        };
        await_part(
            self.upper
                .drive(shots, seed, &mut BoundaryCrossing::new(&mut rest, crossing)),
        )
        .await
    }

    fn check_chain(
        &mut self,
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
            &mut |middle, chain| {
                let receiver = lower.accepts().clone();
                chain.cross(
                    crossing,
                    fit_faults.as_ref(),
                    middle,
                    receiver,
                    |needs, chain| lower.check_chain(needs, below, chain),
                )
            },
            chain,
        )
    }

    /// The driver's adapters, then this one, each ended even if one
    /// before it panics.
    fn end_run(&mut self, run: EndedRun<'_>) {
        end_both(
            || self.upper.end_run(run),
            || self.lower.end_shots(run.shots),
        );
    }
}
