// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a driver may state of its own of what it requires below,
//! [`DriverLimits`].

use crate::{Capabilities, Driver, Limited, NotReady, RunRequirement};

/// What a driver that states anything of its own requires of the parts
/// below, and how it gets ready for what the run requires.
///
/// A driver that states nothing implements [`Driver`] alone: it passes
/// the run below as it arrives. One that states more implements this
/// trait too, and is joined [`limited`](Self::limited): the protocol reads
/// it, at the joins and at each run's start, only through that [`Limited`]
/// part. Every method has a default, so a part implements only what it
/// states.
pub trait DriverLimits: Driver {
    /// What it requires of the parts below on a run that requires `run`:
    /// the run's shots, and seeded runs when its seed is fixed, which the
    /// runner states, with what its shots use of its set. The part below
    /// must provide it, and it passes down the chain, each part below
    /// requiring of the next what it requires given what arrives, as
    /// [`Capabilities`] says.
    ///
    /// By default `run`, with nothing of its set
    /// ([`Capabilities::from_run`]): the driver passes the run below and
    /// states nothing of its own. Stating what its shots use, in its set's
    /// capabilities ([`Capabilities::set_capabilities`]), is the writer's job:
    /// a driver that does not lets `on` accept a chain whose executor
    /// cannot hold its shots, which then fails as it runs. A driver that
    /// handles part of a run itself, such as one that splits each run into
    /// runs of the shots the parts below take, states what it passes below
    /// instead.
    ///
    /// It is deterministic, and monotone: given a `run` that requires no
    /// more than another, as [`RunRequirement`] defines it, it requires no
    /// more than given the other. The protocol does not check either, and
    /// relies on both: `on` checks a chain with nothing arriving at its
    /// top, so a part short then is short on every run; and a joined part
    /// calls `requires` again at `ready`, for what its run's start checked
    /// (`docs/capabilities.md`, The rule).
    fn requires(&self, run: &RunRequirement) -> Capabilities<Self::Sends> {
        Capabilities::from_run(run)
    }

    /// As [`Driver::ready`], for `run`, what the run requires, as
    /// [`requires`](Self::requires) gets it. By default
    /// [`ready`](Driver::ready).
    fn ready_for(&mut self, run: &RunRequirement) -> impl Future<Output = Result<(), NotReady>> {
        let _ = run;
        self.ready()
    }

    /// This part, as the part the protocol reads these limits of.
    fn limited(self) -> Limited<Self>
    where
        Self: Sized,
    {
        Limited { part: self }
    }
}
