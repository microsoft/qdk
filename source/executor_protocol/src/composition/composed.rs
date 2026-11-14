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
//! executor by the runner.

use crate::admission::part_list::PartList;
use crate::checked::{Checked, limits};
use crate::instruction_set::instruction::batch_check::BatchLimits;
use crate::{
    Adapt, Batch, BatchReadouts, Capabilities, Drive, DriveError, Execute, ExecuteError,
    InstructionSet, Needs, OpcodeMap, RunRefusal, Seed, ShotSet,
};

/// Two parts as one: `upper` calls `lower`. A driver, an adapter or an
/// executor, as the four joins of [`Compose`](crate::Compose) give.
///
/// It is public because it appears in a chain's type, such as
/// `Runner<Composed<D, A>, E>`, but only `>>` builds one.
pub struct Composed<U, L> {
    upper: U,
    lower: L,
    /// The needs crossing the boundary between the halves, set when the
    /// chain is provisioned.
    below_needs: Option<Needs>,
    /// The limits of the batches crossing that boundary: computed at the
    /// first `execute`, since they need the offer of the chain below,
    /// which never changes once the runner is made.
    limits: Option<BatchLimits>,
}

impl<U, L> Composed<U, L> {
    /// `upper` calling `lower`.
    pub(crate) fn new(upper: U, lower: L) -> Self {
        Self {
            upper,
            lower,
            below_needs: None,
            limits: None,
        }
    }
}

/// The limits cached in `cache`, else those of batches sent to a receiver
/// of `receiver`'s instructions and of the offer `offer` gives, with the
/// needs `below_needs`.
fn cached_limits<'a>(
    cache: &'a mut Option<BatchLimits>,
    below_needs: Option<&Needs>,
    receiver: &InstructionSet,
    offer: impl FnOnce() -> Capabilities,
) -> &'a BatchLimits {
    cache.get_or_insert_with(|| {
        let needs = below_needs.expect("provisioned by the `>>` that makes its runner");
        limits(receiver, &offer(), needs)
    })
}

/// Driver `>>` adapter: a driver.
impl<U: Drive, L: Adapt> Drive for Composed<U, L> {
    type Output = U::Output;

    fn sends(&self) -> &InstructionSet {
        self.lower.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.lower.bind(below);
    }

    fn needs(&self) -> Needs {
        self.needs_by_part(&mut PartList::ignored())
    }

    /// Top down: the driver, then the adapter with the driver's needs.
    fn provision(&mut self) {
        self.upper.provision();
        let below = self.upper.needs();
        self.lower.provision(below.clone());
        self.below_needs = Some(below);
        self.limits = None;
    }

    fn needs_by_part(&self, parts: &mut PartList) -> Needs {
        let above = self.upper.needs_by_part(parts);
        self.lower.needs_by_part(above, parts)
    }

    fn offers_by_part(&self, below: Capabilities, parts: &mut PartList) -> Capabilities {
        let above = self.lower.offers_by_part(below, parts);
        self.upper.offers_by_part(above, parts)
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
        let limits = cached_limits(
            &mut self.limits,
            self.below_needs.as_ref(),
            self.lower.accepts(),
            || self.lower.offers_above(below.offers()),
        );
        let mut rest = ChainBelow {
            adapter: &mut self.lower,
            below,
        };
        self.upper
            .drive(shots, seed, &mut Checked::new(&mut rest, limits))
            .await
    }
}

/// Adapter `>>` adapter: an adapter.
impl<U: Adapt, L: Adapt> Adapt for Composed<U, L> {
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
        self.needs_by_part(above, &mut PartList::ignored())
    }

    fn offers_above(&self, below: Capabilities) -> Capabilities {
        self.offers_by_part(below, &mut PartList::ignored())
    }

    /// Top down: the upper part with `above`, then the lower one with what
    /// the upper part needs below.
    fn provision(&mut self, above: Needs) {
        let below = self.upper.needs_below(above.clone());
        self.upper.provision(above);
        self.lower.provision(below.clone());
        self.below_needs = Some(below);
        self.limits = None;
    }

    fn needs_by_part(&self, above: Needs, parts: &mut PartList) -> Needs {
        let middle = self.upper.needs_by_part(above, parts);
        self.lower.needs_by_part(middle, parts)
    }

    fn offers_by_part(&self, below: Capabilities, parts: &mut PartList) -> Capabilities {
        let middle = self.lower.offers_by_part(below, parts);
        self.upper.offers_by_part(middle, parts)
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
        let limits = cached_limits(
            &mut self.limits,
            self.below_needs.as_ref(),
            self.lower.accepts(),
            || self.lower.offers_above(below.offers()),
        );
        let mut rest = ChainBelow {
            adapter: &mut self.lower,
            below,
        };
        self.upper
            .adapt(batches, seed, &mut Checked::new(&mut rest, limits))
            .await
    }

    /// The upper part, then the lower one.
    fn end_shots(&mut self, shots: &ShotSet) {
        self.upper.end_shots(shots);
        self.lower.end_shots(shots);
    }
}

/// Adapter `>>` executor: an executor.
impl<U: Adapt, L: Execute> Execute for Composed<U, L> {
    fn accepts(&self) -> &InstructionSet {
        self.upper.accepts()
    }

    fn offers(&self) -> Capabilities {
        self.offers_by_part(&mut PartList::ignored())
    }

    /// Top down, as for an adapter.
    fn provision(&mut self, needs: Needs) {
        let below = self.upper.needs_below(needs.clone());
        self.upper.provision(needs);
        self.lower.provision(below.clone());
        self.below_needs = Some(below);
        self.limits = None;
    }

    fn needs_by_part(&self, above: Needs, parts: &mut PartList) -> Needs {
        let below = self.upper.needs_by_part(above, parts);
        self.lower.needs_by_part(below, parts)
    }

    fn offers_by_part(&self, parts: &mut PartList) -> Capabilities {
        let below = self.lower.offers_by_part(parts);
        self.upper.offers_by_part(below, parts)
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
        let limits = cached_limits(
            &mut self.limits,
            self.below_needs.as_ref(),
            self.lower.accepts(),
            || self.lower.offers(),
        );
        self.upper
            .adapt(batches, seed, &mut Checked::new(&mut self.lower, limits))
            .await
    }

    /// The upper part, then the lower one.
    fn end_shots(&mut self, shots: &ShotSet) {
        self.upper.end_shots(shots);
        self.lower.end_shots(shots);
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
    fn accepts(&self) -> &InstructionSet {
        self.adapter.accepts()
    }

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

    /// The adapter, then the chain below it.
    fn end_shots(&mut self, shots: &ShotSet) {
        self.adapter.end_shots(shots);
        self.below.end_shots(shots);
    }
}
