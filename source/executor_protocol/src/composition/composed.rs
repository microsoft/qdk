// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Two parts as one: `Composed`, what a join builds, in the role its
//! position gives it.
//!
//! Each of its methods answers from the half that faces the same way: what
//! it accepts from the upper part, what it sends from the lower. At run
//! time, the upper part calls the lower one, with the rest of the chain
//! below it, as one executor:
//!
//! ```text
//! d.through(a).drive(shots, seed, below)
//!     is d.drive(shots, seed, a.on(below));
//! u.through(l).adapt(batches, seed, below, readouts)
//!     is u.adapt(batches, seed, l.on(below), readouts);
//! a.on(e).execute(batches, seed, readouts)
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
//! calls. The limits and the names are kept when the
//! [`on`](crate::DriverJoins::on) that makes the runner checks the chain,
//! so that the checks and the names are the same wherever the brackets go.
//! So are the needs that cross the boundary, which the lower part gets
//! ready for on every run.

mod adapter;
mod driver;
mod executor;

use super::boundary_crossing::Crossing;
use super::part_panics::end_both;
use crate::{
    Adapter, Batch, BatchReadouts, ExecuteError, Executor, FitFaults, InstructionSet, Offer, Seed,
    ShotSet,
};

/// Two parts as one: `upper` calls `lower`. A driver, an adapter or an
/// executor, as [`DriverJoins::through`](crate::DriverJoins::through),
/// [`AdapterJoins::through`](crate::AdapterJoins::through) and
/// [`AdapterJoins::on`](crate::AdapterJoins::on) give.
///
/// It is public, because the joins return it, but no public path reaches
/// it: code outside the crate holds one by its role alone, so that a
/// join's type, and with it the runner's, is the protocol's to change.
pub struct Composed<U, L> {
    upper: U,
    lower: L,
    /// The faults of the boundary between the halves, when the upper half
    /// does not fit the lower one, as the join found them: every
    /// [`on`](crate::DriverJoins::on) that makes a runner of a chain
    /// holding this part, itself or lent by `&mut`, reports them with every
    /// other fault of that chain.
    fit_faults: Option<FitFaults>,
    /// What crosses that boundary, kept when the
    /// [`on`](crate::DriverJoins::on) that makes the runner checks the
    /// chain.
    crossing: Option<Crossing>,
}

/// Why a `Composed`'s crossing is there once it runs.
const KEPT: &str = "kept by the `on` that makes its runner";

impl<U, L> Composed<U, L> {
    /// `upper` calling `lower`, with the faults of their boundary, if it
    /// does not fit.
    pub(crate) fn new(upper: U, lower: L, fit_faults: Option<FitFaults>) -> Self {
        Self {
            upper,
            lower,
            fit_faults,
            crossing: None,
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

impl<A: Adapter, B: Executor> Executor for ChainBelow<'_, A, B> {
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

    /// The adapter's `adapt`: it keeps [`Executor::execute`]'s rule, doing
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
