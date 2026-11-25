// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Lq`]: the thin LQ decomposition of a dense complex matrix, the mirror
//! image of the QR: orthonormal rows on the right, in place of orthonormal
//! columns on the left.

use crate::matrix::{Matrix, MatrixRef};
use crate::qr::Qr;

/// A thin LQ decomposition `a = l·q`. Let `k = min(rows, cols)`. `l` is
/// `rows × k`, lower trapezoidal (lower triangular when square or wide).
/// `q` is `k × cols` with orthonormal rows.
#[derive(Clone, Debug)]
pub struct Lq {
    /// The lower-trapezoidal factor (lower triangular when square or
    /// wide).
    pub l: Matrix,
    /// The factor with orthonormal rows.
    pub q: Matrix,
}

impl Lq {
    /// Up to rounding, the thin LQ decomposition of `a`: a lower
    /// trapezoidal factor times one with orthonormal rows. A caller that
    /// moves a matrix's weight to the left keeps `q`, an orthonormal right
    /// factor, and carries `l` on.
    ///
    /// It is the thin QR of the adjoint, read back: `a† = Q·R` gives
    /// `a = R†·Q†`, so `l = R†` and `q = Q†`:
    ///
    /// ```text
    ///   a (m × n)  =  l (m × k)  ·  q (k × n),   k = min(m, n)
    ///   a† = Q·R   →  l = R†, lower trapezoidal;  q = Q†, orthonormal rows
    /// ```
    ///
    /// Like [`Qr::of`], it cannot fail and checks nothing. It answers at
    /// any scale short of an `l` too large to store, and gives empty
    /// factors for a matrix without elements. When an entry of `a` is not
    /// finite (NaN or infinite), the factors are unspecified.
    #[must_use]
    pub fn of(a: MatrixRef<'_>) -> Self {
        let Qr { q, r } = Qr::of(a.adjoint().view());
        Self {
            l: r.view().adjoint(),
            q: q.view().adjoint(),
        }
    }
}

#[cfg(test)]
mod tests;
