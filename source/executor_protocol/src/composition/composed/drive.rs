// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Driver `>>` adapter: a [`Composed`] in the driver's role.

use super::{ChainBelow, Composed, KEPT, limits_below};
use crate::composition::admission::by_part::ByPart;
use crate::composition::part_panics::end_both;
use crate::roles::EndedRun;
use crate::roles::await_part;
use crate::{
    Adapt, AdapterSetup, Drive, DriveError, DriverSetup, Execute, InstructionSet, Needs, NotReady,
    Offer, OpcodeMap, Seed, ShotId, ShotSet,
};

/// Driver `>>` adapter: a driver, at composition time.
impl<U: DriverSetup, L: AdapterSetup> DriverSetup for Composed<U, L> {
    /// The driver's.
    fn name(&self) -> &str {
        self.upper.name()
    }

    fn sends(&self) -> &InstructionSet {
        self.lower.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.lower.bind(below);
    }

    fn needs(&self) -> Needs {
        self.lower.needs_below(self.upper.needs())
    }

    fn needs_by_part(&mut self, by_part: &mut ByPart) -> Needs {
        let above = self.upper.needs_by_part(by_part);
        self.below_needs = Some(above.clone());
        self.lower.needs_by_part(above, by_part)
    }

    fn offers_by_part(&mut self, below: Offer, by_part: &mut ByPart) -> Offer {
        let above = self.lower.offers_by_part(below, by_part);
        self.below_part = by_part.last_offered();
        self.batch_limits = Some(limits_below(
            &above,
            self.lower.accepts(),
            self.below_needs.as_ref(),
        ));
        self.below_offer = Some(above.clone());
        self.upper.offers_by_part(above, by_part)
    }
}

/// Driver `>>` adapter: a driver, at run time.
// Every method stated, for the reason `composed`'s module doc gives.
#[deny(clippy::missing_trait_methods)]
impl<U: Drive, L: Adapt> Drive for Composed<U, L> {
    type Output = U::Output;

    /// The part below first, with the needs kept for it: the driver's, as
    /// the admission checked them; a refusal from below names the part
    /// that answered it.
    async fn ready(&mut self) -> Result<(), NotReady> {
        let below = self.below_needs.as_ref().expect(KEPT);
        let below_part = self.below_part.as_ref().expect(KEPT);
        await_part(self.lower.ready(below))
            .await
            .map_err(|not_ready| not_ready.answered_by(below_part))?;
        await_part(self.upper.ready()).await
    }

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<U::Output>, DriveError> {
        let (upper, lower, boundary) = self.split_at_boundary();
        let mut rest = ChainBelow {
            adapter: lower,
            below,
        };
        await_part(upper.drive(shots, seed, &mut boundary.crossing_to(&mut rest))).await
    }

    /// The driver's outputs as it gives them, over the adapter, through
    /// the boundary between them.
    async fn drive_each(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
        each: &mut impl FnMut(ShotId, U::Output),
    ) -> Result<(), DriveError> {
        let (upper, lower, boundary) = self.split_at_boundary();
        let mut rest = ChainBelow {
            adapter: lower,
            below,
        };
        await_part(upper.drive_each(shots, seed, &mut boundary.crossing_to(&mut rest), each)).await
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
