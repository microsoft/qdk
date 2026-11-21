// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Two parts as one: `Composed`, what a join builds, in the role its
//! position gives it.
//!
//! Each of its methods answers from the half that faces the same way: what
//! it provides from the upper half, what it requires from the lower, each
//! with what the other half forwards. At run
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
//! instructions against the most its receiver provides for, and checks
//! each batch its sender marked [`checked`](Batch::checked) against the
//! receiver's instructions, what is required across and the readout slots
//! the batch states. It names a panic below it by the part that receives
//! the calls, at its position, when the run's `ready` gave it one.
//!
//! What crosses the boundary is the [`Boundary`] the join's fit gave,
//! resolved on each run with what the parts that do not state a resource
//! or a feature forward: what is required, at `ready`, from what the part
//! above it requires; what is provided, at the run's first call, from what
//! the chain below provides.
//!
//! A `Composed` keeps its role's contract through its public methods
//! alone, so it gives the same outputs when the chain's check did not
//! reach it, such as inside a part of the user's that forwards only the
//! role's public methods: a panic or a refusal below it then passes up
//! unnamed, and the boundary above names the part at its position; a
//! `Composed` whose halves do not fit refuses the run at `ready`, with
//! their mismatch, rather than running an unbound opcode; and a joined
//! driver fails a run its adapters do not provide before its driver runs
//! anything, where the runner would have refused it.

mod adapter;
mod driver;
mod executor;

use super::part_panics::end_both;
use crate::PartId;
use crate::part_id::ChainPosition;
use crate::{
    Adapter, Batch, BatchReadouts, Boundary, Capabilities, CapabilitiesMismatch, ExecuteError,
    Executor, NotReady, Seed, ShotSet,
};

/// Two parts as one: `upper` calls `lower`. A driver, an adapter or an
/// executor, as [`DriverJoins::through`](crate::DriverJoins::through),
/// [`AdapterJoins::through`](crate::AdapterJoins::through) and
/// [`AdapterJoins::on`](crate::AdapterJoins::on) give.
///
/// It is public, because the joins return it, but no public path reaches
/// it: code outside the crate holds one by its role alone, so that a
/// join's type, and with it the runner's, is the protocol's to change.
/// The joins return it, rather than an `impl` of the role, so that its
/// `Send`, and that of the futures of its methods, follows its halves'.
pub struct Composed<U, L> {
    upper: U,
    lower: L,
    /// The boundary between the halves, as the join's fit gave it, and as
    /// each run resolves it; or, when the upper half's requirement does
    /// not fit the lower half's provision, their mismatch, with which
    /// every run refuses at `ready`.
    fit: Result<Boundary, CapabilitiesMismatch>,
    /// What the two halves require together, for the driver's and the
    /// adapter's roles: the lower half's requirement, and the upper's
    /// where the lower does not state it.
    requires: Option<Capabilities>,
    /// What the two halves provide together, for the adapter's and the
    /// executor's roles: the upper half's provision, and the stricter of
    /// it and the lower's, where the upper does not require it again.
    provides: Option<Capabilities>,
    /// The parts the upper half holds, as it declares them: how far below
    /// this part's position the lower half's is.
    upper_parts: usize,
    /// Whether this run's boundary has what the chain below provides:
    /// reset at `ready`, set at the run's first call.
    provided_on_run: bool,
}

/// Why a `Composed` has its boundary at its run's first call.
const READY_FIRST: &str = "each run gets a part ready before calling it, as its role says, \
                           and a joined part whose halves do not fit refuses every run then";

/// Why a `Composed` has what it requires or provides together.
const DECLARED: &str = "the join that made a joined part in this role combines it";

impl<U, L> Composed<U, L> {
    /// `upper` calling `lower`, where the upper half's requirement meets
    /// the lower half's provision as `fit` says, and the halves require
    /// and provide `requires` and `provides` together; the upper half
    /// declares `upper_parts` parts.
    fn new(
        (upper, upper_parts): (U, usize),
        lower: L,
        fit: Result<Boundary, CapabilitiesMismatch>,
        requires: Option<Capabilities>,
        provides: Option<Capabilities>,
    ) -> Self {
        Self {
            upper,
            lower,
            fit,
            requires,
            provides,
            upper_parts,
            provided_on_run: true,
        }
    }

    /// Starts a run, at `ready`, of this part at `at`: refuses it if the
    /// halves do not fit, with their mismatch; otherwise resolves the
    /// boundary with `required`, what the lower half is required on this
    /// run. Gives the lower half, what it gets ready for, its position,
    /// and the part a refusal from it names, the lower half by `name`, if
    /// that position is known.
    fn start_run(
        &mut self,
        required: impl FnOnce(&U) -> Capabilities,
        name: impl FnOnce(&L) -> &str,
        at: ChainPosition,
    ) -> Result<(&mut L, &Capabilities, ChainPosition, Option<&PartId>), NotReady> {
        let Self {
            upper,
            lower,
            fit,
            upper_parts,
            provided_on_run,
            ..
        } = self;
        let boundary = fit
            .as_mut()
            .map_err(|mismatch| NotReady::new(mismatch.clone()))?;
        let below = at.below(*upper_parts);
        boundary.require(&required(upper));
        boundary.part = below.part(name(lower));
        *provided_on_run = false;
        Ok((lower, &boundary.required, below, boundary.part.as_ref()))
    }

    /// The halves, and their boundary on this run, given at the run's
    /// first call what the chain below provides, as `provided` reads it
    /// from the lower half.
    fn run_boundary(
        &mut self,
        provided: impl FnOnce(&L) -> Capabilities,
    ) -> (&mut U, &mut L, &Boundary) {
        let Self {
            upper,
            lower,
            fit,
            provided_on_run,
            ..
        } = self;
        let boundary = fit.as_mut().expect(READY_FIRST);
        if !*provided_on_run {
            boundary.provide(&provided(lower));
            *provided_on_run = true;
        }
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

    /// The adapter's. Not asked at run time: the boundary crossing the
    /// part above calls answers what is provided across it.
    fn provides(&self) -> &Capabilities {
        self.adapter.provides()
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
