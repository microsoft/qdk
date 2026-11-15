// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The driver's role, [`Drive`], and what a `drive` returns when it gives
//! no outputs: [`DriveError`].

use std::error::Error;
use std::fmt;

use crate::admission::by_part::ByPart;
use crate::{
    Capabilities, Execute, ExecuteError, Failure, InstructionSet, Needs, OpcodeMap, RunRefusal,
    Seed, ShotId, ShotSet,
};

/// Calls the part below with batches of the instructions it sends, each
/// with its shots, and the seed, and makes each shot's output from the
/// readouts the part below appends to a buffer it lends, which it clears
/// or reuses between calls.
///
/// Such as a program's interpreter, which sends each shot's instructions
/// up to its next branch on a readout. It is the part at the top of a
/// chain, and the one a runner runs shots on.
pub trait Drive {
    /// What a run gives for each shot.
    type Output;

    // Composition time.

    /// As [`Execute::name`].
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// The instructions it sends, which must fit the part below.
    ///
    /// Its answer must not change once the part is composed: the fit and
    /// `bind`'s map are made once, by `>>`.
    fn sends(&self) -> &InstructionSet;

    /// Takes the receiver's opcode for each instruction it sends; `>>`
    /// calls it once, after the fit.
    fn bind(&mut self, below: OpcodeMap);

    /// What its shots need of the chain below, such as their qubits, which
    /// the offer below it must meet, and, adapted by each adapter, the
    /// offer of each part further below. Its answer must not change once
    /// the driver is composed, not even by its own `provision`: the
    /// admission checks it, and each part below is provisioned for it, as
    /// read before that `provision`.
    fn needs(&self) -> Needs;

    /// Provisions this driver once its chain is admitted; it knows its own
    /// needs, so it gets none. The `>>` that makes a runner calls it once;
    /// by default it does nothing.
    fn provision(&mut self) {}

    /// Its needs, listed as this part's; only the crate overrides it, to
    /// list each part it holds.
    #[doc(hidden)]
    fn needs_by_part(&self, by_part: &mut ByPart) -> Needs {
        let needs = self.needs();
        by_part.note_needs(Drive::name(self), &needs);
        needs
    }

    /// The offer that reaches this driver from `below`, listing those of
    /// each adapter it holds; only the crate overrides it, to list them, and
    /// to keep the offer that crosses each boundary it holds. The `>>` that
    /// makes a runner calls it once, before any part is provisioned.
    #[doc(hidden)]
    fn offers_by_part(&mut self, below: Capabilities, by_part: &mut ByPart) -> Capabilities {
        let _ = by_part;
        below
    }

    /// Ends the shots of `run`, which ended, on each adapter it holds, top
    /// to bottom; by default it holds none. The runner calls it after every
    /// run whose `drive` started, then the executor's `end_shots`; only the
    /// crate overrides it.
    #[doc(hidden)]
    fn end_run(&mut self, run: EndedRun<'_>) {
        let _ = run;
    }

    // Run time.

    /// As [`Execute::ready`].
    fn ready(&mut self) -> impl Future<Output = Result<(), RunRefusal>> {
        async { Ok(()) }
    }

    /// Runs `shots` by calling `below`, the rest of the chain as one
    /// executor, until each shot ends, and gives each shot's output, in the
    /// order of `shots`. An error from below passes up with `?`, through
    /// `From<ExecuteError>`. The runner returns the outputs as given: it
    /// does not check their count or order.
    ///
    /// Each batch it sends below holds only shots of `shots`: a rule the
    /// driver keeps, which the protocol does not check. A shot outside them
    /// is not counted against the most shots a run may have
    /// ([`Capabilities::max_shots`]), and the runner does not end it, so a
    /// part may carry its state into a later run, which then does not
    /// repeat with its seed.
    ///
    /// It decides whether each batch it sends is checked before it runs, as
    /// [`Batch::checked`](crate::Batch::checked) says.
    ///
    /// Once `drive` returns, fails or panics, or its run's future is
    /// dropped, the runner ends every shot of `shots` on every part below
    /// ([`Execute::end_shots`]), so a driver need not end them. It may end
    /// a shot below as soon as that shot ends, to free its state early; the
    /// runner's second end of it does nothing. A part's panic in an end the
    /// driver calls fails the run naming the driver (and in one an adapter
    /// calls in `adapt`, naming that adapter), where the same panic in the
    /// runner's end is raised again from the run
    /// ([`Runner::run_async`](crate::Runner::run_async)).
    fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> impl Future<Output = Result<Vec<Self::Output>, DriveError>>;
}

/// A run whose `drive` started and ended, as the runner tells
/// [`Drive::end_run`]: its shots.
///
/// Code outside the crate cannot name it, because this module is private:
/// so only the crate overrides that method, and every part a driver holds
/// is told.
#[derive(Clone, Copy)]
pub struct EndedRun<'a> {
    /// The run's shots.
    pub(crate) shots: &'a ShotSet,
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
