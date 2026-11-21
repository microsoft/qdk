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
/// Each join checks that what the adapter requires fits what the part
/// below provides, and binds them where they fit ([`Adapter::bind`]); a
/// boundary that does not fit is reported, with every other fault of the
/// chain, by the [`on`](crate::DriverJoins::on) that makes the runner.
pub trait AdapterJoins: Adapter + Sized {
    /// This adapter calling `below`, as one adapter: it provides what this
    /// adapter provides, and requires what `below` requires, each with
    /// what the other forwards, as [`Capabilities`](crate::Capabilities)
    /// says.
    fn through<L: Adapter>(self, below: L) -> Composed<Self, L> {
        Composed::adapter_through(self, below)
    }

    /// This adapter calling `executor`, as one executor: it provides what
    /// this adapter provides, with what `executor` provides that this
    /// adapter forwards.
    fn on<E: Executor>(self, executor: E) -> Composed<Self, E> {
        Composed::adapter_on(self, executor)
    }
}

impl<A: Adapter> AdapterJoins for A {}
