// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Driver `>>` adapter: a [`Composed`] in the driver's role.

use super::{ChainBelow, Composed, kept, limits_below};
use crate::admission::by_part::ByPart;
use crate::checked::Checked;
use crate::panics::end_both;
use crate::roles::EndedRun;
use crate::{
    Adapt, Capabilities, Drive, DriveError, Execute, InstructionSet, Needs, OpcodeMap, RunRefusal,
    Seed, ShotSet,
};

/// Driver `>>` adapter: a driver.
impl<U: Drive, L: Adapt> Drive for Composed<U, L> {
    type Output = U::Output;

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
        self.needs_by_part(&mut ByPart::ignored())
    }

    /// Top down: the driver, then the adapter with the driver's needs, as
    /// the admission checked them, read before the driver is provisioned.
    fn provision(&mut self) {
        let below = self.upper.needs();
        self.batch_limits = Some(limits_below(
            self.below_offer.as_ref(),
            self.lower.accepts(),
            &below,
        ));
        self.upper.provision();
        self.lower.provision(below);
    }

    fn needs_by_part(&self, by_part: &mut ByPart) -> Needs {
        let above = self.upper.needs_by_part(by_part);
        self.lower.needs_by_part(above, by_part)
    }

    fn offers_by_part(&mut self, below: Capabilities, by_part: &mut ByPart) -> Capabilities {
        let above = self.lower.offers_by_part(below, by_part);
        self.below_part = by_part.last_offered();
        self.below_offer = Some(above.clone());
        self.upper.offers_by_part(above, by_part)
    }

    /// The driver's adapters, then this one, each ended even if one
    /// before it panics.
    fn end_run(&mut self, run: EndedRun<'_>) {
        end_both(
            || self.upper.end_run(run),
            || self.lower.end_shots(run.shots),
        );
    }

    /// The part below first.
    async fn ready(&mut self) -> Result<(), RunRefusal> {
        self.lower.ready().await?;
        self.upper.ready().await
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
        self.upper
            .drive(
                shots,
                seed,
                &mut Checked::new(&mut rest, part, offer, batch_limits),
            )
            .await
    }
}
