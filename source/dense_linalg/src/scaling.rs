// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`PowerOfTwoScaling`]: the power of two that brings a matrix's largest
//! entry near 1, so that the squared norms its decompositions compute
//! neither overflow nor underflow.
//!
//! The SVD and the thin QR decomposition scale their input by it when its
//! largest entry is outside `[2⁻¹⁰⁰, 2¹⁰⁰]`, and scale back the factors that
//! carry the magnitude: the singular values, `R`. Inside that range they
//! leave the matrix as it is, so that their results there are those of the
//! unscaled matrix. Multiplying by a power of two is exact, except on an
//! entry it takes below the normal range (more than about 2¹⁰²² times
//! smaller than the largest), which it rounds.

use crate::matrix::Matrix;
use num_complex::Complex64;
use std::ops::Div;

/// The power of two `2⁻ᵉ` that brings a matrix's largest entry near 1,
/// held as two factors, each finite whatever `e`.
pub(crate) struct PowerOfTwoScaling {
    factors: [f64; 2],
}

impl PowerOfTwoScaling {
    /// The scaling for a matrix with these elements, or `None` when it
    /// needs none: its largest entry (the largest real or imaginary part's
    /// magnitude) is within `[2⁻¹⁰⁰, 2¹⁰⁰]`, zero, or not finite.
    pub(crate) fn of(elements: &[Complex64]) -> Option<Self> {
        let largest = elements
            .iter()
            .fold(0.0, |m: f64, x| m.max(x.re.abs()).max(x.im.abs()));
        let unscaled = 2f64.powi(-100)..=2f64.powi(100);
        if largest == 0.0 || !largest.is_finite() || unscaled.contains(&largest) {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a binary exponent, |e| < 1100"
        )]
        let exponent = largest.log2().round() as i32;
        Some(Self {
            factors: [
                2f64.powi(-exponent / 2),
                2f64.powi(-(exponent - exponent / 2)),
            ],
        })
    }

    /// `m` times `2⁻ᵉ`.
    pub(crate) fn scaled(&self, m: Matrix) -> Matrix {
        let (rows, cols) = (m.rows(), m.cols());
        let [first, second] = self.factors;
        let elements = m.into_elements().into_iter();
        Matrix::new(rows, cols, elements.map(|x| x * first * second).collect())
    }

    /// `x` times `2ᵉ`, undoing [`Self::scaled`].
    pub(crate) fn scaled_back<T: Div<f64, Output = T>>(&self, x: T) -> T {
        x / self.factors[0] / self.factors[1]
    }
}
