// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The three roles a part takes in a chain, each a trait an implementer
//! writes: [`Drive`], [`Adapt`] and [`Execute`]; and the errors their run-time
//! methods share: [`RunRefusal`] and [`Failure`].
//!
//! A chain is a driver, any number of adapters, and an executor. The driver
//! sends batches of instructions down; each adapter answers each batch it
//! receives by sending batches of its own to the part below; the executor
//! runs them. Users compose parts with `>>`, which checks at each join that
//! the parts fit and, once the chain is whole, that each part's offer
//! serves the driver's needs as they reach it, and makes a runner; so a
//! part writes only its own logic.
//!
//! Each error holds only what can happen where it is returned: an `execute`
//! is refused or fails ([`ExecuteError`](crate::ExecuteError)), a `drive`
//! only fails ([`DriveError`](crate::DriveError)), since a refusal from below
//! once a run started is a failure of the run, and a run is refused before
//! anything runs or fails ([`RunError`](crate::RunError)).

mod adapt;
mod drive;
mod execute;

use std::error::Error;
use std::fmt;

pub use adapt::Adapt;
pub(crate) use drive::FailedRun;
pub use drive::{Drive, DriveError};
pub use execute::{Execute, ExecuteError, ExecuteRefusal};

/// Why a run was refused, by the runner or by a part's `ready`: nothing of
/// it ran, so it can be corrected and sent again.
#[derive(Debug)]
#[non_exhaustive]
pub enum RunRefusal {
    /// The run has more shots than the offer that reaches the driver allows
    /// ([`Capabilities::max_shots`](crate::Capabilities::max_shots)).
    TooManyShots {
        /// The run's shots.
        asked: u32,
        /// The most shots a run may have, by the offer that reaches the
        /// driver.
        most: u32,
    },
    /// The run fixes its seed, and the chain is not seedable
    /// ([`Capabilities::seedable`](crate::Capabilities::seedable)).
    NotSeedable,
    /// A part cannot take a run now, such as a device that is busy, and
    /// why.
    Unavailable(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for RunRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunRefusal::TooManyShots { asked, most } => {
                write!(f, "{asked} shots asked, and a run may have at most {most}")
            }
            RunRefusal::NotSeedable => f.write_str("a fixed seed, and the chain is not seedable"),
            RunRefusal::Unavailable(why) => write!(f, "a part cannot take a run now: {why}"),
        }
    }
}

impl Error for RunRefusal {}

/// Why a shot's work broke once its run had started.
///
/// The part that fails names the shot. A failure the protocol makes is of
/// a whole batch, or a whole run, so it names the first shot of what
/// failed: of the batch, for a refusal or an answer's rows or slots; of the
/// first batch, for an answer of readouts not one per batch, or a panic in
/// an `execute`; of the run, for the driver's panic.
#[derive(Debug)]
#[non_exhaustive]
pub enum Failure {
    /// The part's own error, such as the program's trap or an engine's
    /// fault; or a fault the protocol found in a part's answer, a
    /// [`ShapeFault`](crate::ShapeFault). A caller tells them apart by
    /// downcasting: `error.downcast_ref::<ShapeFault>()`.
    Part(Box<dyn Error + Send + Sync>),
    /// A part panicked. Only where a panic unwinds: on wasm32, it aborts,
    /// and nothing is caught.
    Panicked {
        /// The name of the part whose call panicked
        /// ([`Execute::name`](crate::Execute::name)); a composed part is
        /// named as its upper part, since each part below it is called
        /// through a check of its own, which names it.
        part: String,
        /// Its message, when the panic's payload is text (a `&str` or a
        /// `String`).
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
            } => write!(f, "`{part}` panicked: {message}"),
            Failure::Panicked {
                part,
                message: None,
            } => write!(f, "`{part}` panicked"),
            Failure::Refused(refusal) => write!(f, "an execute was refused: {refusal}"),
        }
    }
}

impl Error for Failure {}
