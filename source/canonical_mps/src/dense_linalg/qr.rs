// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Qr`]: the thin QR decomposition of a dense complex matrix.

use crate::dense_linalg::kernels::{DefaultKernels, Kernels};
use crate::dense_linalg::matrix::{Matrix, MatrixRef};
use crate::dense_linalg::scaling::PowerOfTwoScaling;

/// A thin QR decomposition `a = q·r`: with `k = min(rows, cols)`, `q` is
/// `rows × k` with orthonormal columns and `r` is `k × cols`, upper
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
    /// The thin QR decomposition of `a`, to rounding, by Householder
    /// reflections. It cannot fail, and checks nothing. A matrix without
    /// elements, of any shape, has empty factors.
    ///
    /// It answers at any scale, down to subnormal entries, short of a
    /// matrix whose `r` is too large to store: a matrix whose largest entry
    /// is outside `[2⁻¹⁰⁰, 2¹⁰⁰]` is decomposed scaled by a power of two,
    /// as the SVD's is, and `r` is scaled back.
    ///
    /// For a finite `a` only: when an entry of `a` is not finite (NaN or
    /// infinite), no QR decomposition gives it back, and the factors are
    /// unspecified (they may hold NaNs). Where
    /// [`Svd::of`](crate::dense_linalg::Svd::of) refuses such a matrix, this
    /// returns them unchecked.
    #[must_use]
    pub fn of(a: MatrixRef<'_>) -> Self {
        let (rows, cols) = (a.rows(), a.cols());
        if a.elements().is_empty() {
            // `q` and `r` are empty too. Answered here, not by the library,
            // which may size its work by a dimension, here up to
            // `usize::MAX` with no element to bound it.
            let k = rows.min(cols);
            return Self {
                q: Matrix::new(rows, k, Vec::new()),
                r: Matrix::new(k, cols, Vec::new()),
            };
        }
        let Some(scaling) = PowerOfTwoScaling::of(a.elements()) else {
            return DefaultKernels::qr(a);
        };
        let scaled = scaling.scaled(Matrix::new(rows, cols, a.elements().to_vec()));
        let Qr { q, r } = DefaultKernels::qr(scaled.view());
        // 2⁻ᵉ·a = q·r', so a = q·(2ᵉ·r').
        let k = r.rows();
        let elements = r.into_elements().into_iter();
        let r = Matrix::new(k, cols, elements.map(|x| scaling.scaled_back(x)).collect());
        Qr { q, r }
    }
}
