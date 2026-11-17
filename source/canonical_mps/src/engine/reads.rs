// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The reads of one site: probabilities, conditional probabilities and
//! projections, at the centre, on host copies of the sites they read.
//!
//! Between gates, a program measures qubits: a simulator reads the
//! probability of a value, projects the state on the value it drew, and
//! draws samples from conditional probabilities. With the centre on a
//! site, the rest of the chain drops out of the site's reads, so a read
//! moves the centre there ([`canonical_form`](super::canonical_form)) and
//! reads the site alone, through [`SiteAlgebra::read`], its pending gate
//! applied. The rules, stated for callers on [`MpsEngine`]'s methods:
//!
//! - **The state is kept.** A read does not change the state, except by
//!   applying a truncation a gate deferred: it moves the centre, which
//!   changes the tensors, not the state, and applies the window that
//!   fusion left pending, which truncates as any other operation's
//!   application of it would ([`window`](super::window)). A read with conditions projects them on a
//!   fork, so the caller's state is not projected.
//! - **Each value from its own weight.** A probability is the value's
//!   weight at the centre (the squared norm of the centre's elements for
//!   that value) over the sum of both values' weights, not one minus the
//!   other's: a value of weight exactly zero reads exactly 0, and the other
//!   exactly 1, and a rare value whose complement rounds to one is not
//!   refused.
//! - **Projections.** A projection refuses a value of probability exactly
//!   zero, or NaN, before it projects. It divides the site by the norm of
//!   what it kept, computed scaled rather than as `√p`, and uploads the
//!   result as the site's new tensor. It leaves the retained weight as it
//!   is and drops no singular value of its own.
//!
//! [`HostSite`] is the host copy every read of the engine works on, the
//! expectation's ([`expectation`](super::expectation)) included.

use super::{CanonicalMpsEngine, CanonicalMpsError, CanonicalMpsState, chain::Chain};
use crate::algebra::SiteAlgebra;
use num_complex::Complex64;
use std::borrow::Cow;
use tensornet::{MpsEngine, Refusal};

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// P(Zero) and P(One) of `site`, with the centre moved there.
    pub(super) fn probabilities(
        &self,
        chain: &mut Chain<A::Tensor>,
        site: usize,
    ) -> Result<[f64; 2], CanonicalMpsError<A::Error>> {
        self.centre_on(chain, site)?;
        let centre = self.host_site(chain, site)?;
        let [zero, one] = [centre.weight_of(0), centre.weight_of(1)];
        let total = zero + one;
        Ok([zero / total, one / total])
    }

    /// P(Zero) and P(One) of `site` given `given` (checked): on a fork when
    /// `given` is not empty, each condition projected in order.
    pub(super) fn conditional_probabilities(
        &self,
        state: &CanonicalMpsState<A>,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<[f64; 2], CanonicalMpsError<A::Error>> {
        let mut fork;
        let mut borrowed;
        let chain = if given.is_empty() {
            borrowed = state.chain.borrow_mut();
            &mut *borrowed
        } else {
            fork = self.fork(state)?;
            fork.chain.get_mut()
        };
        for &(at, one) in given {
            self.project_on(chain, at, one)?;
        }
        self.probabilities(chain, site)
    }

    /// Projects `site` (checked) on `one`, and returns the value's
    /// probability, or refuses one of probability zero or NaN with
    /// [`Refusal::ZeroProbabilityOutcome`].
    pub(super) fn project_on(
        &self,
        chain: &mut Chain<A::Tensor>,
        site: usize,
        one: bool,
    ) -> Result<f64, CanonicalMpsError<A::Error>> {
        let level = usize::from(one);
        self.centre_on(chain, site)?;
        let mut centre = self.host_site(chain, site)?;
        let weights = [centre.weight_of(0), centre.weight_of(1)];
        let p = weights[level] / (weights[0] + weights[1]);
        if p == 0.0 || p.is_nan() {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        centre.project(level);
        let tensor = self
            .algebra
            .upload(&centre.elements)
            .map_err(CanonicalMpsError::Algebra)?;
        chain.sites[site].tensor = tensor;
        chain.pending[site] = None;
        Ok(p)
    }

    /// The site `at` on the host, its pending gate applied.
    pub(super) fn host_site<'c>(
        &self,
        chain: &'c Chain<A::Tensor>,
        at: usize,
    ) -> Result<HostSite<'c>, CanonicalMpsError<A::Error>> {
        let site = &chain.sites[at];
        let mut host = HostSite {
            left: site.left,
            right: site.right,
            elements: self
                .algebra
                .read(&site.tensor, site.len())
                .map_err(CanonicalMpsError::Algebra)?,
        };
        if let Some(gate) = chain.pending[at] {
            host.apply(&gate);
        }
        Ok(host)
    }
}

/// A site's elements on the host, `T[a, s, b]` at `a + left·(s + 2b)`
/// (the [algebra's layout](crate::algebra)), borrowed from the tensor
/// where the algebra keeps it on the host, until a gate or a projection
/// changes them.
#[derive(Clone)]
pub(super) struct HostSite<'t> {
    pub(super) left: usize,
    pub(super) right: usize,
    pub(super) elements: Cow<'t, [Complex64]>,
}

impl HostSite<'_> {
    /// The squared norm of the elements whose qubit's value is `level`.
    pub(super) fn weight_of(&self, level: usize) -> f64 {
        let l = self.left;
        (0..self.right)
            .map(|b| {
                let start = l * (level + 2 * b);
                self.elements[start..start + l]
                    .iter()
                    .map(Complex64::norm_sqr)
                    .sum::<f64>()
            })
            .sum()
    }

    /// The squared norm of all the elements.
    pub(super) fn weight(&self) -> f64 {
        self.elements.iter().map(Complex64::norm_sqr).sum()
    }

    /// Applies the one-site gate `m` (row-major) to the qubit.
    pub(super) fn apply(&mut self, m: &[Complex64; 4]) {
        let l = self.left;
        let elements = self.elements.to_mut();
        for b in 0..self.right {
            for a in 0..l {
                let (i0, i1) = (a + 2 * l * b, a + l + 2 * l * b);
                let (v0, v1) = (elements[i0], elements[i1]);
                elements[i0] = m[0] * v0 + m[1] * v1;
                elements[i1] = m[2] * v0 + m[3] * v1;
            }
        }
    }

    /// Keeps the elements whose qubit's value is `level`, not all zero, and
    /// divides the site by their norm. The norm is computed scaled by the
    /// largest modulus, so it neither underflows nor overflows.
    pub(super) fn project(&mut self, level: usize) {
        let l = self.left;
        let elements = self.elements.to_mut();
        for (at, x) in elements.iter_mut().enumerate() {
            if (at / l) % 2 != level {
                *x = Complex64::ZERO;
            }
        }
        let largest = elements.iter().map(|x| x.norm()).fold(0.0, f64::max);
        let scaled: f64 = elements.iter().map(|x| (x / largest).norm_sqr()).sum();
        let norm = scaled.sqrt();
        for x in elements {
            *x = *x / largest / norm;
        }
    }
}
