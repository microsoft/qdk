// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a run returns when it gives no outputs: [`RunError`], with
//! [`RunRefusal`] when nothing of it ran.

use std::error::Error;
use std::fmt;

use crate::{DriveError, NotReady, PartId};

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
    /// The run has more shots than a part's offer allows
    /// ([`Offer::max_shots`](crate::Offer::max_shots)).
    TooManyShots {
        /// The run's shots, or `u32::MAX` when it has more.
        asked: u32,
        /// The most shots a run may have: the fewest any part offers.
        most: u32,
    },
    /// The run fixes its seed, and the chain is not seedable
    /// ([`Offer::seedable`](crate::Offer::seedable)).
    NotSeedable,
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
    /// `top`: it names the part a composition in that chain named, or
    /// `top`.
    pub(crate) fn part_not_ready(mut not_ready: NotReady, top: &PartId) -> Self {
        let part = not_ready.take_part().unwrap_or_else(|| top.clone());
        Self::PartNotReady { part, not_ready }
    }
}

impl fmt::Display for RunRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunRefusal::TooManyShots { asked, most } => {
                write!(f, "{asked} shots asked, and a run may have at most {most}")
            }
            RunRefusal::NotSeedable => f.write_str("a fixed seed, and the chain is not seedable"),
            RunRefusal::PartNotReady { part, not_ready } => write!(f, "{part} is {not_ready}"),
        }
    }
}

impl Error for RunRefusal {}
