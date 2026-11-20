// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Kernels`]: the steps that set the module's speed, behind one trait so
//! that replacing their implementation changes none of the SVD's rules.
//!
//! The SVD's rules are the module's own and computed once, on top of these
//! steps: the checks, the trim's choice of rows, the scaling (the thin
//! QR's too) and the one-sided Jacobi rotations. The kernels bring the
//! product, the thin QR, and the column-pivoted QR and the application of
//! its `Q`.
//!
//! [`DefaultKernels`] is the implementation the public API computes with.

mod nalgebra_matrixmultiply;

use crate::dense_linalg::matrix::{Matrix, MatrixRef};
use crate::dense_linalg::qr::Qr;
use num_complex::Complex64;

/// The implementation of [`Kernels`] the public API computes with.
pub(crate) type DefaultKernels = self::nalgebra_matrixmultiply::NalgebraMatrixmultiply;

/// The steps that set the module's speed, each on the calling thread.
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

    /// The column-pivoted QR decomposition `a·P = Q·R` of `a`, thin, to
    /// rounding, at the scales the SVD's scaling leaves
    /// ([`PowerOfTwoScaling`]): entries far below the largest may be rounded
    /// to 0. The pivot of each step is the remaining column of largest norm,
    /// so that `|R_kk| ≥ ‖R[k.., j]‖` for every `j > k`, to rounding: the
    /// SVD's trim reads the rank from `R`. `a` has elements.
    ///
    /// [`PowerOfTwoScaling`]: crate::dense_linalg::scaling::PowerOfTwoScaling
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
