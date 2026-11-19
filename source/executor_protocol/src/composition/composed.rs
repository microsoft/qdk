// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Two parts as one: [`Composed`], what `>>` builds, in the role its
//! position gives it.
//!
//! Each of its methods answers from the half that faces the same way: what
//! it accepts from the upper part, what it sends from the lower. At run
//! time, the upper part calls the lower one, with the rest of the chain
//! below it, as one executor:
//!
//! ```text
//! (d >> a).drive(shots, seed, below)
//!     is d.drive(shots, seed, a >> below);
//! (u >> l).adapt(batches, seed, below, readouts)
//!     is u.adapt(batches, seed, l >> below, readouts);
//! (a >> e).execute(batches, seed, readouts)
//!     is a.adapt(batches, seed, e, readouts).
//! ```
//!
//! The upper part reaches the lower one through a boundary crossing
//! (`BoundaryCrossing`), so every boundary of the chain is crossed
//! the same way, whatever the brackets: the boundary inside a `Composed`
//! here, and the one where the driver meets the executor by the runner.
//! Each shapes the readouts that cross it, checks every batch's count of
//! instructions against the most its receiver takes, and checks each
//! batch its sender marked [`checked`](Batch::checked) against the
//! receiver's instruction set, the shots' needs and the readout slots the
//! batch states. It names a panic below it by the part that receives the
//! calls. The limits and the names are kept when the `>>` that makes the
//! runner reads every part's set, needs and offer, so that the checks and
//! the names are the same wherever the brackets go. So are the needs that
//! cross the boundary, which the lower part gets ready for on every run.

mod adapt;
mod drive;
mod execute;

use super::boundary_crossing::BatchLimits;
use super::part_panics::end_both;
use crate::{
    Adapt, AdapterSetup, Batch, BatchReadouts, Execute, ExecuteError, ExecutorSetup,
    InstructionSet, Needs, Offer, PartId, Seed, ShotSet,
};

/// Two parts as one: `upper` calls `lower`. A driver, an adapter or an
/// executor, as three of the four joins of [`Compose`](crate::Compose)
/// give.
///
/// It is public because it appears in a chain's type, such as
/// `Runner<Composed<D, A>, E>`, but only `>>` builds one.
pub struct Composed<U, L> {
    upper: U,
    lower: L,
    /// The lower half's top part, the one that receives its calls, as the
    /// `>>` that makes the runner lists it: what a panic of the lower half
    /// names, and what its refusal to get ready names when no part inside
    /// it was named.
    below_part: Option<PartId>,
    /// The needs crossing the boundary between the halves, the upper
    /// half's adapted down to it, kept when the `>>` that makes the runner
    /// reads every part's needs: what the lower half gets ready for on
    /// every run.
    below_needs: Option<Needs>,
    /// The offer crossing that boundary, the lower half's with the chain
    /// below it, kept when the `>>` that makes the runner reads every
    /// part's offer.
    below_offer: Option<Offer>,
    /// The limits of the batches crossing that boundary, from the lower
    /// half's set, that offer and those needs, kept at the same time.
    batch_limits: Option<BatchLimits>,
}

/// Why each of a `Composed`'s kept values is there once it runs.
const KEPT: &str = "kept by the `>>` that makes its runner";

impl<U, L> Composed<U, L> {
    /// `upper` calling `lower`.
    pub(crate) fn new(upper: U, lower: L) -> Self {
        Self {
            upper,
            lower,
            below_part: None,
            below_needs: None,
            below_offer: None,
            batch_limits: None,
        }
    }
}

/// An adapter with the rest of the chain below it, as one executor: what a
/// `Composed` lends its upper part at run time. Unlike a `Composed`, it
/// checks no boundary of its own: the boundary between `adapter` and
/// `below` is checked where `below` was lent.
struct ChainBelow<'a, A, B> {
    adapter: &'a mut A,
    below: &'a mut B,
}

impl<A: AdapterSetup, B: ExecutorSetup> ExecutorSetup for ChainBelow<'_, A, B> {
    /// The adapter's, which receives its calls.
    fn name(&self) -> &str {
        self.adapter.name()
    }

    fn accepts(&self) -> &InstructionSet {
        self.adapter.accepts()
    }

    /// What the adapter offers above the chain below. Not asked at run
    /// time: the boundary crossing the part above calls answers the
    /// offer kept for the boundary.
    fn offers(&self) -> Offer {
        self.adapter.offers_above(self.below.offers())
    }
}

impl<A: Adapt, B: Execute> Execute for ChainBelow<'_, A, B> {
    /// The adapter's `adapt`: it keeps [`Execute::execute`]'s rule, doing
    /// its work only once polled, when the adapter's `adapt` does, and
    /// either way it is reached only through a boundary crossing, which
    /// calls it only once its own future is polled.
    fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.adapter
            .adapt(batches, seed, &mut *self.below, readouts)
    }

    /// The adapter, then the chain below it, ended even if the adapter
    /// panics.
    fn end_shots(&mut self, shots: &ShotSet) {
        end_both(
            || self.adapter.end_shots(shots),
            || self.below.end_shots(shots),
        );
    }
}

/// The lower half's top part, the offer and the batch limits a `Composed`
/// keeps for its boundary.
fn kept<'a>(
    part: Option<&'a PartId>,
    offer: Option<&'a Offer>,
    batch_limits: Option<&'a BatchLimits>,
) -> (&'a PartId, &'a Offer, &'a BatchLimits) {
    (
        part.expect(KEPT),
        offer.expect(KEPT),
        batch_limits.expect(KEPT),
    )
}

/// The limits of the batches sent across a boundary to a receiver of
/// `receiver`'s instructions, with the offer `offer` kept for it, for
/// `below_needs`, the needs crossing it, which the `>>` that makes the
/// runner keeps before it reads the offers.
fn limits_below(
    offer: &Offer,
    receiver: &InstructionSet,
    below_needs: Option<&Needs>,
) -> BatchLimits {
    BatchLimits::new(
        receiver,
        offer,
        below_needs.expect("kept by `needs_by_part`, called before `offers_by_part`"),
    )
}
