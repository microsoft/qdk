// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An adapter as users compose it ([`Adapter`]), and the joins it heads:
//! with an adapter, which gives an adapter, and with an executor, which
//! gives an executor.

use std::ops::Shr;

use super::{Compose, Composed, Executor, check, faults, fit};
use crate::{Adapt, Execute, FitFaults};

/// An adapter, ready to compose with `>>`.
pub struct Adapter<A: Adapt> {
    pub(super) part: A,
    pub(super) fit: Vec<FitFaults>,
}

impl<A: Adapt> Adapter<A> {
    /// The adapter `part`, ready to compose.
    pub fn new(part: A) -> Self {
        Self {
            part,
            fit: Vec::new(),
        }
    }

    /// As [`Driver::check`](crate::Driver::check).
    pub fn check(&self) -> Result<(), &[FitFaults]> {
        check(&self.fit)
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

impl<U: Adapt, L: Adapt> Shr<Adapter<L>> for Adapter<U> {
    type Output = <Self as Compose<Adapter<L>>>::Output;

    fn shr(self, rhs: Adapter<L>) -> Self::Output {
        self.compose(rhs)
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

impl<A: Adapt, E: Execute> Shr<Executor<E>> for Adapter<A> {
    type Output = <Self as Compose<Executor<E>>>::Output;

    fn shr(self, rhs: Executor<E>) -> Self::Output {
        self.compose(rhs)
    }
}
