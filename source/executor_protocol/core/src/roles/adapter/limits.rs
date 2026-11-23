// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The limits an adapter may state, [`AdapterLimits`].

use crate::{Adapter, Capabilities, InstructionSet, Limited, NotReady};

/// The limits of an adapter that has them: what it provides to the part
/// above, what it requires of the part below, and how it gets ready for
/// what arrives from above.
///
/// An adapter with no limits implements [`Adapter`] alone: it states
/// nothing, and passes what arrives below unchanged. One with limits
/// implements this trait too, and is joined [`limited`](Self::limited):
/// the protocol reads them, at the joins and at each run's start, only
/// through that [`Limited`] part. Every method has a default, so a part
/// implements only what it states.
pub trait AdapterLimits: Adapter {
    /// Its fixed limits on what arrives from above, such as a job queue's
    /// most shots, or what its set's capabilities say it takes: what
    /// arrives must fit them, as [`Capabilities`] says.
    ///
    /// By default its set's
    /// [`unconstrained`](InstructionSet::unconstrained): it states nothing.
    /// Stating its real limits is the writer's job: an adapter that does
    /// not lets `on` accept a chain that sends it more than it can take,
    /// which then fails as it runs.
    fn provides(&self) -> &Capabilities<Self::Set> {
        Self::Set::unconstrained()
    }

    /// What it requires of the part below, given `above`, what arrives
    /// from the part above: such as a job splitter's runs of at most 100
    /// shots, whatever run arrives, or, in its set's capabilities, what an
    /// encoding needs below for what arrives.
    ///
    /// By default `above` itself: it passes what arrives below unchanged.
    /// Stating what it changes is the writer's job: an adapter that needs
    /// more below, and does not say so, lets `on` accept a chain whose
    /// executor cannot hold it, which then fails as it runs.
    ///
    /// It is deterministic, and monotone: given an `above` that requires
    /// no more than another, as [`Capabilities`] defines it, it requires
    /// no more than given the other. The protocol does not check either,
    /// and relies on both: `on` checks a chain with nothing arriving at
    /// its top, so a part short then is short on every run; and a joined
    /// part calls `requires` again at `ready`, for what its run's start
    /// checked (`docs/capabilities.md`, The rule).
    fn requires(&self, above: &Capabilities<Self::Set>) -> Capabilities<Self::Set> {
        above.clone()
    }

    /// As [`ExecutorLimits::ready_for`](crate::ExecutorLimits::ready_for),
    /// for `above`, what arrives from the part above. By default
    /// [`ready`](Adapter::ready).
    fn ready_for(
        &mut self,
        above: &Capabilities<Self::Set>,
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
