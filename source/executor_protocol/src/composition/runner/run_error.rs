// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a run returns when it gives no outputs: [`RunError`], with
//! [`RunRefusal`] when nothing of it ran.

use std::error::Error;
use std::fmt;

use crate::{CapabilitiesMismatch, DriveError, NotReady, PartId};

/// What a run returns to the user when it gives no outputs: it was refused,
/// and nothing of it ran, or a shot's work broke.
///
/// The errors pass up as [`ExecuteError`](crate::ExecuteError), then
/// [`DriveError`], then `RunError`.
#[derive(Debug)]
#[non_exhaustive]
pub enum RunError {
    /// Nothing ran: the run can be corrected and sent again.
    Refused(RunRefusal),
    /// The shot the failure names ([`DriveError::shot`]), and why the run
    /// broke.
    Failed(DriveError),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Refused(refusal) => write!(f, "run refused: {refusal}"),
            RunError::Failed(failure) => write!(f, "run failed: {failure}"),
        }
    }
}

impl Error for RunError {}

/// Why a run was refused, by the runner or by a part's `ready`: nothing of
/// it ran, so it can be corrected and sent again.
#[derive(Debug)]
#[non_exhaustive]
pub enum RunRefusal {
    /// The run requires what the parts below the driver do not provide
    /// together: more shots than the fewest of those down to the first
    /// adapter that requires shots again, which serves them itself
    /// ([`Resources::with_shots`](crate::Resources::with_shots)), or a
    /// fixed seed where seeded runs are not provided
    /// ([`Features::with_seeded_runs`](crate::Features::with_seeded_runs)).
    /// A joined driver held by a part of the user's, whose adapters the
    /// runner cannot see, fails such a run instead
    /// ([`Failure::NotProvided`](crate::Failure::NotProvided)).
    NotProvided(CapabilitiesMismatch),
    /// A part's `ready` said it cannot take the run now.
    PartNotReady {
        /// The part that refused: the first asked to, bottom up.
        part: PartId,
        /// Why it cannot.
        not_ready: NotReady,
    },
}

impl RunRefusal {
    /// The refusal of `not_ready`, answered by the chain whose top part is
    /// `top`: it names the part a joined part in that chain named, or
    /// `top`.
    pub(crate) fn part_not_ready(mut not_ready: NotReady, top: &PartId) -> Self {
        let part = not_ready.take_part().unwrap_or_else(|| top.clone());
        Self::PartNotReady { part, not_ready }
    }
}

impl fmt::Display for RunRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunRefusal::NotProvided(mismatch) => {
                write!(
                    f,
                    "the run requires what the chain does not provide: {mismatch}"
                )
            }
            RunRefusal::PartNotReady { part, not_ready } => write!(f, "{part} is {not_ready}"),
        }
    }
}

impl Error for RunRefusal {}
