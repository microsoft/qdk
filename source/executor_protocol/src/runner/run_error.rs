// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a run returns when it gives no outputs: [`RunError`].

use std::error::Error;
use std::fmt;

use crate::{DriveError, RunRefusal};

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
