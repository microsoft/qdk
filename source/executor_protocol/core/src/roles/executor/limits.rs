// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The limits an executor may state, [`ExecutorLimits`].

use crate::{Capabilities, Executor, InstructionSet, Limited, NotReady};

/// The limits of an executor that has them: what it provides to the part
/// above, and how it gets ready for what arrives from it.
///
/// An executor with no limits implements [`Executor`] alone and states
/// nothing. One with limits implements this trait too, and is joined
/// [`limited`](Self::limited): the protocol reads them, at the joins and
/// at each run's start, only through that [`Limited`] part. Every method
/// has a default, so a part implements only what it states.
pub trait ExecutorLimits: Executor {
    /// Its fixed limits on what arrives from above, such as its most shots
    /// in a run, whether a run may fix its seed, or what its set's
    /// capabilities say it holds: what arrives must fit them, as
    /// [`Capabilities`] says.
    ///
    /// By default its set's
    /// [`unconstrained`](InstructionSet::unconstrained): it states nothing,
    /// for a part that implements this trait only to get ready for what
    /// arrives ([`ready_for`](Self::ready_for)). Stating its real limits is
    /// the writer's job: an executor that does not lets `on` accept a chain
    /// it cannot run, which then fails as it runs.
    fn provides(&self) -> &Capabilities<Self::Accepts> {
        Self::Accepts::unconstrained()
    }

    /// As [`Executor::ready`], for `above`, what arrives from the part
    /// above: its [`Resources`](crate::Resources), such as the most shots
    /// of a run, its [`Features`](crate::Features), such as whether a shot
    /// is run again alone, and its set's capabilities; such as when its
    /// resources are not available. By default [`ready`](Executor::ready).
    /// `above` follows from the run's shots and whether its seed is fixed,
    /// so work that depends only on it may be done once and kept for the
    /// runs that give the same.
    fn ready_for(
        &mut self,
        above: &Capabilities<Self::Accepts>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        let _ = above;
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
