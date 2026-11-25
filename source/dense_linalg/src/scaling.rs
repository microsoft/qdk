// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`PowerOfTwoScaling`]: the power of two that brings a matrix's largest
//! entry near 1. Then the largest squared norms its decompositions compute
//! neither overflow nor underflow. A far smaller column's may underflow,
//! far below rounding of the largest.
//!
//! The SVD and the thin QR decomposition scale their input by it when its
//! largest entry is far from 1 ([`PowerOfTwoScaling::of`] says how far).
//! They scale back the factors that carry the magnitude: the singular
//! values, `R`. Otherwise they leave the matrix as it is, so their results
//! are those of the unscaled matrix. Multiplying by a power of two is
//! exact, with one exception. It rounds an entry it takes below the normal
//! range (more than about 2¹⁰²² times smaller than the largest).
//!
//! [`largest_part`], the entry the scaling reads, also sets the scale of
//! the SVD's checks.

use crate::matrix::Matrix;
use num_complex::Complex64;
use std::ops::Div;

/// The power of two `2⁻ᵉ` that brings a matrix's largest entry near 1,
/// held as two factors, each finite whatever `e`.
pub(crate) struct PowerOfTwoScaling {
    factors: [f64; 2],
}

impl PowerOfTwoScaling {
    /// The scaling for a matrix with these elements. `None` when it needs
    /// none: its largest entry (the largest real or imaginary part's
    /// magnitude) is within `[2⁻¹⁰⁰, 2¹⁰⁰]`, zero, or not finite.
    pub(crate) fn of(elements: &[Complex64]) -> Option<Self> {
        let largest = largest_part(elements);
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

    /// `matrix` times `2⁻ᵉ`.
    pub(crate) fn scaled(&self, matrix: Matrix) -> Matrix {
        let (rows, cols) = (matrix.rows(), matrix.cols());
        let [first, second] = self.factors;
        let elements = matrix.into_elements().into_iter();
        Matrix::new(rows, cols, elements.map(|x| x * first * second).collect())
    }

    /// `x` times `2ᵉ`, undoing [`Self::scaled`].
    pub(crate) fn scaled_back<T: Div<f64, Output = T>>(&self, x: T) -> T {
        x / self.factors[0] / self.factors[1]
    }
}

/// The largest magnitude among the real and imaginary parts of
/// `elements`, 0 when there are none. A NaN part is passed over.
pub(crate) fn largest_part(elements: &[Complex64]) -> f64 {
    elements.iter().fold(0.0, |largest: f64, x| {
        largest.max(x.re.abs()).max(x.im.abs())
    })
}
