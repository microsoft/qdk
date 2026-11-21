// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An adapter through an adapter: a [`Composed`] in the adapter's role.

use super::{ChainBelow, Composed, DECLARED};
use crate::capabilities::Forwarding;
use crate::composition::DeclaredParts;
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::part_panics::{await_part, end_both};
use crate::part_id::ChainPosition;
use crate::{
    Adapter, Batch, BatchReadouts, BoundaryCompatible, Capabilities, ExecuteError, Executor,
    NotReady, OpcodeMap, Seed, ShotSet,
};

impl<U: Adapter, L: Adapter> Composed<U, L> {
    /// `upper` through `lower`: binds `upper` where its requirement fits
    /// what `lower` provides.
    pub(crate) fn adapter_through(mut upper: U, lower: L) -> Self {
        let fit = upper.requires().fits(lower.provides());
        if let Ok(boundary) = &fit {
            upper.bind(boundary.opcode_map().clone());
        }
        let requires = lower.requires().or(upper.requires());
        let provides = (upper.provides()).stricter_through(upper.requires(), lower.provides());
        let mut parts = DeclaredParts::new();
        upper.declare(&mut parts);
        let upper_parts = parts.len();
        Self::new(
            (upper, upper_parts),
            lower,
            fit,
            Some(requires),
            Some(provides),
        )
    }
}

/// An adapter through an adapter: an adapter.
impl<U: Adapter, L: Adapter> Adapter for Composed<U, L> {
    /// The upper part's, which receives its calls.
    fn name(&self) -> &str {
        self.upper.name()
    }

    /// The upper part's provision, and the stricter of it and the lower
    /// part's, where the upper part does not require it again.
    fn provides(&self) -> &Capabilities {
        self.provides.as_ref().expect(DECLARED)
    }

    /// The lower part's requirement, and the upper part's where the lower
    /// part does not state it.
    fn requires(&self) -> &Capabilities {
        self.requires.as_ref().expect(DECLARED)
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
        let (upper, lower, boundary) = self.run_boundary(|lower| {
            (lower.provides()).stricter_through(lower.requires(), below.provides())
        });
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

    /// The upper part, then the lower one.
    fn declare<'a>(&'a self, parts: &mut DeclaredParts<'a>) {
        self.upper.declare(parts);
        self.lower.declare(parts);
    }

    /// Refuses the run if the halves do not fit; else the lower part
    /// first, for the upper part's requirement, and `required` where the
    /// upper part does not state it; then the upper part, for `required`.
    /// A refusal from the lower part names it, at its position, if `at`
    /// is known.
    async fn ready_at(
        &mut self,
        required: &Capabilities,
        at: ChainPosition<'_>,
    ) -> Result<(), NotReady> {
        let (lower, below_required, below, part) =
            self.start_run(|upper| upper.requires().or(required), at)?;
        await_part(lower.ready_at(below_required, below))
            .await
            .map_err(|not_ready| not_ready.answered_by(part))?;
        await_part(self.upper.ready_at(required, at)).await
    }
}
