// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Site`]: one qubit's tensor in the chain, and the kernels that act on
//! it alone, without a factorization: applying a one-site gate, projecting
//! the qubit on a value, and weighing its values, which the engine's reads
//! and gates compose with the products, QR and SVD.

use super::c;
use crate::linear_algebra::MatrixRef;
use num_complex::Complex64;

/// One site tensor `T[a, s, b]` of extents `[left, 2, right]`, column-major:
/// element `a + left·(s + 2·b)`. Read as a `2·left × right` matrix its row
/// is `(a, s)`, and as a `left × 2·right` matrix its column is `(s, b)`, both
/// without a copy.
#[derive(Clone, Debug)]
pub(crate) struct Site {
    pub(crate) left: usize,
    pub(crate) right: usize,
    pub(crate) elements: Vec<Complex64>,
}

impl Site {
    /// The qubit's |0⟩, on bonds of 1.
    pub(super) fn zero() -> Self {
        Self {
            left: 1,
            right: 1,
            elements: vec![c(1.0), c(0.0)],
        }
    }

    /// The tensor as a `2·left × right` matrix.
    pub(crate) fn rows_left(&self) -> MatrixRef<'_> {
        MatrixRef::new(2 * self.left, self.right, &self.elements)
    }

    /// The tensor as a `left × 2·right` matrix.
    pub(crate) fn cols_right(&self) -> MatrixRef<'_> {
        MatrixRef::new(self.left, 2 * self.right, &self.elements)
    }

    /// The squared norm of the elements whose physical index is `level`.
    pub(crate) fn weight_of(&self, level: usize) -> f64 {
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

    pub(crate) fn weight(&self) -> f64 {
        self.elements.iter().map(Complex64::norm_sqr).sum()
    }

    /// Applies the one-site gate `m` (row-major) to the physical index.
    pub(crate) fn apply(&mut self, m: &[Complex64]) {
        let l = self.left;
        for b in 0..self.right {
            for a in 0..l {
                let (i0, i1) = (a + 2 * l * b, a + l + 2 * l * b);
                let (v0, v1) = (self.elements[i0], self.elements[i1]);
                self.elements[i0] = m[0] * v0 + m[1] * v1;
                self.elements[i1] = m[2] * v0 + m[3] * v1;
            }
        }
    }

    /// Keeps the elements whose physical index is `level`, zeroes the
    /// others, and divides the site by the kept elements' norm, so that the
    /// site, as the centre, holds a state of norm 1. The norm is computed on
    /// the elements divided by the largest of their moduli, so that the sum
    /// of squares, at least 1, does not underflow however small the kept
    /// weight; the kept elements are not all zero.
    pub(crate) fn project(&mut self, level: usize) {
        let l = self.left;
        for (at, x) in self.elements.iter_mut().enumerate() {
            if (at / l) % 2 != level {
                *x = c(0.0);
            }
        }
        let largest = self.elements.iter().map(|x| x.norm()).fold(0.0, f64::max);
        let scaled: f64 = self.elements.iter().map(|x| (x / largest).norm_sqr()).sum();
        let norm = scaled.sqrt();
        for x in &mut self.elements {
            *x = *x / largest / norm;
        }
    }
}
