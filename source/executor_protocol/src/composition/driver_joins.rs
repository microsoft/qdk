// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The joins a driver heads ([`DriverJoins`]): through an adapter, which
//! gives a driver, and on an executor, which checks the whole chain and
//! gives a runner.

use super::composed::Composed;
use super::{CompositionFault, CompositionFaults, DeclaredParts};
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
    /// joins: what each part requires against what the part below it
    /// provides, and each resource and feature the part below does not
    /// require again, which it forwards, against each part further below,
    /// until a part requires it again. On any fault, every fault of the
    /// chain, top to bottom. Otherwise the runner, which bounds each run by
    /// what the parts below the driver provide together, such as their
    /// fewest shots, so that an adapter may lower a bound but never raise
    /// it. What each part requires and provides is read once.
    ///
    /// # Errors
    ///
    /// [`CompositionFaults`] when a part's requirement is not provided.
    fn on<E: Executor>(mut self, executor: E) -> Result<Runner<Self, E>, CompositionFaults> {
        let fit = self.requires().fits(executor.provides());
        let (faults, driver_part, executor_part, provision) = {
            let mut parts = DeclaredParts::new();
            self.declare(&mut parts);
            let executor_at = parts.len();
            executor.declare(&mut parts);
            let mut faults = parts.faults();
            if faults.is_empty()
                && let Err(mismatch) = &fit
            {
                // The whole chain's fit holds what a part-by-part check
                // reads: a fault it alone finds is still reported.
                faults.push(CompositionFault {
                    requirer: parts.id(0),
                    provider: parts.id(executor_at),
                    mismatch: mismatch.clone(),
                });
            }
            (
                faults,
                parts.id(0),
                parts.id(executor_at),
                parts.provision(),
            )
        };
        let mut boundary = match fit {
            Ok(boundary) if faults.is_empty() => boundary,
            _ => return Err(CompositionFaults::new(faults)),
        };
        self.bind(boundary.opcode_map().clone());
        boundary.part = Some(executor_part);
        let provision = provision.expect("a chain has a part below its driver");
        Ok(Runner::new(
            (self, driver_part),
            executor,
            boundary,
            provision,
        ))
    }
}

impl<D: Driver> DriverJoins for D {}
