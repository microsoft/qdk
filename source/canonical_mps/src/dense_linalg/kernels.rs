// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Kernels`]: the steps of the module's computations that a linear
//! algebra library provides, so that the library is one file.
//!
//! The SVD's rules are the module's own and computed once, on top of these
//! steps: the checks, the trim's choice of rows, the scaling (the thin
//! QR's too), the one-sided Jacobi rotations and the rule that routes a
//! matrix to the fast route. A library brings only what it computes faster
//! or better than code written here: the product, the thin QR, the
//! column-pivoted QR and the application of its `Q`, and the fast route's
//! SVD (`kernels/t4a_faer.rs`'s header gives how much faster).
//!
//! [`DefaultKernels`] is the implementation the public API computes with.

mod t4a_faer;

use crate::dense_linalg::matrix::{Matrix, MatrixRef};
use crate::dense_linalg::qr::Qr;
use num_complex::Complex64;

/// The implementation of [`Kernels`] the public API computes with.
pub(crate) type DefaultKernels = self::t4a_faer::T4aFaer;

/// An SVD as column-major arrays, `(U, S, V)` with `W = U·diag(S)·V†`,
/// the values nonincreasing: `U` is `rows × r` and `V` is `cols × r`,
/// `W` being `rows × cols`. Both routes answer one: the fast route
/// ([`Kernels::tall_svd`]) and the rotations.
pub(crate) type ColumnMajorSvd = (Vec<Complex64>, Vec<f64>, Vec<Complex64>);

/// The steps a linear algebra library computes for the module, each on the
/// calling thread.
pub(crate) trait Kernels {
    /// What [`Kernels::q_kept_times`] needs of a column-pivoted QR to apply
    /// its `Q` (for instance, its reflectors).
    type Q;

    /// The product `a·b`; `a.cols()` equals `b.rows()`, and the product's
    /// element count fits `usize`.
    fn product(a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix;

    /// The thin QR decomposition of `a`, to rounding, at the scales the
    /// library handles ([`Qr::of`] scales the others). `a` has elements, so
    /// its dimensions are at most its element count: [`Qr::of`] answers an
    /// empty matrix itself.
    fn qr(a: MatrixRef<'_>) -> Qr;

    /// The column-pivoted QR decomposition `a·P = Q·R` of `a`, thin.
    fn pivoted_qr(a: MatrixRef<'_>) -> PivotedQr<Self::Q>;

    /// `Q_kept·x`, column-major, as many rows as `qr`'s `A`: `Q_kept` the
    /// columns `kept` (increasing) of `qr`'s `Q`, `x` `kept.len() × rank`,
    /// column-major.
    fn q_kept_times(
        qr: &PivotedQr<Self::Q>,
        kept: &[usize],
        x: &[Complex64],
        rank: usize,
    ) -> Vec<Complex64>;

    /// The fast route's SVD: `w = U·diag(S)·V†` of the `rows × cols`
    /// matrix `w`, column-major, `rows ≥ cols ≥ 1`, with one value per
    /// column; or `None` when it reports a failure. Its factors are not
    /// trusted: the SVD checks them, and factors `w` by the rotations
    /// when they are not finite.
    fn tall_svd(rows: usize, cols: usize, w: &[Complex64]) -> Option<ColumnMajorSvd>;
}

/// A column-pivoted QR decomposition `A·P = Q·R` of a `rows × cols`
/// matrix `A`, thin: `R` is `min(rows, cols) × cols`, upper trapezoidal
/// (upper triangular when square or tall).
pub(crate) struct PivotedQr<Q> {
    /// `R`.
    pub(crate) r: Matrix,
    /// The permutation: column `j` of `R` is column `forward[j]` of `A`,
    /// so that `(R·Pᵀ)[i, forward[j]] = R[i, j]`.
    pub(crate) forward: Vec<usize>,
    /// What [`Kernels::q_kept_times`] needs of `Q`.
    pub(crate) q: Q,
}
