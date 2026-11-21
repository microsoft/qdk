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
    /// `adapter` requires, and what of this driver's requirement reaches
    /// past `adapter`; it provides a run what this driver provides it, over
    /// what `adapter` provides.
    fn through<A: Adapter>(self, adapter: A) -> Composed<Self, A> {
        Composed::driver_through(self, adapter)
    }

    /// The runner of this driver calling `executor`, or every fault of the
    /// chain it can see: the parts inside a part of the user's, which it
    /// cannot see into, are checked on each run instead, as the crate's
    /// README says ("Composing and running a chain").
    ///
    /// Checks the chain it sees, part by part, whatever the brackets of the
    /// joins: what each part requires against what the part below it
    /// provides, and each resource and feature it requires against each
    /// part further below that states it, down to the first that serves it.
    /// On any fault, every such fault, top to bottom, with the same parts'
    /// positions and mismatches in every bracketing, and the same names
    /// where each part's name stays the same through composition, `bind`
    /// included. Otherwise the
    /// runner, which checks each run against what the driver provides a
    /// run over the parts below it, by the same rule: a run's shots reach,
    /// for instance, every part below down to the first that serves them,
    /// so that a part that limits them lowers the most a run may have, and
    /// only one that serves them raises it. What each part requires and
    /// provides is read at the joins and at this `on`, for the checks and
    /// the runner's boundary, and each part's name is read here, for the
    /// refusals and panics that name it; a joined part reads its parts'
    /// resources and features again on each run, for the boundary between
    /// its halves, whose instructions stay those of its join, and a joined
    /// driver for its check of the run.
    ///
    /// # Errors
    ///
    /// [`CompositionFaults`] when a part's requirement is not provided.
    fn on<E: Executor>(mut self, executor: E) -> Result<Runner<Self, E>, CompositionFaults> {
        let mut parts = DeclaredParts::new();
        self.declare(&mut parts);
        let provision = self.provides().over(executor.provides());
        let executor_at = parts.len();
        executor.declare(&mut parts);
        let faults = parts.faults();
        if !faults.is_empty() {
            return Err(CompositionFaults { faults });
        }
        let parts = parts.ids();
        // The whole chain's fit finds no fault the part-by-part check
        // misses: the joined driver requires, key by key, the largest
        // requirement that reaches past its lowest part, and the joined
        // executor provides the smallest provision down to the first part
        // that serves it, which are what that check compares; and both
        // check the instructions between adjacent parts.
        let mut boundary = (self.requires().fits(executor.provides()))
            .expect("the chain fits where each part fits what it reaches");
        self.bind(boundary.opcode_map().clone());
        boundary.part = Some(parts[executor_at].clone());
        Ok(Runner {
            driver: self,
            executor,
            parts,
            boundary,
            provision,
        })
    }
}

impl<D: Driver> DriverJoins for D {}
