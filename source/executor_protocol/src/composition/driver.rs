// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A driver as users compose it ([`Driver`]), and the joins it heads:
//! with an adapter, which gives a driver, and with an executor, which
//! checks the admission and gives a runner.

use std::ops::Shr;

use super::boundary_crossing::BatchLimits;
use super::runner::RunLimits;
use super::{Adapter, Compose, Composed, CompositionFaults, Executor, check, faults, fit};
use crate::composition::admission::by_part::ByPart;
use crate::{Adapt, AdmissionFaults, Drive, Execute, FitFaults, Runner};

/// A driver, ready to compose with `>>`.
pub struct Driver<D: Drive> {
    pub(super) part: D,
    pub(super) fit: Vec<FitFaults>,
}

impl<D: Drive> Driver<D> {
    /// The driver `part`, ready to compose.
    pub fn new(part: D) -> Self {
        Self {
            part,
            fit: Vec::new(),
        }
    }

    /// `Ok` when every boundary inside it fits, else each that does not,
    /// top to bottom. The admission waits for the `>>` that makes a runner.
    pub fn check(&self) -> Result<(), &[FitFaults]> {
        check(&self.fit)
    }
}

impl<D: Drive, A: Adapt> Compose<Adapter<A>> for Driver<D> {
    type Output = Driver<Composed<D, A>>;

    fn compose(mut self, rhs: Adapter<A>) -> Self::Output {
        let boundary = fit(self.part.sends(), rhs.part.accepts());
        let boundary = boundary.map(|below| self.part.bind(below)).err();
        Driver {
            fit: faults(self.fit, boundary, rhs.fit),
            part: Composed::new(self.part, rhs.part),
        }
    }
}

impl<D: Drive, A: Adapt> Shr<Adapter<A>> for Driver<D> {
    type Output = <Self as Compose<Adapter<A>>>::Output;

    fn shr(self, rhs: Adapter<A>) -> Self::Output {
        self.compose(rhs)
    }
}

impl<D: Drive, E: Execute> Compose<Executor<E>> for Driver<D> {
    type Output = Result<Runner<D, E>, CompositionFaults>;

    /// Also checks the admission at every boundary of the chain: the needs
    /// of the part above, adapted down to it, against the offer of the part
    /// below, adapted up to it, so that the verdict and its numbers are the
    /// same wherever the brackets go. On any fault, every fault of the
    /// chain, with each part's needs and offer when an offer falls short;
    /// otherwise the runner, which keeps as its run limits the most shots
    /// and the seedability that every part's offer meets, so that an
    /// adapter may lower them but never raise them. Every part's needs and
    /// offer are read once, and each boundary keeps the needs and the offer
    /// that cross it, so that each run gets every part ready for the needs
    /// the admission checked, and checks counts and limits against the
    /// offers it checked.
    fn compose(mut self, mut rhs: Executor<E>) -> Self::Output {
        let boundary = fit(self.part.sends(), rhs.part.accepts());
        let boundary = boundary.map(|below| self.part.bind(below)).err();
        let fit = faults(self.fit, boundary, rhs.fit);
        let mut by_part = ByPart::new();
        let needs = self.part.needs_by_part(&mut by_part);
        rhs.part.needs_by_part(needs.clone(), &mut by_part);
        let offer = rhs.part.offers_by_part(&mut by_part);
        let executor_part = by_part
            .last_offered()
            .expect("the executor's offer was just listed");
        self.part.offers_by_part(offer.clone(), &mut by_part);
        let driver_part = by_part
            .first_needing()
            .expect("the driver's needs were listed first");
        let run_limits = RunLimits::of(by_part.offers.iter().map(|(_, offer)| offer));
        let admission = AdmissionFaults::of(by_part);
        if !fit.is_empty() || admission.is_some() {
            return Err(CompositionFaults::new(fit, admission));
        }
        let batch_limits = BatchLimits::new(rhs.part.accepts(), &offer, &needs);
        Ok(Runner::new(
            (self.part, driver_part),
            (rhs.part, executor_part),
            run_limits,
            (needs, offer),
            batch_limits,
        ))
    }
}

impl<D: Drive, E: Execute> Shr<Executor<E>> for Driver<D> {
    type Output = <Self as Compose<Executor<E>>>::Output;

    fn shr(self, rhs: Executor<E>) -> Self::Output {
        self.compose(rhs)
    }
}
