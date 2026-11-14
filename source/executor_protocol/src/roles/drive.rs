// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The driver's role, [`Drive`], and what a `drive` returns when it gives
//! no outputs: [`DriveError`].

use std::error::Error;
use std::fmt;

use crate::admission::part_list::PartList;
use crate::{
    Capabilities, Execute, ExecuteError, Failure, InstructionSet, Needs, OpcodeMap, RunRefusal,
    Seed, ShotId, ShotSet,
};

/// Calls the part below with batches of the instructions it sends, each
/// with its shots, and the seed, and makes each shot's output from the
/// readouts it gets back.
///
/// Such as a program's interpreter, which sends each shot's instructions
/// up to its next branch on a readout. It is the part at the top of a
/// chain, and the one a runner runs shots on.
pub trait Drive {
    /// What a run gives for each shot.
    type Output;

    // Composition time.

    /// The instructions it sends, which must fit the part below.
    fn sends(&self) -> &InstructionSet;

    /// Takes the receiver's opcode for each instruction it sends; `>>`
    /// calls it once, after the fit.
    fn bind(&mut self, below: OpcodeMap);

    /// What its shots need of the chain below, such as their qubits, which,
    /// adapted by each adapter below it, the executor's offer must meet.
    fn needs(&self) -> Needs;

    /// Provisions this driver once its chain is admitted; it knows its own
    /// needs, so it gets none. The `>>` that makes a runner calls it once;
    /// by default it does nothing.
    fn provision(&mut self) {}

    /// Its needs, listed as this part's; only the crate overrides it, to
    /// list each part it holds.
    #[doc(hidden)]
    fn needs_by_part(&self, parts: &mut PartList) -> Needs {
        let needs = self.needs();
        parts.note_needs::<Self>(&needs);
        needs
    }

    /// The offer that reaches this driver from `below`, listing those of
    /// each adapter it holds; only the crate overrides it.
    #[doc(hidden)]
    fn offers_by_part(&self, below: Capabilities, parts: &mut PartList) -> Capabilities {
        let _ = parts;
        below
    }

    /// Ends the shots of `run`, which failed, on each adapter it holds, top
    /// to bottom; by default it holds none. The runner calls it, then the
    /// executor's `end_shots`; only the crate overrides it.
    #[doc(hidden)]
    fn end_failed_run(&mut self, run: FailedRun<'_>) {
        let _ = run;
    }

    // Run time.

    /// As [`Execute::ready`].
    fn ready(&mut self) -> impl Future<Output = Result<(), RunRefusal>> {
        async { Ok(()) }
    }

    /// Runs `shots` by calling `below`, the rest of the chain as one
    /// executor, until each shot ends; ends each shot below when it ends;
    /// and gives each shot's output, in the order of `shots`. An error from
    /// below passes up with `?`, through `From<ExecuteError>`.
    fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> impl Future<Output = Result<Vec<Self::Output>, DriveError>>;
}

/// A run whose `drive` failed, as the runner tells
/// [`Drive::end_failed_run`]: its shots.
///
/// Code outside the crate cannot name it, because this module is private:
/// so only the crate overrides that method, and every part a driver holds
/// is told.
#[derive(Clone, Copy)]
pub struct FailedRun<'a> {
    /// The run's shots.
    pub(crate) shots: &'a ShotSet,
}

/// What a `drive` returns to the runner when it gives no outputs: the shot
/// whose work broke, and why.
///
/// A driver can only fail, never refuse: once a run started, an `execute`'s
/// refusal is a failure of the run, [`Failure::Refused`].
#[derive(Debug)]
pub struct DriveError {
    /// The shot whose work broke.
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
