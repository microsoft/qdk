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
//! - `(d >> a).drive(shots, seed, below)` is `d.drive(shots, seed, a >> below)`;
//! - `(u >> l).adapt(batches, seed, below)` is `u.adapt(batches, seed, l >> below)`;
//! - `(a >> e).execute(batches, seed)` is `a.adapt(batches, seed, e)`.
//!
//! The upper part reaches the lower one through a `Checked`, so every
//! boundary of the chain is crossed through one, whatever the brackets: the
//! boundary inside a `Composed` here, and the one where the driver meets the
//! executor by the runner. Each checks the answers that cross it, and the
//! batches their senders marked [`checked`](Batch::checked), against the
//! offer that crosses it, and names a panic below it by the part that
//! receives the calls, both kept when the `>>` that makes the runner reads
//! every part's offer, before any part is provisioned, so that the checks
//! and the names are the same wherever the brackets go.

mod adapt;
mod drive;
mod execute;

use crate::checked::batch_limits;
use crate::instruction_set::instruction::batch_check::BatchLimits;
use crate::panics::end_both;
use crate::{
    Adapt, Batch, Capabilities, ChainPart, Execute, ExecuteError, InstructionSet, Needs, Readouts,
    Seed, ShotSet,
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
    /// names.
    below_part: Option<ChainPart>,
    /// The offer crossing the boundary between the halves, the lower
    /// half's with the chain below it, kept when the `>>` that makes the
    /// runner reads every part's offer, before any part is provisioned.
    below_offer: Option<Capabilities>,
    /// The limits of the checked batches crossing that boundary, from that
    /// offer and the needs crossing it, set when the chain is provisioned.
    batch_limits: Option<BatchLimits>,
}

impl<U, L> Composed<U, L> {
    /// `upper` calling `lower`.
    pub(crate) fn new(upper: U, lower: L) -> Self {
        Self {
            upper,
            lower,
            below_part: None,
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

impl<A: Adapt, B: Execute> Execute for ChainBelow<'_, A, B> {
    /// The adapter's, which receives its calls.
    fn name(&self) -> &str {
        self.adapter.name()
    }

    fn accepts(&self) -> &InstructionSet {
        self.adapter.accepts()
    }

    /// What the adapter offers above the chain below. Not asked at run
    /// time: the `Checked` the part above calls answers the offer kept
    /// for the boundary.
    fn offers(&self) -> Capabilities {
        self.adapter.offers_above(self.below.offers())
    }

    fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut Readouts,
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
    part: Option<&'a ChainPart>,
    offer: Option<&'a Capabilities>,
    batch_limits: Option<&'a BatchLimits>,
) -> (&'a ChainPart, &'a Capabilities, &'a BatchLimits) {
    let precondition = "kept and provisioned by the `>>` that makes its runner";
    (
        part.expect(precondition),
        offer.expect(precondition),
        batch_limits.expect(precondition),
    )
}

/// The limits of the batches sent across a boundary to a receiver of
/// `receiver`'s instructions, with the offer `offer` kept for it, for
/// `below_needs`, the needs crossing it.
fn limits_below(
    offer: Option<&Capabilities>,
    receiver: &InstructionSet,
    below_needs: &Needs,
) -> BatchLimits {
    let offer = offer.expect("kept by the `>>` that makes its runner, before it provisions");
    batch_limits(receiver, offer, below_needs)
}
