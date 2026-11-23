// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An adapter through an adapter: a [`Composed`] in the adapter's role.

use super::{ChainBelow, Composed, boundary};
use crate::composition::DeclaredParts;
use crate::composition::boundary_crossing::BoundaryCrossing;
use crate::composition::part_panics::{await_part, end_both};
use crate::part_id::ChainPosition;
use crate::roles::{Sealed, lent_translating_adapter};
use crate::{
    Batch, BatchReadouts, Binding, Capabilities, ExecuteError, Executor, NotReady, Seed, ShotSet,
    TranslatingAdapter,
};

impl<U: TranslatingAdapter, L: TranslatingAdapter<Accepts = U::Sends>> Composed<U, L, U::Sends> {
    /// `upper` through `lower`: binds `upper` where its set fits the one
    /// `lower` accepts.
    pub(crate) fn adapter_through(mut upper: U, lower: L) -> Self {
        let fit = boundary(upper.sends(), lower.accepts(), lower.provides()).map(
            |(boundary, binding)| {
                upper.bind(binding);
                boundary
            },
        );
        let mut parts = DeclaredParts::without_listing();
        upper.count_parts(&mut parts);
        let upper_parts = parts.len();
        Self::new((upper, upper_parts), lower, fit)
    }
}

/// An adapter through an adapter: an adapter.
impl<U: TranslatingAdapter, L: TranslatingAdapter<Accepts = U::Sends>> TranslatingAdapter
    for Composed<U, L, U::Sends>
{
    type Accepts = U::Accepts;
    type Sends = L::Sends;

    /// The upper part's, which receives its calls.
    fn name(&self) -> &str {
        self.upper.name()
    }

    /// The upper part's.
    fn accepts(&self) -> &U::Accepts {
        self.upper.accepts()
    }

    /// The lower part's.
    fn sends(&self) -> &L::Sends {
        self.lower.sends()
    }

    /// The upper part's: what arrives at the lower part is checked against
    /// what it provides when the joined part gets ready.
    fn provides(&self) -> Option<&Capabilities<U::Accepts>> {
        self.upper.provides()
    }

    /// What the lower part requires, given what the upper part requires.
    fn requires(&self, above: &Capabilities<U::Accepts>) -> Capabilities<L::Sends> {
        self.lower.requires(&self.upper.requires(above))
    }

    fn bind(&mut self, below: Binding<L::Sends>) {
        self.lower.bind(below);
    }

    /// As [`ready_at`](Self::ready_at), at a position not known.
    fn ready(
        &mut self,
        above: &Capabilities<U::Accepts>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        self.ready_at(above, ChainPosition::UNKNOWN)
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, U::Accepts>],
        seed: Seed,
        below: &mut impl Executor<Accepts = L::Sends>,
        readouts: &mut [BatchReadouts<U::Accepts>],
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
    fn declare<'a>(
        &'a self,
        above: &Capabilities<U::Accepts>,
        parts: &mut DeclaredParts<'a>,
    ) -> Capabilities<L::Sends> {
        let required = self.upper.declare(above, parts);
        self.lower.declare(&required, parts)
    }

    /// The upper part's, then the lower one's.
    fn count_parts<'a>(&'a self, parts: &mut DeclaredParts<'a>) {
        self.upper.count_parts(parts);
        self.lower.count_parts(parts);
    }

    /// Refuses the run if the halves' sets do not fit, or if what the
    /// upper part requires does not fit what the lower part provides, with
    /// every mismatch at their boundary; else the lower part first, for what
    /// the upper part requires, then the upper part, for `above`. A refusal
    /// from the lower part names it, at its position, if `at` is known.
    async fn ready_at(
        &mut self,
        above: &Capabilities<U::Accepts>,
        at: ChainPosition<'_>,
    ) -> Result<(), NotReady> {
        let (lower, below_required, below, part) =
            self.start_run(|upper| upper.requires(above), at)?;
        await_part(lower.ready_at(below_required, below))
            .await
            .map_err(|not_ready| not_ready.answered_by(part))?;
        let upper = await_part(self.upper.ready_at(above, at)).await;
        self.finish_run(upper)
    }

    /// Refuses the run as [`ready_at`](Self::ready_at) does, for what
    /// arrives from a part above that states nothing, at a position not
    /// known; else the lower part, then the upper one, each as a part above
    /// that states nothing readies it.
    async fn ready_alone(&mut self, sealed: Sealed) -> Result<(), NotReady> {
        let (lower, ..) = self.start_run(
            |upper| upper.requires(&Capabilities::needs_nothing()),
            ChainPosition::UNKNOWN,
        )?;
        await_part(lower.ready_alone(Sealed(()))).await?;
        let upper = await_part(self.upper.ready_alone(sealed)).await;
        self.finish_run(upper)
    }
}

lent_translating_adapter!(
    [U: TranslatingAdapter, L: TranslatingAdapter<Accepts = U::Sends>] Composed<U, L, U::Sends>
);
