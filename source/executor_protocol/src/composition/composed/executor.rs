// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An adapter on an executor: a [`Composed`] in the executor's role.

use super::{Composed, boundary};
use crate::composition::DeclaredParts;
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::part_panics::{await_part, end_both};
use crate::part_id::ChainPosition;
use crate::{
    Adapter, Batch, BatchReadouts, Capabilities, ExecuteError, Executor, InstructionSet, NotReady,
    Seed, ShotSet,
};

impl<U: Adapter, L: Executor> Composed<U, L> {
    /// `upper` on `lower`: binds `upper` where its instructions fit those
    /// `lower` accepts.
    pub(crate) fn adapter_on(mut upper: U, lower: L) -> Self {
        let fit = boundary(upper.sends(), lower.accepts(), lower.provides());
        if let Ok(boundary) = &fit {
            upper.bind(boundary.opcode_map().clone());
        }
        let mut parts = DeclaredParts::checking();
        upper.declare(&Capabilities::UNCONSTRAINED, &mut parts);
        let upper_parts = parts.len();
        Self::new((upper, upper_parts), lower, fit)
    }
}

/// An adapter on an executor: an executor.
impl<U: Adapter, L: Executor> Executor for Composed<U, L> {
    /// The adapter's, which receives its calls.
    fn name(&self) -> &str {
        self.upper.name()
    }

    /// The adapter's.
    fn accepts(&self) -> &InstructionSet {
        self.upper.accepts()
    }

    /// The adapter's: what arrives at the executor is checked against what
    /// it provides when the joined part gets ready.
    fn provides(&self) -> &Capabilities {
        self.upper.provides()
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
        let (upper, lower, boundary) = self.halves();
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

    /// The adapter, then the executor, given what the adapter requires.
    fn declare<'a>(&'a self, above: &Capabilities, parts: &mut DeclaredParts<'a>) {
        let required = self.upper.declare(above, parts);
        self.lower.declare(&required, parts);
    }

    /// Refuses the run if the halves' instructions do not fit, or if what the
    /// adapter requires does not fit what the executor provides, with every
    /// mismatch of the two; else the executor first, for what the adapter
    /// requires, then the adapter, for `required`. A refusal from the executor
    /// names it, at its position, if `at` is known.
    async fn ready_at(
        &mut self,
        required: &Capabilities,
        at: ChainPosition<'_>,
    ) -> Result<(), NotReady> {
        let (lower, below_required, below, part) =
            self.start_run(|upper| upper.requires(required), at)?;
        await_part(lower.ready_at(below_required, below))
            .await
            .map_err(|not_ready| not_ready.answered_by(part))?;
        await_part(self.upper.ready_at(required, at)).await
    }
}
