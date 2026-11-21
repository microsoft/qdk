// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A driver through an adapter: a [`Composed`] in the driver's role.

use std::panic::resume_unwind;

use super::{ChainBelow, Composed};
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::chain_check::ChainCheck;
use crate::composition::part_panics::{PanicEndingShots, await_part, catch_panic};
use crate::composition::unended_shots::UnendedShots;
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

    /// Refuses the run if the halves do not fit; else the part below
    /// first, for the driver's needs, as the chain's check found them, or
    /// as the driver states them now; a refusal from below names the part
    /// that answered it, at its position, if the check found it.
    async fn ready(&mut self) -> Result<(), NotReady> {
        let (lower, needs, part) = self.start_run(U::needs)?;
        await_part(lower.ready(needs))
            .await
            .map_err(|not_ready| not_ready.answered_by(part))?;
        await_part(self.upper.ready()).await
    }

    /// The driver's `drive`, on the adapter and the chain below as one
    /// executor; then ends the run's shots on the adapter, whatever that
    /// `drive` gave, so that a joined driver ends the shots its adapters
    /// hold, as the runner ends the executor's, whoever calls it. A panic
    /// ending them is raised again from the run, through any joined driver
    /// above, as one in the runner's end is.
    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> Result<Vec<U::Output>, DriveError> {
        let (upper, lower, crossing) = self
            .run_crossing(|lower| (lower.offers_above(below.offers()), lower.accepts().clone()));
        let mut unended = UnendedShots::new(
            ChainBelow {
                adapter: lower,
                below,
            },
            shots,
            |rest, shots| rest.adapter.end_shots(shots),
        );
        let mut rest = BoundaryCrossing::new(&mut unended.part, crossing);
        let drove = catch_panic(|| upper.drive(shots, seed, &mut rest)).await;
        match (drove, unended.end()) {
            (Err(payload), _) if payload.is::<PanicEndingShots>() => resume_unwind(payload),
            (_, Err(payload)) => resume_unwind(Box::new(PanicEndingShots(payload))),
            (Err(payload), Ok(())) => resume_unwind(payload),
            (Ok(drove), Ok(())) => drove,
        }
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
            checked,
            ..
        } = self;
        upper.check_chain(
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
