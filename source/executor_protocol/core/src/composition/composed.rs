// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Two parts as one: `Composed`, what a join builds, in the role its
//! position gives it.
//!
//! Each of its methods answers from the half that faces the same way: what
//! it accepts and provides from the upper half; what it sends from the
//! lower half; what it requires given what arrives, from the lower half
//! given what the upper half requires. At run time, the upper part calls
//! the lower one, with the rest of the chain below it, as one executor:
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
//! receiver's instructions, what arrives at it and the readout slots the
//! batch states. It names a panic below it by the part that receives the
//! calls, at its position and by the name the runner's `on` read, when
//! the run's `ready` gave it one.
//!
//! What crosses the boundary is the [`Boundary`] the join's fit gave,
//! with, on each run, what arrives at the lower half, which `ready` gets
//! from what the upper half requires.
//!
//! A `Composed` keeps its role's contract through its public methods
//! alone, when the chain's check does not see into it, such as inside a
//! part of the user's that forwards only the role's public methods: a
//! panic or a refusal below it then passes up unnamed, and the boundary
//! above names the part at its position. Such a part states nothing, and
//! the chain's checks see no further, so a `Composed` readied through its
//! public `ready` refuses the run when its halves' instruction sets do not
//! fit, rather than running instructions its upper half was never bound
//! for, or when what arrives at its lower half from an upper half given
//! nothing does not fit what that half provides, with every mismatch of
//! that boundary, in one `BoundaryMismatch`; else it readies each half
//! through its public `ready`. A chain of parts stating no limits so gives
//! the same outputs, and a part stating limits refuses the run, naming
//! itself ([`StatesLimits`](crate::StatesLimits)), rather than run with
//! its limits unread.

mod adapter;
mod driver;
mod executor;

use super::BoundaryMismatch;
use super::boundary::{Boundary, READY_FIRST};
use super::part_panics::end_both;
use crate::PartId;
use crate::part_id::ChainPosition;
use crate::roles::Sealed;
use crate::{
    Batch, BatchReadouts, Binding, Capabilities, ExecuteError, Executor, InstructionSet, NotReady,
    Seed, ShotSet, TranslatingAdapter,
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
/// `S` is the instruction set that crosses between the halves, which its
/// boundary checks batches against.
pub struct Composed<U, L, S: InstructionSet> {
    upper: U,
    lower: L,
    /// The boundary between the halves, as the join's fit gave it, with
    /// what arrives at the lower half on each run; or, when the upper
    /// half's set does not fit the lower half's, what each run's refusal
    /// at `ready` is built from.
    fit: Result<Boundary<S>, UnfitHalves<S>>,
    /// The parts the upper half holds, as it counts them (its hidden
    /// `count_parts`, which asks no part what it requires): how far below
    /// this part's position the lower half's is.
    upper_parts: usize,
}

/// What [`Composed::start_run`] gives: the lower half, what arrives at it,
/// its position, and the part a refusal from it names.
type LowerHalfRun<'c, 'p, L, S> = (
    &'c mut L,
    &'c Capabilities<S>,
    ChainPosition<'p>,
    Option<&'c PartId>,
);

impl<U, L, S: InstructionSet> Composed<U, L, S> {
    /// `upper` calling `lower`, where they meet as `fit` says; the upper
    /// half holds `upper_parts` parts.
    fn new(
        (upper, upper_parts): (U, usize),
        lower: L,
        fit: Result<Boundary<S>, UnfitHalves<S>>,
    ) -> Self {
        Self {
            upper,
            lower,
            fit,
            upper_parts,
        }
    }

    /// Starts a run, at `ready`, of this part at `at`: refuses it if the
    /// halves' sets do not fit, or if what arrives at the lower
    /// half on this run, `required` of the upper half, does not fit what
    /// the lower half provides, with every mismatch at their boundary.
    /// Gives the lower half, what arrives at it, its position, and the
    /// part a refusal from it names, the lower half as `on` named it, if
    /// that position is known.
    fn start_run<'p>(
        &mut self,
        required: impl FnOnce(&U) -> Capabilities<S>,
        at: ChainPosition<'p>,
    ) -> Result<LowerHalfRun<'_, 'p, L, S>, NotReady> {
        let Self {
            upper,
            lower,
            fit,
            upper_parts,
        } = self;
        let boundary = match fit {
            Ok(boundary) => boundary,
            Err(unfit) => return Err(unfit.refusal(&required(upper))),
        };
        boundary.require(required(upper));
        (boundary.required.fits_limits(boundary.provided.as_ref())).map_err(|capabilities| {
            NotReady::new(BoundaryMismatch {
                capabilities,
                ..BoundaryMismatch::default()
            })
        })?;
        let below = at.below(*upper_parts);
        boundary.part = below.part().cloned();
        Ok((lower, &boundary.required, below, boundary.part.as_ref()))
    }

    /// The halves, and their boundary on this run.
    fn halves(&mut self) -> (&mut U, &mut L, &Boundary<S>) {
        let Ok(boundary) = &self.fit else {
            panic!("{READY_FIRST}")
        };
        (&mut self.upper, &mut self.lower, boundary)
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

impl<A: TranslatingAdapter, B: Executor<Accepts = A::Sends>> Executor for ChainBelow<'_, A, B> {
    type Accepts = A::Accepts;

    /// The adapter's, which receives its calls.
    fn name(&self) -> &str {
        self.adapter.name()
    }

    /// The adapter's. Not asked at run time: the boundary crossing the
    /// part above calls answers what is accepted across it.
    fn accepts(&self) -> &A::Accepts {
        self.adapter.accepts()
    }

    /// The adapter's. Not asked at run time: the boundary crossing the
    /// part above calls answers what is provided across it.
    fn provision(&self, sealed: Sealed) -> Option<&Capabilities<A::Accepts>> {
        let _ = sealed;
        self.adapter.provides()
    }

    /// The adapter's `adapt`: it keeps [`Executor::execute`]'s rule, doing
    /// its work only once polled, when the adapter's `adapt` does, and
    /// either way it is reached only through a boundary crossing, which
    /// calls it only once its own future is polled.
    fn execute(
        &mut self,
        batches: &[Batch<'_, A::Accepts>],
        seed: Seed,
        readouts: &mut [BatchReadouts<A::Accepts>],
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

/// Where a part sending `sends` meets one that accepts `accepted` and
/// provides `provided`: the boundary and the fit's binding, for the
/// sender's `bind`; or the halves that do not fit, with what is provided.
fn boundary<S: InstructionSet>(
    sends: &S,
    accepted: &S,
    provided: Option<&Capabilities<S>>,
) -> Result<(Boundary<S>, Binding<S>), UnfitHalves<S>> {
    match Boundary::fit(sends, accepted, provided) {
        Ok(fit) => Ok(fit),
        Err(_) => Err(UnfitHalves {
            sends: sends.clone(),
            accepted: accepted.clone(),
            provided: provided.cloned(),
        }),
    }
}

/// Halves whose sets do not fit, as the join read them: the set the upper
/// half sends, the set the lower half accepts, and what the lower half
/// provides (`None`, no limit), against which each run checks what arrives
/// at the lower half, so that a run's refusal holds every mismatch at their
/// boundary.
struct UnfitHalves<S: InstructionSet> {
    sends: S,
    accepted: S,
    provided: Option<Capabilities<S>>,
}

impl<S: InstructionSet> UnfitHalves<S> {
    /// The refusal of a run on which `required` arrives at the lower half.
    /// The sets' mismatch is found again, as the join found it, since a
    /// refusal owns its mismatch.
    #[cold]
    fn refusal(&self, required: &Capabilities<S>) -> NotReady {
        NotReady::new(BoundaryMismatch {
            instructions: (self.sends.fits(&self.accepted).err())
                .map(|mismatch| Box::new(mismatch) as Box<dyn std::error::Error + Send + Sync>),
            capabilities: (required.fits_limits(self.provided.as_ref()))
                .err()
                .unwrap_or_default(),
        })
    }
}
