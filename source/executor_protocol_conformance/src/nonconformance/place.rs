// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Place`]: where a check found a fault, as a report names it.

use std::fmt;

use executor_protocol::{ReadoutSlot, ShotId};

/// Where a check found a fault. Indices count from 0, runs from 1, as the
/// check's documentation numbers them: an `execute` among the part's (or,
/// for a driver, among the driver's calls below); a batch among its
/// `execute`'s; a row among its batch's shots; a call below among those
/// the part made in one `execute`. A fault in one run of several names
/// that run in its text (`alone`, `after a failed run`); otherwise it
/// holds in the check's only run, or in every run.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Place {
    /// A batch the part answered.
    Batch {
        /// Its `execute`.
        execute: usize,
        /// Its index in that `execute`.
        batch: usize,
    },
    /// A shot's row of a batch the part answered.
    Row {
        /// Its `execute`.
        execute: usize,
        /// Its batch.
        batch: usize,
        /// Its position in the batch.
        row: usize,
        /// Its shot.
        shot: ShotId,
    },
    /// A readout slot of a shot's row.
    Slot {
        /// Its `execute`.
        execute: usize,
        /// Its batch.
        batch: usize,
        /// Its row's position in the batch.
        row: usize,
        /// Its row's shot.
        shot: ShotId,
        /// The slot.
        slot: ReadoutSlot,
    },
    /// A shot, over a whole run.
    Shot {
        /// The shot.
        shot: ShotId,
    },
    /// A batch the part sent below.
    BatchBelow {
        /// The part's `execute` it was sent in.
        execute: usize,
        /// Its call below, among those of that `execute`.
        call: usize,
        /// Its index in that call.
        batch: usize,
    },
    /// An output the driver gave.
    Output {
        /// Its index among the run's outputs.
        index: usize,
        /// The shot at that index in the run's shots.
        shot: ShotId,
    },
    /// The part once its `ready` answered `Ok`, at the start of a run.
    Ready,
    /// The part at an `execute`.
    Execute {
        /// The `execute`.
        execute: usize,
    },
    /// A whole run of the check.
    Run {
        /// The run, as the check's documentation numbers it, from 1.
        run: usize,
    },
    /// A call to the part's `end_shots`, from the kit's driver or the
    /// runner.
    EndShots {
        /// The part's latest `execute` before it, if any.
        after: Option<usize>,
    },
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Place::Batch { execute, batch } => write!(f, "execute {execute}, batch {batch}"),
            Place::Row {
                execute,
                batch,
                row,
                shot,
            } => write!(
                f,
                "execute {execute}, batch {batch}, row {row} (shot {shot})"
            ),
            Place::Slot {
                execute,
                batch,
                row,
                shot,
                slot,
            } => write!(
                f,
                "execute {execute}, batch {batch}, row {row} (shot {shot}), slot {}",
                slot.get()
            ),
            Place::Shot { shot } => write!(f, "shot {shot}"),
            Place::BatchBelow {
                execute,
                call,
                batch,
            } => write!(f, "execute {execute}, call {call} below, batch {batch}"),
            Place::Output { index, shot } => write!(f, "output {index} (shot {shot})"),
            Place::Ready => f.write_str("once ready"),
            Place::Execute { execute } => write!(f, "execute {execute}"),
            Place::Run { run } => write!(f, "run {run}"),
            Place::EndShots {
                after: Some(execute),
            } => {
                write!(f, "`end_shots` after execute {execute}")
            }
            Place::EndShots { after: None } => f.write_str("`end_shots` before any execute"),
        }
    }
}
