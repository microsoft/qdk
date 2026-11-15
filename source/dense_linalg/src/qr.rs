// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Qr`]: the thin QR decomposition of a dense complex matrix.

use crate::kernels::{Kernels, Shipped};
use crate::matrix::{Matrix, MatrixRef};

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
    /// reflections. It cannot fail.
    #[must_use]
    pub fn of(a: MatrixRef<'_>) -> Self {
        Shipped::qr(a)
    }
}
