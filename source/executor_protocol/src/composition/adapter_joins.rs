// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The joins an adapter heads ([`AdapterJoins`]): through an adapter,
//! which gives an adapter, and on an executor, which gives an executor.

use super::composed::Composed;
use super::fit;
use crate::{Adapter, Executor};

/// The joins of an adapter with the part below it:
/// [`through`](Self::through) an adapter, which gives an adapter, and
/// [`on`](Self::on) an executor, which gives an executor. Every adapter
/// has them.
///
/// Each join checks that the instructions the adapter sends fit those the
/// part below accepts, and binds them where they fit
/// ([`Adapter::bind`]); a boundary that does not fit is reported, with
/// every other fault of the chain, by the
/// [`on`](crate::DriverJoins::on) that makes the runner.
pub trait AdapterJoins: Adapter + Sized {
    /// This adapter calling `below`, as one adapter: it accepts what this
    /// adapter accepts, and sends what `below` sends.
    fn through<L: Adapter>(mut self, below: L) -> Composed<Self, L> {
        let fit_faults = fit(self.sends(), below.accepts())
            .map(|map| self.bind(map))
            .err();
        Composed::new(self, below, fit_faults)
    }

    /// This adapter calling `executor`, as one executor: it accepts what
    /// this adapter accepts, and offers what this adapter offers above the
    /// executor's offer.
    fn on<E: Executor>(mut self, executor: E) -> Composed<Self, E> {
        let fit_faults = fit(self.sends(), executor.accepts())
            .map(|below| self.bind(below))
            .err();
        Composed::new(self, executor, fit_faults)
    }
}

impl<A: Adapter> AdapterJoins for A {}
