// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A driver through an adapter: a [`Composed`] in the driver's role.

use std::panic::resume_unwind;

use super::{ChainBelow, Composed, DECLARED};
use crate::capabilities::Forwarding;
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::part_panics::{PanicEndingShots, await_part, catch_panic};
use crate::composition::unended_shots::UnendedShots;
use crate::composition::{ChainPosition, DeclaredParts};
use crate::{
    Adapter, BoundaryCompatible, Capabilities, DriveError, Driver, Executor, NotReady, OpcodeMap,
    Seed, ShotSet,
};

impl<U: Driver, L: Adapter> Composed<U, L> {
    /// `upper` through `lower`: binds `upper` where its requirement fits
    /// what `lower` provides.
    pub(crate) fn driver_through(mut upper: U, lower: L) -> Self {
        let fit = upper.requires().fits(lower.provides());
        if let Ok(boundary) = &fit {
            upper.bind(boundary.opcode_map().clone());
        }
        let requires = lower.requires().or(upper.requires());
        let mut parts = DeclaredParts::new();
        upper.declare(&mut parts);
        let upper_parts = parts.len();
        Self::new((upper, upper_parts), lower, fit, Some(requires), None)
    }
}

/// A driver through an adapter: a driver.
impl<U: Driver, L: Adapter> Driver for Composed<U, L> {
    type Output = U::Output;

    /// The driver's.
    fn name(&self) -> &str {
        self.upper.name()
    }

    /// The adapter's requirement, and the driver's where the adapter does
    /// not state it.
    fn requires(&self) -> &Capabilities {
        self.requires.as_ref().expect(DECLARED)
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.lower.bind(below);
    }

    /// As [`ready_at`](Self::ready_at), at a position not known.
    fn ready(&mut self) -> impl Future<Output = Result<(), NotReady>> {
        self.ready_at(ChainPosition::UNKNOWN)
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
        let (upper, lower, boundary) = self.run_boundary(|lower| {
            lower
                .provides()
                .stricter(&below.provides().unanswered(lower.requires()))
        });
        let mut unended = UnendedShots::new(
            ChainBelow {
                adapter: lower,
                below,
            },
            shots,
            |rest, shots| rest.adapter.end_shots(shots),
        );
        let mut rest = BoundaryCrossing::new(&mut unended.part, boundary);
        let drove = catch_panic(|| upper.drive(shots, seed, &mut rest)).await;
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
    async fn ready_at(&mut self, at: ChainPosition) -> Result<(), NotReady> {
        let (lower, required, below, part) =
            self.start_run(|upper| upper.requires().clone(), |lower| lower.name(), at)?;
        await_part(lower.ready_at(required, below))
            .await
            .map_err(|not_ready| not_ready.answered_by(part))?;
        await_part(self.upper.ready_at(at)).await
    }
}
