// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A driver through an adapter: a [`Composed`] in the driver's role.

use std::panic::resume_unwind;

use super::{ChainBelow, Composed, boundary};
use crate::composition::DeclaredParts;
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::part_panics::{PanicEndingShots, await_part, catch_panic};
use crate::composition::unended_shots::UnendedShots;
use crate::part_id::ChainPosition;
use crate::{
    Adapter, Binding, Capabilities, DriveError, Driver, Executor, NotReady, RunRequirement, Seed,
    ShotSet,
};

impl<U: Driver, L: Adapter<Accepts = U::Sends>> Composed<U, L, U::Sends> {
    /// `upper` through `lower`: binds `upper` where its set fits the one
    /// `lower` accepts.
    pub(crate) fn driver_through(mut upper: U, lower: L) -> Self {
        let fit = boundary(upper.sends(), lower.accepts(), lower.provides()).map(
            |(boundary, binding)| {
                upper.bind(binding);
                boundary
            },
        );
        let mut parts = DeclaredParts::checking();
        upper.declare(&RunRequirement::unconstrained(), &mut parts);
        let upper_parts = parts.len();
        Self::new((upper, upper_parts), lower, fit)
    }
}

/// A driver through an adapter: a driver.
impl<U: Driver, L: Adapter<Accepts = U::Sends>> Driver for Composed<U, L, U::Sends> {
    type Output = U::Output;
    type Sends = L::Sends;

    /// The driver's.
    fn name(&self) -> &str {
        self.upper.name()
    }

    /// The adapter's.
    fn sends(&self) -> &L::Sends {
        self.lower.sends()
    }

    /// What the adapter requires, given what the driver requires.
    fn requires(&self, run: &RunRequirement) -> Capabilities<L::Sends> {
        self.lower.requires(&self.upper.requires(run))
    }

    fn bind(&mut self, below: Binding<L::Sends>) {
        self.lower.bind(below);
    }

    /// As [`ready_at`](Self::ready_at), at a position not known.
    fn ready(&mut self, run: &RunRequirement) -> impl Future<Output = Result<(), NotReady>> {
        self.ready_at(run, ChainPosition::UNKNOWN)
    }

    /// The driver's `drive`, on the adapter and the chain below as one
    /// executor. Then ends the run's shots on the
    /// adapter, whatever that `drive` gave, so that a joined driver ends the
    /// shots its adapters hold, as the runner ends the executor's, whoever
    /// calls it. A panic ending them is raised again from the run, through any
    /// joined driver above, as one in the runner's end is.
    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = L::Sends>,
    ) -> Result<Vec<U::Output>, DriveError> {
        let (upper, lower, boundary) = self.halves();
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

    /// The driver, then the adapter, given what the driver requires.
    fn declare<'a>(
        &'a self,
        run: &RunRequirement,
        parts: &mut DeclaredParts<'a>,
    ) -> Capabilities<L::Sends> {
        let required = self.upper.declare(run, parts);
        self.lower.declare(&required, parts)
    }

    /// Refuses the run if the halves' sets do not fit, or if what the
    /// driver requires does not fit what the adapter provides, with every
    /// mismatch at their boundary; else the adapter first, for what the driver
    /// requires, then the driver. A refusal from the adapter names it, at its
    /// position, if `at` is known.
    async fn ready_at(
        &mut self,
        run: &RunRequirement,
        at: ChainPosition<'_>,
    ) -> Result<(), NotReady> {
        let (lower, required, below, part) = self.start_run(|upper| upper.requires(run), at)?;
        await_part(lower.ready_at(required, below))
            .await
            .map_err(|not_ready| not_ready.answered_by(part))?;
        await_part(self.upper.ready_at(run, at)).await
    }
}
