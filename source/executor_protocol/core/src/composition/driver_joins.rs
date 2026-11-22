// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The joins a driver heads ([`DriverJoins`]): through an adapter, which
//! gives a driver, and on an executor, which checks the whole chain and
//! gives a runner.

use super::composed::Composed;
use super::{CompositionFaults, DeclaredParts};
use crate::capabilities::Boundary;
use crate::{Driver, Executor, RunRequirement, Runner, TranslatingAdapter};

/// The joins of a driver with the part below it: [`through`](Self::through)
/// an adapter, which gives a driver, and [`on`](Self::on) an executor,
/// which gives the [`Runner`] of the whole chain. Every driver has them.
///
/// Each join joins only a part below that accepts the instruction set type
/// the driver sends ([`Driver::Sends`]), checked at compile time: between
/// two set types, a [`Lowered`](crate::Lowered) adapter joins them. It
/// checks that the set the driver sends fits the one the part below
/// accepts, and binds the driver where it fits ([`Driver::bind`]); a
/// boundary that does not fit is reported, with every other fault of the
/// chain, by the `on` that makes the runner.
pub trait DriverJoins: Driver + Sized {
    /// This driver calling `adapter`, as one driver: it sends what
    /// `adapter` sends, and requires what `adapter` requires given what
    /// this driver requires.
    fn through<A: TranslatingAdapter<Accepts = Self::Sends>>(
        self,
        adapter: A,
    ) -> Composed<Self, A, Self::Sends> {
        Composed::driver_through(self, adapter)
    }

    /// The runner of this driver calling `executor`, or every fault of the
    /// chain it can see: the parts inside a part of the user's, which it
    /// cannot see into, are checked when they get ready instead, as the
    /// crate's README says ("Composing and running a chain").
    ///
    /// Checks the chain it sees, part by part, whatever the brackets of the
    /// joins: the set each part sends against the one the part below
    /// accepts, and what arrives at each part against what it provides,
    /// starting from a run that requires nothing. What arrives at a part is
    /// what the part above requires given what arrived there
    /// ([`Driver::requires`], [`Adapter::requires`]). On any fault, every
    /// such fault, top to bottom, with the same parts' positions and
    /// mismatches in every bracketing, and the same names where each
    /// part's name stays the same through composition, `bind` included.
    /// Otherwise the runner, which walks the chain the same way on each
    /// run, from what the run requires. What each part accepts, sends and
    /// provides is read at the joins and at this `on`, and each part's
    /// name is read here, for the refusals and panics that name it; what
    /// each part requires is asked on each run.
    ///
    /// # Errors
    ///
    /// [`CompositionFaults`] when a part does not accept the set the part
    /// above sends, or does not provide what arrives at it.
    fn on<E: Executor<Accepts = Self::Sends>>(
        mut self,
        executor: E,
    ) -> Result<Runner<Self, E>, CompositionFaults> {
        let mut parts = DeclaredParts::listing_faults();
        let required = self.declare(&RunRequirement::unconstrained(), &mut parts);
        let executor_at = parts.len();
        executor.declare(&required, &mut parts);
        let ids = parts.ids();
        if !parts.fit() {
            return Err(CompositionFaults {
                faults: parts.faults(&ids),
            });
        }
        let (mut boundary, binding) =
            Boundary::fit(self.sends(), executor.accepts(), executor.provides())
                .expect("the chain fits where each part accepts what the part above sends");
        self.bind(binding);
        boundary.part = Some(ids[executor_at].clone());
        Ok(Runner {
            driver: self,
            executor,
            parts: ids,
            boundary,
        })
    }
}

impl<D: Driver> DriverJoins for D {}
