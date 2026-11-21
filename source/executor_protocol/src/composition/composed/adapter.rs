// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An adapter through an adapter: a [`Composed`] in the adapter's role.

use super::{ChainBelow, Composed, boundary};
use crate::composition::DeclaredParts;
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::part_panics::{await_part, end_both};
use crate::part_id::ChainPosition;
use crate::{
    Adapter, Batch, BatchReadouts, Capabilities, ExecuteError, Executor, InstructionSet, NotReady,
    OpcodeMap, Seed, ShotSet,
};

impl<U: Adapter, L: Adapter> Composed<U, L> {
    /// `upper` through `lower`: binds `upper` where its instructions fit
    /// those `lower` accepts.
    pub(crate) fn adapter_through(mut upper: U, lower: L) -> Self {
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

/// An adapter through an adapter: an adapter.
impl<U: Adapter, L: Adapter> Adapter for Composed<U, L> {
    /// The upper part's, which receives its calls.
    fn name(&self) -> &str {
        self.upper.name()
    }

    /// The upper part's.
    fn accepts(&self) -> &InstructionSet {
        self.upper.accepts()
    }

    /// The lower part's.
    fn sends(&self) -> &InstructionSet {
        self.lower.sends()
    }

    /// The upper part's: what arrives at the lower part is checked against
    /// what it provides when the joined part gets ready.
    fn provides(&self) -> &Capabilities {
        self.upper.provides()
    }

    /// What the lower part requires, given what the upper part requires.
    fn requires(&self, above: &Capabilities) -> Capabilities {
        self.lower.requires(&self.upper.requires(above))
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.lower.bind(below);
    }

    /// As [`ready_at`](Self::ready_at), at a position not known.
    fn ready(&mut self, required: &Capabilities) -> impl Future<Output = Result<(), NotReady>> {
        self.ready_at(required, ChainPosition::UNKNOWN)
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let (upper, lower, boundary) = self.halves();
        let mut rest = ChainBelow {
            adapter: lower,
            below,
        };
        await_part(upper.adapt(
            batches,
            seed,
            &mut BoundaryCrossing::new(&mut rest, boundary),
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

    /// The upper part, then the lower one, given what the upper part
    /// requires.
    fn declare<'a>(&'a self, above: &Capabilities, parts: &mut DeclaredParts<'a>) -> Capabilities {
        let required = self.upper.declare(above, parts);
        self.lower.declare(&required, parts)
    }

    /// Refuses the run if the halves' instructions do not fit, or if what the
    /// upper part requires does not fit what the lower part provides, with
    /// every mismatch of the two; else the lower part first, for what the upper
    /// part requires, then the upper part, for `required`. A refusal from the
    /// lower part names it, at its position, if `at` is known.
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
