// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why a shot's work broke once its run had started: [`Failure`], which an
//! `execute` and a `drive` both fail with.

use std::error::Error;
use std::fmt;

use crate::{ExecuteRefusal, PartId};

/// Why a shot's work broke once its run had started.
///
/// The part that fails names the shot. A failure the protocol makes is of
/// a whole batch, or a whole run, so it names the first shot of what
/// failed: of the batch, for a refusal; of the first batch, for a panic in
/// an `execute`, which includes a panic in an `end_shots` an adapter calls
/// in `adapt`; of the run, for the driver's panic, which includes a panic
/// in an `end_shots` the driver calls on the chain below.
#[derive(Debug)]
#[non_exhaustive]
pub enum Failure {
    /// The part's own error, such as the program's trap or an engine's
    /// fault.
    Part(Box<dyn Error + Send + Sync>),
    /// A part panicked. Only where a panic unwinds: on wasm32, it aborts,
    /// and nothing is caught.
    ///
    /// The part above gets this failure as it gets any other from below,
    /// and may go on, and the next run calls every part as before, so a
    /// part must tolerate being called again after it panicked, as after
    /// an error.
    Panicked {
        /// The part whose call panicked: the driver, for a panic in its
        /// `drive`; else the part whose `execute` or `adapt` panicked. A
        /// panic in the destructor of the future such a call returned is
        /// that part's too when the protocol drops the future, once it is
        /// ready or has panicked. Two panics unwind through the call of a
        /// part above instead, and so name that part: one in the destructor
        /// of a future that part stopped waiting for and dropped
        /// unfinished, and one in an `end_shots` it calls on the chain
        /// below (the driver, for an end it calls as each shot ends; an
        /// adapter, for an end it calls in `adapt`).
        part: PartId,
        /// Its message, when the panic's payload is text (a `&str` or a
        /// `String`): its poll's, when the future's destructor panics
        /// after its poll did, the destructor's panic being dropped.
        message: Option<String>,
    },
    /// An `execute` was refused once the run had started.
    Refused(ExecuteRefusal),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Part(error) => write!(f, "{error}"),
            Failure::Panicked {
                part,
                message: Some(message),
            } => write!(f, "{part} panicked: {message}"),
            Failure::Panicked {
                part,
                message: None,
            } => write!(f, "{part} panicked"),
            Failure::Refused(refusal) => write!(f, "an execute was refused: {refusal}"),
        }
    }
}

impl Error for Failure {}
