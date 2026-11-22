// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The joins an adapter heads ([`AdapterJoins`]): through an adapter,
//! which gives an adapter, and on an executor, which gives an executor.

use super::composed::Composed;
use crate::{Adapter, Executor};

/// The joins of an adapter with the part below it:
/// [`through`](Self::through) an adapter, which gives an adapter, and
/// [`on`](Self::on) an executor, which gives an executor. Every adapter
/// has them.
///
/// Each join joins only a part below that accepts the instruction set type
/// the adapter sends ([`Adapter::Sends`]), checked at compile time, and
/// checks that the set the adapter sends fits the one the part below
/// accepts, and binds the adapter where it fits ([`Adapter::bind`]);
/// a boundary that does not fit is reported, with every other fault of
/// the chain, by the [`on`](crate::DriverJoins::on) that makes the runner.
pub trait AdapterJoins: Adapter + Sized {
    /// This adapter calling `below`, as one adapter: it accepts and
    /// provides what this adapter accepts and provides, sends what `below`
    /// sends, and requires what `below` requires given what this adapter
    /// requires. What arrives at `below` is checked against what it
    /// provides when the joined adapter gets ready, or, seen from the
    /// chain, by the walk of [`on`](crate::DriverJoins::on).
    fn through<L: Adapter<Accepts = Self::Sends>>(
        self,
        below: L,
    ) -> Composed<Self, L, Self::Sends> {
        Composed::adapter_through(self, below)
    }

    /// This adapter calling `executor`, as one executor: it accepts and
    /// provides what this adapter accepts and provides. What arrives at
    /// `executor` is checked against what it provides when the joined
    /// executor gets ready, or, seen from the chain, by the walk of
    /// [`on`](crate::DriverJoins::on).
    fn on<E: Executor<Accepts = Self::Sends>>(self, executor: E) -> Composed<Self, E, Self::Sends> {
        Composed::adapter_on(self, executor)
    }
}

impl<A: Adapter> AdapterJoins for A {}
