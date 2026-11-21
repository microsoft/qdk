// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The joins a driver heads ([`DriverJoins`]): through an adapter, which
//! gives a driver, and on an executor, which checks the admission and
//! gives a runner.

use super::chain_check::ChainCheck;
use super::composed::Composed;
use super::{CompositionFaults, fit};
use crate::{Adapter, Driver, Executor, Runner};

/// The joins of a driver with the part below it: [`through`](Self::through)
/// an adapter, which gives a driver, and [`on`](Self::on) an executor,
/// which gives the [`Runner`] of the whole chain. Every driver has them.
///
/// Each join checks that the instructions the driver sends fit those the
/// part below accepts, and binds them where they fit
/// ([`Driver::bind`]); a boundary that does not fit is reported, with
/// every other fault of the chain, by the `on` that makes the runner.
pub trait DriverJoins: Driver + Sized {
    /// This driver calling `adapter`, as one driver: it sends what
    /// `adapter` sends, and needs what `adapter` needs below for this
    /// driver's needs.
    fn through<A: Adapter>(mut self, adapter: A) -> Composed<Self, A> {
        let fit_faults = fit(self.sends(), adapter.accepts())
            .map(|below| self.bind(below))
            .err();
        Composed::new(self, adapter, fit_faults)
    }

    /// The runner of this driver calling `executor`, or every fault of the
    /// chain.
    ///
    /// Also checks the admission at every boundary of the chain: the needs
    /// of the part above, adapted down to it, against the offer of the part
    /// below, adapted up to it, so that the verdict and its numbers are the
    /// same wherever the brackets go. On any fault, every fault of the
    /// chain: the fit faults of each boundary, top to bottom, and, when an
    /// offer falls short, each part's needs and offer. Otherwise the
    /// runner, which keeps as its run limits the most shots and the
    /// seedability that every part's offer meets, so that an adapter may
    /// lower them but never raise them. Every part's needs and offer are
    /// read once, and each boundary keeps the needs and the offer that
    /// cross it, so that each run gets every part ready for the needs the
    /// admission checked, and checks counts and limits against the offers
    /// it checked.
    ///
    /// # Errors
    ///
    /// [`CompositionFaults`] when a boundary does not fit or the chain is
    /// not admitted.
    fn on<E: Executor>(mut self, mut executor: E) -> Result<Runner<Self, E>, CompositionFaults> {
        let fit_faults = fit(self.sends(), executor.accepts())
            .map(|below| self.bind(below))
            .err();
        let (chain, crossing) = ChainCheck::of(&mut self, fit_faults.as_ref(), &mut executor);
        let driver_part = chain
            .first_needing()
            .expect("the driver added its needs first");
        let run_limits = chain.conclude()?;
        Ok(Runner::new(
            (self, driver_part),
            executor,
            crossing,
            run_limits,
        ))
    }
}

impl<D: Driver> DriverJoins for D {}
