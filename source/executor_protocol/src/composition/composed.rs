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
//! calls. The limits and the names are those the
//! [`on`](crate::DriverJoins::on) that makes the runner found when it
//! checked the chain, as long as that runner lives, so that the checks and
//! the names are the same wherever the brackets go; so are the needs that
//! cross the boundary, which the lower part gets ready for on every run.
//!
//! A `Composed` keeps its role's contract through its public methods
//! alone, so it runs the same when that check did not reach it, such as
//! inside a part of the user's that forwards only the role's public
//! methods: each run then reads its boundary from its halves
//! as the run starts, the needs at `ready` and the offer at the first
//! call; a panic or a refusal below it passes up unnamed, and the boundary
//! above names the part at its position; and a `Composed` whose halves do
//! not fit refuses the run at `ready`, with the fit faults, rather than
//! running an unbound opcode.

mod adapter;
mod driver;
mod executor;

use std::sync::{Arc, Weak};

use super::boundary_crossing::Boundary;
use super::part_panics::end_both;
use crate::{
    Adapter, Batch, BatchReadouts, ExecuteError, Executor, FitFaults, InstructionSet, Needs,
    NotReady, Offer, PartId, Seed, ShotSet,
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
    /// other fault of that chain; and every run refuses at `ready` with
    /// them, for a chain whose check did not reach this part.
    fit_faults: Option<FitFaults>,
    /// That boundary, as the last chain's check that reached this part
    /// found it, held by the runner that check made: alive as long as that
    /// runner is.
    checked_boundary: Weak<Boundary>,
    /// That boundary on this run: the checked one, or one read from the
    /// halves, once the run's first call reads its offer.
    boundary: Option<Arc<Boundary>>,
    /// The needs read from the upper half at `ready`, for a run whose
    /// boundary is read from the halves, until its first call reads it.
    run_needs: Option<Needs>,
}

/// Why a `Composed` has its run's needs at its run's first call.
const READY_FIRST: &str = "each run gets a part ready before calling it, as its role says";

impl<U, L> Composed<U, L> {
    /// `upper` calling `lower`, with the faults of their boundary, if it
    /// does not fit.
    pub(crate) fn new(upper: U, lower: L, fit_faults: Option<FitFaults>) -> Self {
        Self {
            upper,
            lower,
            fit_faults,
            checked_boundary: Weak::new(),
            boundary: None,
            run_needs: None,
        }
    }

    /// Starts a run, at `ready`: refuses it if the halves do not fit, with
    /// their fit faults; otherwise takes their boundary as the chain's
    /// check found it, while the runner it made lives, or else the needs
    /// `needs_below` reads from the upper half, for a boundary read on this
    /// run. Gives the lower half, the needs it gets ready
    /// for, and its position in the chain, if the check found it, to name
    /// its refusal.
    fn start_run(
        &mut self,
        needs_below: impl FnOnce(&U) -> Needs,
    ) -> Result<(&mut L, &Needs, Option<&PartId>), NotReady> {
        if let Some(faults) = &self.fit_faults {
            return Err(NotReady::new(faults.clone()));
        }
        let Self {
            upper,
            lower,
            checked_boundary,
            boundary,
            run_needs,
            ..
        } = self;
        // Dropped first: the boundary of the last run, if it was checked,
        // would otherwise keep alive the check of a runner since dropped.
        *boundary = None;
        *boundary = checked_boundary.upgrade();
        Ok(match boundary {
            Some(boundary) => (lower, &boundary.needs, boundary.part.as_ref()),
            None => (lower, run_needs.insert(needs_below(upper)), None),
        })
    }

    /// The halves, and their boundary on this run: as
    /// [`start_run`](Self::start_run) took it, or else read now, at the
    /// run's first call, from the needs read there, and the offer and the
    /// receiver's set `read` gives from the lower half.
    fn run_boundary(
        &mut self,
        read: impl FnOnce(&L) -> (Offer, InstructionSet),
    ) -> (&mut U, &mut L, &Boundary) {
        let Self {
            upper,
            lower,
            boundary,
            run_needs,
            ..
        } = self;
        let boundary = boundary.get_or_insert_with(|| {
            let (offer, receiver) = read(lower);
            let needs = run_needs.take().expect(READY_FIRST);
            Arc::new(Boundary::new(None, needs, offer, receiver))
        });
        (upper, lower, boundary)
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
