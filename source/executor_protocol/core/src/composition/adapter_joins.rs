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
/// Each join checks that the instructions the adapter sends fit those the
/// part below accepts, and binds them where they fit ([`Adapter::bind`]);
/// a boundary that does not fit is reported, with every other fault of
/// the chain, by the [`on`](crate::DriverJoins::on) that makes the runner.
pub trait AdapterJoins: Adapter + Sized {
    /// This adapter calling `below`, as one adapter: it accepts and
    /// provides what this adapter accepts and provides, sends what `below`
    /// sends, and requires what `below` requires given what this adapter
    /// requires. What arrives at `below` is checked against what it
    /// provides when the joined adapter gets ready, or, seen from the
    /// chain, by the walk of [`on`](crate::DriverJoins::on).
    fn through<L: Adapter>(self, below: L) -> Composed<Self, L> {
        Composed::adapter_through(self, below)
    }

    /// This adapter calling `executor`, as one executor: it accepts and
    /// provides what this adapter accepts and provides. What arrives at
    /// `executor` is checked against what it provides when the joined
    /// executor gets ready, or, seen from the chain, by the walk of
    /// [`on`](crate::DriverJoins::on).
    fn on<E: Executor>(self, executor: E) -> Composed<Self, E> {
        Composed::adapter_on(self, executor)
    }
}

impl<A: Adapter> AdapterJoins for A {}
