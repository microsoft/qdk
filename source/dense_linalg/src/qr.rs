// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Qr`]: the thin QR decomposition of a dense complex matrix.

use crate::kernels::{DefaultKernels, Kernels};
use crate::matrix::{Matrix, MatrixRef};
use crate::scaling::PowerOfTwoScaling;

/// A thin QR decomposition `a = q·r`. Let `k = min(rows, cols)`. `q` is
/// `rows × k` with orthonormal columns. `r` is `k × cols`, upper
/// trapezoidal (upper triangular when square or tall).
#[derive(Clone, Debug)]
pub struct Qr {
    /// The factor with orthonormal columns.
    pub q: Matrix,
    /// The upper-trapezoidal factor (upper triangular when square or
    /// tall).
    pub r: Matrix,
}

impl Qr {
    /// Up to rounding, the thin QR decomposition of `a`, by Householder
    /// reflections (`docs/algorithms.md`'s Terms defines them; the
    /// kernel's `householder_reflectors.rs` draws a step). It cannot fail,
    /// and checks nothing. A matrix without elements, of any shape, has
    /// empty factors.
    ///
    /// It answers at any scale, down to subnormal entries, short of a
    /// matrix whose `r` is too large to store. A matrix whose largest entry
    /// is far from 1 is decomposed scaled by a power of two, as the SVD's
    /// is. Then `r` is scaled back.
    ///
    /// For a finite `a` only. When an entry of `a` is not finite (NaN or
    /// infinite), no QR decomposition gives it back. The factors are then
    /// unspecified (they may hold NaNs).
    /// [`Svd::of`](crate::Svd::of) refuses such a matrix;
    /// this returns them unchecked.
    #[must_use]
    pub fn of(a: MatrixRef<'_>) -> Self {
        let (rows, cols) = (a.rows(), a.cols());
        if a.elements().is_empty() {
            // Answered here, so the kernels see only matrices whose element
            // count bounds their dimensions; an empty one's may be up to
            // `usize::MAX`.
            return Self::empty(rows, cols);
        }
        let Some(scaling) = PowerOfTwoScaling::of(a.elements()) else {
            return DefaultKernels::qr(a);
        };
        let scaled = scaling.scaled(Matrix::new(rows, cols, a.elements().to_vec()));
        DefaultKernels::qr(scaled.view()).with_r_scaled_back(&scaling)
    }

    /// The empty factors of a `rows × cols` matrix without elements.
    fn empty(rows: usize, cols: usize) -> Self {
        let k = rows.min(cols);
        Self {
            q: Matrix::new(rows, k, Vec::new()),
            r: Matrix::new(k, cols, Vec::new()),
        }
    }

    /// The QR of `a` from that of `a` scaled by `scaling`:
    /// `2⁻ᵉ·a = q·r'`, so `a = q·(2ᵉ·r')`.
    fn with_r_scaled_back(self, scaling: &PowerOfTwoScaling) -> Self {
        let Qr { q, r } = self;
        let (rows, cols) = (r.rows(), r.cols());
        let elements = r.into_elements().into_iter();
        let r = Matrix::new(
            rows,
            cols,
            elements.map(|x| scaling.scaled_back(x)).collect(),
        );
        Qr { q, r }
    }
}

#[cfg(test)]
mod tests;
