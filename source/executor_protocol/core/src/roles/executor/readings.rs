// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the protocol reads of any executor, [`ExecutorReadings`].

use crate::part_id::ChainPosition;
use crate::roles::Sealed;
use crate::{Capabilities, Executor, NotReady};

/// What the protocol reads of every executor, whether or not it states
/// limits: what it provides, and getting it ready for what arrives.
///
/// An adapter reads them of the part below it, and a part that wraps an
/// executor forwards them, as its own
/// [`ExecutorLimits`](crate::ExecutorLimits).
/// An executor that states no limits answers its set's
/// [`unconstrained`](crate::InstructionSet::unconstrained) and its
/// [`ready`](Executor::ready); a [`Limited`](crate::Limited) one, its
/// limits; a joined one, its parts'.
pub trait ExecutorReadings: Executor {
    /// What it provides to the part above.
    fn provided(&self) -> &Capabilities<Self::Accepts>;

    /// Gets it ready for `above`, what arrives from the part above.
    fn ready_given(
        &mut self,
        above: &Capabilities<Self::Accepts>,
    ) -> impl Future<Output = Result<(), NotReady>>;
}

impl<E: Executor + ?Sized> ExecutorReadings for E {
    fn provided(&self) -> &Capabilities<E::Accepts> {
        self.provision(Sealed(()))
    }

    fn ready_given(
        &mut self,
        above: &Capabilities<E::Accepts>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        self.ready_at(above, ChainPosition::UNKNOWN)
    }
}
