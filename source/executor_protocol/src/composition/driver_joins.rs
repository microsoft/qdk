// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The joins a driver heads ([`DriverJoins`]): through an adapter, which
//! gives a driver, and on an executor, which checks the whole chain and
//! gives a runner.

use super::composed::Composed;
use super::{CompositionFaults, DeclaredParts};
use crate::{Adapter, BoundaryCompatible, Driver, Executor, Runner};

/// The joins of a driver with the part below it: [`through`](Self::through)
/// an adapter, which gives a driver, and [`on`](Self::on) an executor,
/// which gives the [`Runner`] of the whole chain. Every driver has them.
///
/// Each join checks that what the driver requires fits what the part below
/// provides, and binds them where they fit ([`Driver::bind`]); a boundary
/// that does not fit is reported, with every other fault of the chain, by
/// the `on` that makes the runner.
pub trait DriverJoins: Driver + Sized {
    /// This driver calling `adapter`, as one driver: it requires what
    /// `adapter` requires, and what this driver requires that `adapter`
    /// does not state.
    fn through<A: Adapter>(self, adapter: A) -> Composed<Self, A> {
        Composed::driver_through(self, adapter)
    }

    /// The runner of this driver calling `executor`, or every fault of the
    /// chain.
    ///
    /// Checks the whole chain, part by part, whatever the brackets of the
    /// joins: what each part requires against what the part below it provides,
    /// and each resource and feature the part below does not require again,
    /// which it forwards, against each part further below, until a part
    /// requires it again. On any fault, every fault of the chain, top to
    /// bottom. Otherwise the runner, which bounds each run by what the parts
    /// below the driver provide together, such as the fewest shots of those
    /// down to the first adapter that requires shots again, so that an adapter
    /// may lower a bound, and raise it only by requiring it again, serving it
    /// itself. What each part requires and provides is read at the joins and at
    /// this `on`, for the checks and the runner's boundary; a joined part reads
    /// its halves' resources and features again on each run, for the boundary
    /// between them, whose instructions stay those of its join.
    ///
    /// # Errors
    ///
    /// [`CompositionFaults`] when a part's requirement is not provided.
    fn on<E: Executor>(mut self, executor: E) -> Result<Runner<Self, E>, CompositionFaults> {
        let mut parts = DeclaredParts::new();
        self.declare(&mut parts);
        let executor_at = parts.len();
        executor.declare(&mut parts);
        let faults = parts.faults();
        if !faults.is_empty() {
            return Err(CompositionFaults { faults });
        }
        let (driver_part, executor_part) = (parts.id(0), parts.id(executor_at));
        let provision = parts.provision();
        // The whole chain's fit finds no fault the part-by-part check
        // misses: the joined driver requires its lowest part's
        // requirement, else the one above, key by key, and the joined
        // executor provides its top part's provision, the stricter of it
        // and the one below where the top part does not require it again,
        // which are what that check compares; and both check the
        // instructions between adjacent parts.
        let mut boundary = (self.requires().fits(executor.provides()))
            .expect("the chain fits where each part fits what it reaches");
        self.bind(boundary.opcode_map().clone());
        boundary.part = Some(executor_part);
        Ok(Runner {
            driver: self,
            executor,
            driver_part,
            boundary,
            provision,
        })
    }
}

impl<D: Driver> DriverJoins for D {}
