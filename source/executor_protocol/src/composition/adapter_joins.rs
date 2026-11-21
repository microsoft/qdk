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
    /// This adapter calling `below`, as one adapter: it provides this
    /// adapter's provision over `below`'s, and requires what `below`
    /// requires with what this adapter requires that reaches past `below`,
    /// as [`Capabilities`](crate::Capabilities) says.
    fn through<L: Adapter>(self, below: L) -> Composed<Self, L> {
        Composed::adapter_through(self, below)
    }

    /// This adapter calling `executor`, as one executor: it provides this
    /// adapter's provision over `executor`'s, as
    /// [`Capabilities`](crate::Capabilities) says.
    fn on<E: Executor>(self, executor: E) -> Composed<Self, E> {
        Composed::adapter_on(self, executor)
    }
}

impl<A: Adapter> AdapterJoins for A {}
