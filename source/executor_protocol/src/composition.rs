// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Composing parts into a chain: the role types users hold ([`Driver`],
//! [`Adapter`], [`Executor`]), and [`Compose`], whose operator is `>>`.
//!
//! Users compose parts, and implementers write one part, as a trait. Rust
//! lets the protocol write `>>` only for its own types, so each role has a
//! role type, a thin wrapper of a part, that users compose. Four joins are
//! allowed, each giving a part of the role its position gives:
//!
//! | `>>` | gives |
//! | --- | --- |
//! | driver `>>` adapter | a driver |
//! | adapter `>>` adapter | an adapter |
//! | adapter `>>` executor | an executor |
//! | driver `>>` executor | a [`Runner`], or the chain's faults |
//!
//! So the brackets do not matter: `(d >> a) >> e` and `d >> (a >> e)` are
//! the same chain. Each join checks that the instructions sent fit those
//! accepted, and keeps the faults of each boundary that does not, so that
//! the join that makes a runner reports every fault of the chain at once:
//! each boundary's, and the admission's, at each boundary the needs of the
//! part above against the offer of the part below ([`CompositionFaults`]).
//! A chain that composes is then provisioned, top down, and the protocol
//! checks each answer crossing its boundaries, and each batch its sender
//! marked [`checked`](crate::Batch::checked), so a part writes only its own
//! logic.

mod composed;
mod composition_faults;

use std::ops::Shr;

use crate::admission::by_part::ByPart;
use crate::checked::batch_limits;
use crate::runner::RunLimits;
use crate::{Adapt, AdmissionFaults, Drive, Execute, FitFaults, InstructionSet, OpcodeMap, Runner};

pub use composed::Composed;
pub use composition_faults::CompositionFaults;

/// Joins two parts into one, in the role their positions give it: checks
/// that the instructions the upper part sends fit those the lower part
/// accepts, binds them where they fit, and keeps every fit fault of both
/// parts and of their boundary, top to bottom. The join of a driver with an
/// executor also checks the admission, and returns every fault of the
/// chain, or provisions every part and gives a [`Runner`].
///
/// Its operator is `>>`: `a >> b` is `a.compose(b)`. Rust has no
/// overloading, so one name for every pair of roles is a trait.
pub trait Compose<Rhs> {
    /// The part, or the runner, the join gives.
    type Output;

    /// Joins `self`, above, with `rhs`, below.
    fn compose(self, rhs: Rhs) -> Self::Output;
}

/// A driver, ready to compose with `>>`.
pub struct Driver<D: Drive> {
    part: D,
    fit: Vec<FitFaults>,
}

/// An adapter, ready to compose with `>>`.
pub struct Adapter<A: Adapt> {
    part: A,
    fit: Vec<FitFaults>,
}

/// An executor, ready to compose with `>>`.
pub struct Executor<E: Execute> {
    part: E,
    fit: Vec<FitFaults>,
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

impl<A: Adapt> Adapter<A> {
    /// The adapter `part`, ready to compose.
    pub fn new(part: A) -> Self {
        Self {
            part,
            fit: Vec::new(),
        }
    }

    /// As [`Driver::check`].
    pub fn check(&self) -> Result<(), &[FitFaults]> {
        check(&self.fit)
    }
}

impl<E: Execute> Executor<E> {
    /// The executor `part`, ready to compose.
    pub fn new(part: E) -> Self {
        Self {
            part,
            fit: Vec::new(),
        }
    }

    /// As [`Driver::check`].
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

impl<U: Adapt, L: Adapt> Compose<Adapter<L>> for Adapter<U> {
    type Output = Adapter<Composed<U, L>>;

    fn compose(mut self, rhs: Adapter<L>) -> Self::Output {
        let boundary = fit(self.part.sends(), rhs.part.accepts());
        let boundary = boundary.map(|below| self.part.bind(below)).err();
        Adapter {
            fit: faults(self.fit, boundary, rhs.fit),
            part: Composed::new(self.part, rhs.part),
        }
    }
}

impl<A: Adapt, E: Execute> Compose<Executor<E>> for Adapter<A> {
    type Output = Executor<Composed<A, E>>;

    fn compose(mut self, rhs: Executor<E>) -> Self::Output {
        let boundary = fit(self.part.sends(), rhs.part.accepts());
        let boundary = boundary.map(|below| self.part.bind(below)).err();
        Executor {
            fit: faults(self.fit, boundary, rhs.fit),
            part: Composed::new(self.part, rhs.part),
        }
    }
}

impl<D: Drive, E: Execute> Compose<Executor<E>> for Driver<D> {
    type Output = Result<Runner<D, E>, CompositionFaults>;

    /// Also checks the admission at every boundary of the chain: the needs
    /// of the part above, adapted down to it, against the offer of the part
    /// below, adapted up to it, so that the verdict and its numbers are the
    /// same wherever the brackets go. On any fault, every fault of the
    /// chain, with each part's needs and offer when an offer falls short;
    /// otherwise provisions the driver, then the executor with the driver's
    /// needs, and the runner keeps as its run limits the most shots and the
    /// seedability that every part's offer meets, so that an adapter may
    /// lower them but never raise them. Every offer is read once, before
    /// any part is provisioned, and each boundary keeps the one that
    /// crosses it, so that every check of a run is against the offers the
    /// admission checked.
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
        let batch_limits = batch_limits(rhs.part.accepts(), &offer, &needs);
        self.part.provision();
        rhs.part.provision(needs);
        Ok(Runner::new(
            (self.part, driver_part),
            (rhs.part, executor_part),
            run_limits,
            offer,
            batch_limits,
        ))
    }
}

impl<D: Drive, A: Adapt> Shr<Adapter<A>> for Driver<D> {
    type Output = <Self as Compose<Adapter<A>>>::Output;

    fn shr(self, rhs: Adapter<A>) -> Self::Output {
        self.compose(rhs)
    }
}

impl<U: Adapt, L: Adapt> Shr<Adapter<L>> for Adapter<U> {
    type Output = <Self as Compose<Adapter<L>>>::Output;

    fn shr(self, rhs: Adapter<L>) -> Self::Output {
        self.compose(rhs)
    }
}

impl<A: Adapt, E: Execute> Shr<Executor<E>> for Adapter<A> {
    type Output = <Self as Compose<Executor<E>>>::Output;

    fn shr(self, rhs: Executor<E>) -> Self::Output {
        self.compose(rhs)
    }
}

impl<D: Drive, E: Execute> Shr<Executor<E>> for Driver<D> {
    type Output = <Self as Compose<Executor<E>>>::Output;

    fn shr(self, rhs: Executor<E>) -> Self::Output {
        self.compose(rhs)
    }
}

/// The receiver's opcode for each instruction `sent`, or the boundary's
/// fault, naming both sets.
fn fit(sent: &InstructionSet, accepted: &InstructionSet) -> Result<OpcodeMap, FitFaults> {
    sent.fits(accepted).map_err(|mismatches| FitFaults {
        sent: sent.name().to_owned(),
        accepted: accepted.name().to_owned(),
        mismatches,
    })
}

/// The fit faults of a join, top to bottom: the upper part's, the
/// boundary's, the lower part's.
fn faults(
    mut upper: Vec<FitFaults>,
    boundary: Option<FitFaults>,
    lower: Vec<FitFaults>,
) -> Vec<FitFaults> {
    upper.extend(boundary);
    upper.extend(lower);
    upper
}

/// `Ok` when there is no fault in `fit`.
fn check(fit: &[FitFaults]) -> Result<(), &[FitFaults]> {
    if fit.is_empty() { Ok(()) } else { Err(fit) }
}
