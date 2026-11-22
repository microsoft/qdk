// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The driver's role, [`Driver`], and what a `drive` returns when it gives
//! no outputs: [`DriveError`].

use std::error::Error;
use std::fmt;

use crate::composition::DeclaredParts;
use crate::part_id::ChainPosition;
use crate::{
    Binding, Capabilities, ExecuteError, Executor, Failure, InstructionSet, NotReady,
    RunRequirement, Seed, ShotId, ShotSet,
};

/// Calls the part below with batches of the instructions it sends, of its
/// set [`Sends`](Self::Sends), each
/// with its shots, and the seed, and makes each shot's output from the
/// readouts the part below fills, which the driver keeps in a
/// [`Readouts`](crate::Readouts) to reuse from one call to the next.
///
/// Such as a program's interpreter, which sends each shot's instructions
/// up to its next branch on a readout, or a generator of a program too
/// long to hold, which sends it in bounded batches and reads only at the
/// end: a driver calls the part below as many times as it likes per shot.
/// It is the part at the top of a chain, and the one a runner runs shots
/// on.
///
/// # Declarations
///
/// The joins ([`DriverJoins`](crate::DriverJoins)) read them before any
/// run: [`name`](Self::name), [`sends`](Self::sends) and
/// [`requires`](Self::requires). The join to the part below hands back
/// what it made of them: [`bind`](Self::bind) gets the fit's binding.
/// A join only decides, from what the parts declare; what prepares a part
/// to run belongs to each run.
///
/// What it sends must not change once it is joined: the fit and `bind`'s
/// binding are made once, from it. What it requires is asked again at each
/// run's start, of that run, and checked down the chain.
///
/// # Run
///
/// On every run of the runner that
/// [`on`](crate::DriverJoins::on) makes, the runner checks what the run
/// requires down the chain, gets it ready ([`ready`](Self::ready)), then
/// runs the shots on it ([`drive`](Self::drive)).
pub trait Driver {
    /// What a run gives for each shot.
    type Output;

    /// The instruction set it sends: the part below must accept the same,
    /// checked at compile time, where a
    /// [`Lowered`](crate::Lowered) adapter joins two sets.
    type Sends: InstructionSet;

    /// As [`Executor::name`].
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// The instructions it sends: the part below must accept them.
    fn sends(&self) -> &Self::Sends;

    /// What it requires of the parts below on a run that requires `run`:
    /// the run's shots, and seeded runs when its seed is fixed, which the
    /// runner states, with what its shots use of its set. The part below
    /// must provide it, and it passes down the chain, each part below
    /// requiring of the next what it requires given what arrives, as
    /// [`Capabilities`] says.
    ///
    /// By default `run`, with nothing of its set
    /// ([`Capabilities::from_run`]): the driver passes the run below and
    /// states nothing of its own. Stating what its shots use, in its set's
    /// capabilities ([`Capabilities::set_capabilities`]), is the writer's job:
    /// a driver that does not lets `on` accept a chain whose executor
    /// cannot hold its shots, which then fails as it runs. A driver that
    /// handles part of a run itself, such as one that splits each run into
    /// runs of the shots the parts below take, states what it passes below
    /// instead.
    ///
    /// It is deterministic, and monotone: given a `run` that requires no
    /// more than another, as [`RunRequirement`] defines it, it requires no
    /// more than given the other. The protocol does not check either, and
    /// relies on both: `on` checks a chain with nothing arriving at its
    /// top, so a part short then is short on every run; and a joined part
    /// calls `requires` again at `ready`, for what its run's start checked
    /// (`docs/capabilities.md`, The rule).
    fn requires(&self, run: &RunRequirement) -> Capabilities<Self::Sends> {
        Capabilities::from_run(run)
    }

    /// Receives, once, the fit's binding: what its set's
    /// [`fits`](crate::BoundaryCompatible::fits) gave against the part
    /// below's, such as the receiver's encoding of each instruction it
    /// sends, which every instruction it sends must then carry. The join to
    /// the part below calls it once, after the fit and before any run; a
    /// driver rewrites its prebuilt instructions then, or keeps the binding
    /// to encode each one it sends.
    fn bind(&mut self, below: Binding<Self::Sends>);

    /// As [`Executor::ready`], asked last, for `run`, what the run
    /// requires, as [`requires`](Self::requires) gets it.
    fn ready(&mut self, run: &RunRequirement) -> impl Future<Output = Result<(), NotReady>> {
        let _ = run;
        async { Ok(()) }
    }

    /// Runs `shots` by calling `below`, the rest of the chain as one
    /// executor, until each shot ends, and gives each shot's output, in the
    /// order of `shots`. An error from below passes up with `?`, through
    /// `From<ExecuteError>`. The runner returns the outputs as given: it
    /// does not check their count or order.
    ///
    /// Each batch it sends below holds only shots of `shots`: a rule the driver
    /// keeps, which the protocol does not check. A shot outside them is not
    /// counted against the most shots a run may have
    /// ([`Resources::shots`](crate::Resources::shots)), and the
    /// runner does not end it, so a part may carry its state into a later run,
    /// which then does not repeat with its seed.
    ///
    /// It decides whether each batch it sends is checked before it runs, as
    /// [`Batch::checked`](crate::Batch::checked) says.
    ///
    /// Once `drive` returns, fails or panics, or its run's future is
    /// dropped, the protocol ends every shot of `shots` on every part below
    /// ([`Executor::end_shots`]), so a driver need not end them. It may end
    /// a shot below as soon as that shot ends, to free its state early; the
    /// protocol's second end of it does nothing. A part's panic in an end
    /// the driver calls fails the run naming the driver (and in one an
    /// adapter calls in `adapt`, naming that adapter), where the same panic
    /// in the protocol's end is raised again from the run
    /// ([`Runner::run_async`](crate::Runner::run_async)).
    ///
    /// A `drive` does its work, calling `below` included, when its future is
    /// first polled, not when it is called, as Rust's futures do by
    /// convention (an `async fn drive`'s does), and as
    /// [`Executor::execute`] asks of an `execute`: a future dropped before
    /// its first poll has then sent nothing below. It is a rule for
    /// implementers, which the protocol does not check.
    fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = Self::Sends>,
    ) -> impl Future<Output = Result<Vec<Self::Output>, DriveError>>;

    // Protocol hooks: sealed and hidden. Their parameter types, `DeclaredParts`
    // (`src/composition/`) and `ChainPosition` (`src/part_id/`), cannot be
    // named outside this crate, so no other crate can call or override them.
    // They are in the trait because Rust cannot specialise a default method:
    // the protocol's joined parts (`Composed`, in `src/composition/composed/`)
    // override them, to declare each inner part, so that a composition fault
    // names the inner part that is short; to count their inner parts; and to
    // name an inner part's refusal by its position. Correctness does not
    // depend on them: a joined part they do not reach, such as one inside a
    // part of the user's that forwards only the public methods, checks its
    // inner parts itself at `ready` (its halves' saved fit, and what arrives
    // at its lower half), and gives the same outputs; a fault is then its
    // refusal at `ready`, for the first boundary inside it found not to fit.

    /// Adds this part to `parts`, for a run that requires `run`, and gives
    /// what it requires of the part below. Only the crate overrides it, to
    /// add each part it holds, top to bottom. The
    /// [`on`](crate::DriverJoins::on) that makes a runner calls it, and
    /// the runner on each run.
    #[doc(hidden)]
    fn declare<'a>(
        &'a self,
        run: &RunRequirement,
        parts: &mut DeclaredParts<'a>,
    ) -> Capabilities<Self::Sends> {
        parts.push(Driver::name(self));
        let required = self.requires(run);
        parts.send(self.sends(), &required);
        required
    }

    /// Adds each part it holds to `parts`, top to bottom, by name alone,
    /// without asking what any requires: how a join counts the parts above
    /// the boundary it makes, before any run says what arrives. Only the
    /// crate overrides it, to add each part it holds.
    #[doc(hidden)]
    fn count_parts<'a>(&'a self, parts: &mut DeclaredParts<'a>) {
        parts.push(Driver::name(self));
    }

    /// [`ready`](Self::ready), for this part at `at` in its chain. Only
    /// the crate overrides it, to name a refusal of a part it holds by
    /// that part's position.
    #[doc(hidden)]
    fn ready_at(
        &mut self,
        run: &RunRequirement,
        at: ChainPosition<'_>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        let _ = at;
        self.ready(run)
    }
}

/// What a `drive` returns to the runner when it gives no outputs: the shot
/// the failure names, and why.
///
/// A driver can only fail, never refuse: once a run started, an `execute`'s
/// refusal is a failure of the run, [`Failure::Refused`].
#[derive(Debug)]
pub struct DriveError {
    /// The shot the failure names: the failing part's own choice, or the
    /// first shot of the batch or run that failed, for a failure the
    /// protocol makes ([`Failure`]).
    pub shot: ShotId,
    /// Why.
    pub failure: Failure,
}

/// So that a driver passes an error from below with `?`: a failure as it
/// is, a refusal as [`Failure::Refused`], at the shot it names.
impl From<ExecuteError> for DriveError {
    fn from(error: ExecuteError) -> Self {
        match error {
            ExecuteError::Refused(refusal) => Self {
                shot: refusal.shot(),
                failure: Failure::Refused(refusal),
            },
            ExecuteError::Failed { shot, failure } => Self { shot, failure },
        }
    }
}

impl fmt::Display for DriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "shot {} failed: {}", self.shot, self.failure)
    }
}

impl Error for DriveError {}
