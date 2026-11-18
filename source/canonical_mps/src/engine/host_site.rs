// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`HostSite`]: a site's elements copied to the host, its pending gate
//! applied, which every computation the engine makes on the host works on:
//! the reads of a measurement ([`measurement`](super::measurement)), an
//! observable's expectation ([`expectation`](super::expectation)), and the
//! sites an MPO multiplies ([`mpo`](super::mpo)).

use super::{CanonicalMpsEngine, CanonicalMpsError, chain::Chain};
use crate::site_algebra::SiteAlgebra;
use num_complex::Complex64;
use std::borrow::Cow;

/// A site's elements on the host, `T[a, s, b]` at `a + left·(s + 2b)`
/// (the [algebra's layout](crate::site_algebra)), borrowed from the tensor
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

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
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
