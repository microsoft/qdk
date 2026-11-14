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
//! boundary of the chain is checked once, whatever the brackets: the
//! boundary inside a `Composed` here, and the one where the driver meets the
//! executor by the runner. Each checks against the offer that crosses it,
//! and names a panic below it by the part that receives the calls, both
//! kept when the `>>` that makes the runner reads every part's offer, before
//! any part is provisioned, so that the checks and the names are the same
//! wherever the brackets go.

use crate::admission::by_part::ByPart;
use crate::checked::{Checked, batch_limits};
use crate::instruction_set::instruction::batch_check::BatchLimits;
use crate::panics::end_both;
use crate::roles::FailedRun;
use crate::{
    Adapt, Batch, BatchReadouts, Capabilities, ChainPart, Drive, DriveError, Execute, ExecuteError,
    InstructionSet, Needs, OpcodeMap, RunRefusal, Seed, ShotSet,
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
    /// The limits of the batches crossing that boundary, from that offer
    /// and the needs crossing it, set when the chain is provisioned.
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

/// Driver `>>` adapter: a driver.
impl<U: Drive, L: Adapt> Drive for Composed<U, L> {
    type Output = U::Output;

    /// The driver's.
    fn name(&self) -> &str {
        self.upper.name()
    }

    fn sends(&self) -> &InstructionSet {
        self.lower.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.lower.bind(below);
    }

    fn needs(&self) -> Needs {
        self.needs_by_part(&mut ByPart::ignored())
    }

    /// Top down: the driver, then the adapter with the driver's needs, as
    /// the admission checked them, read before the driver is provisioned.
    fn provision(&mut self) {
        let below = self.upper.needs();
        self.batch_limits = Some(limits_below(
            self.below_offer.as_ref(),
            self.lower.accepts(),
            &below,
        ));
        self.upper.provision();
        self.lower.provision(below);
    }

    fn needs_by_part(&self, by_part: &mut ByPart) -> Needs {
        let above = self.upper.needs_by_part(by_part);
        self.lower.needs_by_part(above, by_part)
    }

    fn offers_by_part(&mut self, below: Capabilities, by_part: &mut ByPart) -> Capabilities {
        let above = self.lower.offers_by_part(below, by_part);
        self.below_part = by_part.last_offered();
        self.below_offer = Some(above.clone());
        self.upper.offers_by_part(above, by_part)
    }

    /// The driver's adapters, then this one, each ended even if one
    /// before it panics.
    fn end_failed_run(&mut self, run: FailedRun<'_>) {
        end_both(
            || self.upper.end_failed_run(run),
            || self.lower.end_shots(run.shots),
        );
    }

    /// The part below first.
    async fn ready(&mut self) -> Result<(), RunRefusal> {
        self.lower.ready().await?;
        self.upper.ready().await
    }

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<U::Output>, DriveError> {
        let (part, offer, batch_limits) = kept(
            self.below_part.as_ref(),
            self.below_offer.as_ref(),
            self.batch_limits.as_ref(),
        );
        let mut rest = ChainBelow {
            adapter: &mut self.lower,
            below,
        };
        self.upper
            .drive(
                shots,
                seed,
                &mut Checked::new(&mut rest, part, offer, batch_limits),
            )
            .await
    }
}

/// Adapter `>>` adapter: an adapter.
impl<U: Adapt, L: Adapt> Adapt for Composed<U, L> {
    /// The upper part's, which receives its calls.
    fn name(&self) -> &str {
        self.upper.name()
    }

    fn accepts(&self) -> &InstructionSet {
        self.upper.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.lower.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.lower.bind(below);
    }

    fn needs_below(&self, above: Needs) -> Needs {
        self.needs_by_part(above, &mut ByPart::ignored())
    }

    fn offers_above(&self, below: Capabilities) -> Capabilities {
        self.upper.offers_above(self.lower.offers_above(below))
    }

    /// Top down: the upper part with `above`, then the lower one with what
    /// the upper part needs below.
    fn provision(&mut self, above: Needs) {
        let below = self.upper.needs_below(above.clone());
        self.batch_limits = Some(limits_below(
            self.below_offer.as_ref(),
            self.lower.accepts(),
            &below,
        ));
        self.upper.provision(above);
        self.lower.provision(below);
    }

    fn needs_by_part(&self, above: Needs, by_part: &mut ByPart) -> Needs {
        let middle = self.upper.needs_by_part(above, by_part);
        self.lower.needs_by_part(middle, by_part)
    }

    fn offers_by_part(&mut self, below: Capabilities, by_part: &mut ByPart) -> Capabilities {
        let middle = self.lower.offers_by_part(below, by_part);
        self.below_part = by_part.last_offered();
        self.below_offer = Some(middle.clone());
        self.upper.offers_by_part(middle, by_part)
    }

    /// The part below first.
    async fn ready(&mut self) -> Result<(), RunRefusal> {
        self.lower.ready().await?;
        self.upper.ready().await
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        let (part, offer, batch_limits) = kept(
            self.below_part.as_ref(),
            self.below_offer.as_ref(),
            self.batch_limits.as_ref(),
        );
        let mut rest = ChainBelow {
            adapter: &mut self.lower,
            below,
        };
        self.upper
            .adapt(
                batches,
                seed,
                &mut Checked::new(&mut rest, part, offer, batch_limits),
            )
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
}

/// Adapter `>>` executor: an executor.
impl<U: Adapt, L: Execute> Execute for Composed<U, L> {
    /// The adapter's, which receives its calls.
    fn name(&self) -> &str {
        self.upper.name()
    }

    fn accepts(&self) -> &InstructionSet {
        self.upper.accepts()
    }

    fn offers(&self) -> Capabilities {
        self.upper.offers_above(self.lower.offers())
    }

    /// Top down, as for an adapter.
    fn provision(&mut self, needs: Needs) {
        let below = self.upper.needs_below(needs.clone());
        self.batch_limits = Some(limits_below(
            self.below_offer.as_ref(),
            self.lower.accepts(),
            &below,
        ));
        self.upper.provision(needs);
        self.lower.provision(below);
    }

    fn needs_by_part(&self, above: Needs, by_part: &mut ByPart) -> Needs {
        let below = self.upper.needs_by_part(above, by_part);
        self.lower.needs_by_part(below, by_part)
    }

    fn offers_by_part(&mut self, by_part: &mut ByPart) -> Capabilities {
        let below = self.lower.offers_by_part(by_part);
        self.below_part = by_part.last_offered();
        self.below_offer = Some(below.clone());
        self.upper.offers_by_part(below, by_part)
    }

    /// The part below first.
    async fn ready(&mut self) -> Result<(), RunRefusal> {
        self.lower.ready().await?;
        self.upper.ready().await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        let (part, offer, batch_limits) = kept(
            self.below_part.as_ref(),
            self.below_offer.as_ref(),
            self.batch_limits.as_ref(),
        );
        self.upper
            .adapt(
                batches,
                seed,
                &mut Checked::new(&mut self.lower, part, offer, batch_limits),
            )
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
    ) -> impl Future<Output = Result<Vec<BatchReadouts>, ExecuteError>> {
        self.adapter.adapt(batches, seed, &mut *self.below)
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
