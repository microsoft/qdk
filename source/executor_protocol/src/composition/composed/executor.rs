// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An adapter on an executor: a [`Composed`] in the executor's role.

use super::{Composed, DECLARED};
use crate::capabilities::Forwarding;
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::part_panics::{await_part, end_both};
use crate::composition::{ChainPosition, DeclaredParts};
use crate::{
    Adapter, Batch, BatchReadouts, BoundaryCompatible, Capabilities, ExecuteError, Executor,
    NotReady, Seed, ShotSet,
};

impl<U: Adapter, L: Executor> Composed<U, L> {
    /// `upper` on `lower`: binds `upper` where its requirement fits what
    /// `lower` provides.
    pub(crate) fn adapter_on(mut upper: U, lower: L) -> Self {
        let fit = upper.requires().fits(lower.provides());
        if let Ok(boundary) = &fit {
            upper.bind(boundary.opcode_map().clone());
        }
        let provides = upper
            .provides()
            .stricter(&lower.provides().unanswered(upper.requires()));
        let mut parts = DeclaredParts::new();
        upper.declare(&mut parts);
        let upper_parts = parts.len();
        Self::new((upper, upper_parts), lower, fit, None, Some(provides))
    }
}

/// An adapter on an executor: an executor.
impl<U: Adapter, L: Executor> Executor for Composed<U, L> {
    /// The adapter's, which receives its calls.
    fn name(&self) -> &str {
        self.upper.name()
    }

    /// The adapter's provision, and the stricter of it and the executor's,
    /// where the adapter does not require it again.
    fn provides(&self) -> &Capabilities {
        self.provides.as_ref().expect(DECLARED)
    }

    /// As [`ready_at`](Self::ready_at), at a position not known.
    fn ready(&mut self, required: &Capabilities) -> impl Future<Output = Result<(), NotReady>> {
        self.ready_at(required, ChainPosition::UNKNOWN)
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let (upper, lower, boundary) = self.run_boundary(|lower| lower.provides().clone());
        await_part(upper.adapt(
            batches,
            seed,
            &mut BoundaryCrossing::new(lower, boundary),
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

    /// The adapter, then the executor.
    fn declare<'a>(&'a self, parts: &mut DeclaredParts<'a>) {
        self.upper.declare(parts);
        self.lower.declare(parts);
    }

    /// Refuses the run if the halves do not fit; else the executor first,
    /// for the adapter's requirement, and `required` where the adapter does
    /// not state it; then the adapter, for `required`. A refusal from the
    /// executor names it, at its position, if `at` is known.
    async fn ready_at(
        &mut self,
        required: &Capabilities,
        at: ChainPosition,
    ) -> Result<(), NotReady> {
        let (lower, below_required, below, part) = self.start_run(
            |upper| upper.requires().or(required),
            |lower| lower.name(),
            at,
        )?;
        await_part(lower.ready_at(below_required, below))
            .await
            .map_err(|not_ready| not_ready.answered_by(part))?;
        await_part(self.upper.ready_at(required, at)).await
    }
}
