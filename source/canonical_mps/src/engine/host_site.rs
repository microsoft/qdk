// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`HostSite`]: a site's elements copied to the host, its pending gate
//! applied. Every computation the engine makes on the host works on it:
//! the reads of a measurement ([`measurement`](super::measurement)), an
//! observable's expectation ([`expectation`](super::expectation)), and the
//! sites an MPO multiplies ([`mpo`](super::mpo)).
//!
//! The elements follow the [algebra's layout](crate::site_algebra):
//! `T[a, s, b]`, for the left bond's index `a`, the qubit's value `s` and
//! the right bond's index `b`, is at `a + left·(s + 2b)`. So they come in
//! runs of `left` elements, one run per value and right index, the values
//! alternating. With `left = 3` and `right = 2`:
//!
//! ```text
//!   position   0  1  2 | 3  4  5 | 6  7  8 | 9 10 11
//!   a          0  1  2 | 0  1  2 | 0  1  2 | 0  1  2
//!   (s, b)      (0, 0) |  (1, 0) |  (0, 1) |  (1, 1)
//! ```
//!
//! A value's weight sums the runs of that value. A gate mixes each element
//! with the one a run away, which differs only in the qubit's value.

use super::{CanonicalMpsEngine, CanonicalMpsError, chain::Chain};
use crate::site_algebra::SiteAlgebra;
use dense_linalg::{norm_divided_by, squared_norm};
use num_complex::Complex64;
use std::borrow::Cow;

/// A site's elements on the host, in the layout of this module's header.
/// Where the algebra keeps the tensor on the host, they are borrowed from
/// it, until a gate or a projection changes them.
#[derive(Clone)]
pub(super) struct HostSite<'t> {
    /// The left bond's extent.
    pub(super) left: usize,
    /// The right bond's extent.
    pub(super) right: usize,
    /// The elements, `left·2·right` of them, in the header's layout.
    pub(super) elements: Cow<'t, [Complex64]>,
}

impl HostSite<'_> {
    /// The weight of the qubit's `value` (0 or 1): the squared norm of the
    /// elements `T[·, value, ·]`.
    pub(super) fn weight_of(&self, value: usize) -> f64 {
        (0..self.right)
            .map(|right_index| squared_norm(self.run_of(value, right_index)))
            .sum()
    }

    /// The squared norm of all the elements.
    pub(super) fn weight(&self) -> f64 {
        squared_norm(&self.elements)
    }

    /// Applies the one-site gate `gate` (row-major) to the qubit: each pair
    /// of elements `T[a, 0, b]`, `T[a, 1, b]` becomes `gate` times the pair.
    pub(super) fn apply(&mut self, gate: &[Complex64; 4]) {
        let left = self.left;
        let elements = self.elements.to_mut();
        for right_index in 0..self.right {
            let zero_run_start = run_start(left, 0, right_index);
            let one_run_start = run_start(left, 1, right_index);
            for left_index in 0..left {
                let (zero_at, one_at) = (zero_run_start + left_index, one_run_start + left_index);
                let pair = (elements[zero_at], elements[one_at]);
                [elements[zero_at], elements[one_at]] = gate_times(gate, pair);
            }
        }
    }

    /// Keeps the elements of the qubit's `value`, not all zero, and divides
    /// the site by their norm.
    pub(super) fn project(&mut self, value: usize) {
        self.keep_only_value(value);
        self.normalise();
    }

    /// The run of `left` elements `T[·, value, right_index]`.
    fn run_of(&self, value: usize, right_index: usize) -> &[Complex64] {
        let start = run_start(self.left, value, right_index);
        &self.elements[start..start + self.left]
    }

    /// Keeps the elements `T[·, value, ·]` and sets the others to zero.
    fn keep_only_value(&mut self, value: usize) {
        let left = self.left;
        for (at, element) in self.elements.to_mut().iter_mut().enumerate() {
            if value_at(left, at) != value {
                *element = Complex64::ZERO;
            }
        }
    }

    /// Divides the elements, not all zero, by their norm. The norm is
    /// computed scaled by the largest modulus, so it neither underflows nor
    /// overflows.
    fn normalise(&mut self) {
        let elements = self.elements.to_mut();
        let largest = largest_modulus(elements);
        let norm = norm_divided_by(elements, largest);
        for element in elements {
            *element = *element / largest / norm;
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

/// The position of `T[0, value, right_index]`, where its run starts, on a
/// site whose left bond is `left`.
#[inline]
fn run_start(left: usize, value: usize, right_index: usize) -> usize {
    left * (value + 2 * right_index)
}

/// The qubit's value of the element at `at`, on a site whose left bond is
/// `left`.
#[inline]
fn value_at(left: usize, at: usize) -> usize {
    (at / left) % 2
}

/// `gate · (zero, one)`, for the row-major `gate`.
#[inline]
fn gate_times(gate: &[Complex64; 4], (zero, one): (Complex64, Complex64)) -> [Complex64; 2] {
    [
        gate[0] * zero + gate[1] * one,
        gate[2] * zero + gate[3] * one,
    ]
}

/// The largest modulus of the elements, 0 when there are none.
fn largest_modulus(elements: &[Complex64]) -> f64 {
    elements.iter().map(|x| x.norm()).fold(0.0, f64::max)
}
