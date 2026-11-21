// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A driver through an adapter: a [`Composed`] in the driver's role.

use std::panic::resume_unwind;

use super::{ChainBelow, Composed, DECLARED};
use crate::composition::DeclaredParts;
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::part_panics::{PanicEndingShots, await_part, catch_panic};
use crate::composition::unended_shots::UnendedShots;
use crate::part_id::ChainPosition;
use crate::{
    Adapter, BoundaryCompatible, Capabilities, DriveError, Driver, Executor, Failure, NotReady,
    OpcodeMap, RunProvision, Seed, ShotSet,
};

impl<U: Driver, L: Adapter> Composed<U, L> {
    /// `upper` through `lower`: binds `upper` where its requirement fits
    /// what `lower` provides.
    pub(crate) fn driver_through(mut upper: U, lower: L) -> Self {
        let fit = upper.requires().fits(lower.provides());
        if let Ok(boundary) = &fit {
            upper.bind(boundary.opcode_map().clone());
        }
        let requires = (upper.requires()).past(lower.requires(), lower.provides());
        let run_provides = upper.provides().over(lower.provides());
        let mut parts = DeclaredParts::new();
        upper.declare(&mut parts);
        let upper_parts = parts.len();
        let mut joined = Self::new((upper, upper_parts), lower, fit, Some(requires), None);
        joined.run_provides = Some(run_provides);
        joined
    }
}

/// A driver through an adapter: a driver.
impl<U: Driver, L: Adapter> Driver for Composed<U, L> {
    type Output = U::Output;

    /// The driver's.
    fn name(&self) -> &str {
        self.upper.name()
    }

    /// The adapter's requirement, with what of the driver's reaches past
    /// the adapter.
    fn requires(&self) -> &Capabilities {
        self.requires.as_ref().expect(DECLARED)
    }

    /// The driver's, over the adapter's provision.
    fn provides(&self) -> &RunProvision {
        self.run_provides.as_ref().expect(DECLARED)
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.lower.bind(below);
    }

    /// As [`ready_at`](Self::ready_at), at a position not known.
    fn ready(&mut self) -> impl Future<Output = Result<(), NotReady>> {
        self.ready_at(ChainPosition::UNKNOWN)
    }

    /// The driver's `drive`, on the adapter and the chain below as one
    /// executor, if the run's shots, and its seed if fixed, fit what it
    /// provides a run over the chain below, by the rule the runner checks a
    /// run with; else the run's first shot fails
    /// with [`Failure::NotProvided`], and the driver runs nothing. The
    /// runner's check comes first and gives the same verdict, unless a part
    /// of the user's holds this driver. Then ends the run's shots on the
    /// adapter, whatever that `drive` gave, so that a joined driver ends the
    /// shots its adapters hold, as the runner ends the executor's, whoever
    /// calls it. A panic ending them is raised again from the run, through any
    /// joined driver above, as one in the runner's end is.
    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> Result<Vec<U::Output>, DriveError> {
        let provision = Driver::provides(self).over(below.provides());
        let (upper, lower, boundary) =
            self.run_boundary(|lower| lower.provides().over(below.provides()));
        let mut unended = UnendedShots::new(
            ChainBelow {
                adapter: lower,
                below,
            },
            shots,
            |rest, shots| rest.adapter.end_shots(shots),
        );
        let drove = match provision.admits_run(shots.len(), seed.is_fixed()) {
            Ok(()) => {
                let mut rest = BoundaryCrossing::new(&mut unended.part, boundary);
                catch_panic(|| upper.drive(shots, seed, &mut rest)).await
            }
            Err(mismatch) => Ok(Err(DriveError {
                shot: shots.first(),
                failure: Failure::NotProvided(mismatch),
            })),
        };
        match (drove, unended.end()) {
            (Err(payload), _) if payload.is::<PanicEndingShots>() => resume_unwind(payload),
            (_, Err(payload)) => resume_unwind(Box::new(PanicEndingShots(payload))),
            (Err(payload), Ok(())) => resume_unwind(payload),
            (Ok(drove), Ok(())) => drove,
        }
    }

    /// The driver, then the adapter.
    fn declare<'a>(&'a self, parts: &mut DeclaredParts<'a>) {
        self.upper.declare(parts);
        self.lower.declare(parts);
    }

    /// Refuses the run if the halves do not fit; else the adapter first,
    /// for the driver's requirement, then the driver. A refusal from the
    /// adapter names it, at its position, if `at` is known.
    async fn ready_at(&mut self, at: ChainPosition<'_>) -> Result<(), NotReady> {
        let (lower, required, below, part) =
            self.start_run(|upper| upper.requires().clone(), at)?;
        await_part(lower.ready_at(required, below))
            .await
            .map_err(|not_ready| not_ready.answered_by(part))?;
        await_part(self.upper.ready_at(at)).await
    }
}
