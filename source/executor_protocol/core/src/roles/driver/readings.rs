// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the protocol reads of any driver, [`DriverReadings`].

use crate::part_id::ChainPosition;
use crate::roles::Sealed;
use crate::{Capabilities, Driver, NotReady, RunRequirement};

/// What the protocol reads of every driver, whether or not it states
/// anything of its own: what it requires below on a run, and getting it
/// ready for the run.
///
/// A part that wraps a driver forwards them, as its own
/// [`DriverRequirements`](crate::DriverRequirements). A driver that states
/// nothing answers [`Capabilities::from_run`] and its
/// [`ready`](Driver::ready); a [`StatesLimits`](crate::StatesLimits) one,
/// its limits; a joined one, its parts'.
pub trait DriverReadings: Driver {
    /// What it requires of the parts below on a run that requires `run`.
    fn required(&self, run: &RunRequirement) -> Capabilities<Self::Sends>;

    /// Gets it ready for `run`, what the run requires.
    fn ready_given(&mut self, run: &RunRequirement) -> impl Future<Output = Result<(), NotReady>>;
}

impl<D: Driver + ?Sized> DriverReadings for D {
    fn required(&self, run: &RunRequirement) -> Capabilities<D::Sends> {
        self.requirement(run, Sealed(()))
    }

    fn ready_given(&mut self, run: &RunRequirement) -> impl Future<Output = Result<(), NotReady>> {
        self.ready_at(run, ChainPosition::UNKNOWN)
    }
}
