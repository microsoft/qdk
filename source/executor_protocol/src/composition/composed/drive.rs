// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Driver `>>` adapter: a [`Composed`] in the driver's role.

use super::{ChainBelow, Composed, KEPT, kept, limits_below};
use crate::composition::admission::by_part::ByPart;
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::part_panics::{await_part, end_both};
use crate::roles::EndedRun;
use crate::{
    Adapt, AdapterSetup, Drive, DriveError, DriverSetup, Execute, InstructionSet, Needs, NotReady,
    Offer, OpcodeMap, Seed, ShotSet,
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
        let (part, offer, batch_limits) = kept(
            self.below_part.as_ref(),
            self.below_offer.as_ref(),
            self.batch_limits.as_ref(),
        );
        let mut rest = ChainBelow {
            adapter: &mut self.lower,
            below,
        };
        await_part(self.upper.drive(
            shots,
            seed,
            &mut BoundaryCrossing::new(&mut rest, part, offer, batch_limits),
        ))
        .await
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
