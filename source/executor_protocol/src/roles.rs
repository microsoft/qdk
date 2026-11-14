// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The three roles a part takes in a chain, each a trait an implementer
//! writes: [`Drive`], [`Adapt`] and [`Execute`]; and the errors their run-time
//! methods share: [`RunRefusal`] and [`Failure`].
//!
//! A chain is a driver, any number of adapters, and an executor. The driver
//! sends batches of instructions down; each adapter answers each batch it
//! receives by sending batches of its own to the part below; the executor
//! runs them. Users compose parts with `>>` into a runner, which checks at
//! each join that the parts fit, and once whole that the executor serves
//! the driver's needs, so that a part writes only its own logic.
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
pub use drive::{Drive, DriveError};
pub use execute::{Execute, ExecuteError, ExecuteRefusal};

/// Why a run was refused, by the runner or by a part's `ready`: nothing of
/// it ran, so it can be corrected and sent again.
#[derive(Debug)]
#[non_exhaustive]
pub enum RunRefusal {
    /// The run has more shots than the executor's offer allows
    /// ([`Capabilities::max_shots`](crate::Capabilities::max_shots)).
    TooManyShots {
        /// The run's shots.
        asked: u32,
        /// The most the chain runs at once.
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
                write!(f, "{asked} shots asked, and the chain runs at most {most}")
            }
            RunRefusal::NotSeedable => f.write_str("a fixed seed, and the chain is not seedable"),
            RunRefusal::Unavailable(why) => write!(f, "a part cannot take a run now: {why}"),
        }
    }
}

impl Error for RunRefusal {}

/// Why a shot's work broke once its run had started.
#[derive(Debug)]
#[non_exhaustive]
pub enum Failure {
    /// The part's own error, such as the program's trap or an engine's
    /// fault.
    Part(Box<dyn Error + Send + Sync>),
    /// A part panicked, with its message when it had one.
    Panicked(Option<String>),
    /// An `execute` was refused once the run had started.
    Refused(ExecuteRefusal),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Part(error) => write!(f, "{error}"),
            Failure::Panicked(Some(message)) => write!(f, "a part panicked: {message}"),
            Failure::Panicked(None) => f.write_str("a part panicked"),
            Failure::Refused(refusal) => write!(f, "an execute was refused: {refusal}"),
        }
    }
}

impl Error for Failure {}
