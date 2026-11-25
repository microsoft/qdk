// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Kernels`]: the steps that set the crate's speed. They sit behind one
//! trait, so replacing their implementation changes none of the SVD's
//! rules.
//!
//! The SVD's rules are the crate's own, computed once on top of these
//! steps: the checks, the trim's choice of rows, the scaling (the thin
//! QR's too) and the one-sided Jacobi rotations. The kernels bring:
//!
//! - the product;
//! - the thin QR;
//! - the column-pivoted QR, and the application of its `Q`.
//!
//! [`DefaultKernels`] is the implementation the public API computes with.

mod matrixmultiply_householder;

use crate::matrix::{Matrix, MatrixRef};
use crate::qr::Qr;
use num_complex::Complex64;

/// The implementation of [`Kernels`] the public API computes with.
pub(crate) type DefaultKernels = self::matrixmultiply_householder::MatrixmultiplyHouseholder;

/// The steps that set the crate's speed, each on the calling thread.
pub(crate) trait Kernels {
    /// What [`Kernels::q_kept_times`] needs of a column-pivoted QR to apply
    /// its `Q` (for instance, its reflectors).
    type Q;

    /// The product `a·b`. `a.cols()` equals `b.rows()`, and the product's
    /// element count fits `usize`.
    fn product(a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix;

    /// Up to rounding of `a`'s largest entry, the thin QR decomposition of
    /// `a`. It holds at the scales [`Qr::of`]'s scaling leaves: entries far
    /// below the largest may be rounded to 0. `a` has elements, so its
    /// dimensions are at most its element count. [`Qr::of`] answers an
    /// empty matrix itself.
    fn qr(a: MatrixRef<'_>) -> Qr;

    /// Up to rounding, the thin column-pivoted QR decomposition `a·P = Q·R`
    /// of `a`. It holds at the scales the SVD's scaling leaves
    /// ([`PowerOfTwoScaling`]): entries far below the largest may be
    /// rounded to 0. The pivot of each step is the remaining column of
    /// largest norm. So, to within about `√ε/2` of the pivot (the allowance
    /// a norm downdate may need), `|R_kk| ≥ ‖R[k.., j]‖` for every `j > k`.
    /// The SVD's trim reads the rank from `R`. `a` has elements.
    ///
    /// [`PowerOfTwoScaling`]: crate::scaling::PowerOfTwoScaling
    fn pivoted_qr(a: MatrixRef<'_>) -> PivotedQr<Self::Q>;

    /// `Q_kept·x`, column-major, with as many rows as `qr`'s `A`. `Q_kept`
    /// is the columns `kept` (increasing) of `qr`'s `Q`. `x` is
    /// `kept.len() × rank`, column-major.
    fn q_kept_times(
        qr: &PivotedQr<Self::Q>,
        kept: &[usize],
        x: &[Complex64],
        rank: usize,
    ) -> Vec<Complex64>;
}

/// A thin column-pivoted QR decomposition `A·P = Q·R` of a `rows × cols`
/// matrix `A`. `R` is `min(rows, cols) × cols`, upper trapezoidal (upper
/// triangular when square or tall).
pub(crate) struct PivotedQr<Q> {
    /// `R`.
    pub(crate) r: Matrix,
    /// The permutation. Column `j` of `R` is column `forward[j]` of `A`,
    /// so `(R·Pᵀ)[i, forward[j]] = R[i, j]`.
    pub(crate) forward: Vec<usize>,
    /// What [`Kernels::q_kept_times`] needs of `Q`.
    pub(crate) q: Q,
}

#[cfg(test)]
mod tests;
