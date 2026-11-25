// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`checked`]: the checks every SVD answer passes before [`Svd::of`]
//! returns it.
//!
//! [`Svd::of`] answers no SVD unchecked ([`Svd`]'s module says why). That
//! includes the rotations' answer, which passes through the kernels' QR and
//! products. So each answer is checked against the matrix. [`Svd::of`]
//! fails with an [`SvdError`] rather than answer any of these:
//!
//! - values that are not finite;
//! - factors whose product is not the matrix;
//! - singular vectors that are not orthonormal.
//!
//! The bounds are those of LAPACK's SVD tests ([`TEST_RATIO`]), in the
//! Frobenius norm (`docs/algorithms.md`'s Terms). They hold at any scale:
//! the reconstruction is measured relative to the matrix's largest entry.
//! The checks cost three more products, each at most of the matrix's size.
//!
//! [`Svd::of`]: crate::Svd::of

use crate::kernels::Kernels;
use crate::matrix::{Matrix, MatrixRef};
use crate::norms::norm_divided_by;
use crate::scaling::largest_part;
use crate::svd::{Svd, SvdError};

/// The SVD `answer` of `a`, when all three hold:
///
/// - it is one (the rotations report a failure with `None`);
/// - its factors give back `a` ([`reconstructs`], which also refuses
///   factors that are not finite);
/// - its singular vectors are orthonormal ([`is_orthonormal`]).
///
/// Else [`SvdError::DidNotConverge`] for `None`, and
/// [`SvdError::FailedChecks`] otherwise. The products are `K`'s.
pub(crate) fn checked<K: Kernels>(a: MatrixRef<'_>, answer: Option<Svd>) -> Result<Svd, SvdError> {
    let (rows, cols) = (a.rows(), a.cols());
    let svd = answer.ok_or(SvdError::DidNotConverge { rows, cols })?;
    if reconstructs::<K>(a, &svd) && is_orthonormal::<K>(a, &svd) {
        Ok(svd)
    } else {
        Err(SvdError::FailedChecks { rows, cols })
    }
}

/// The bound on the test ratios of an SVD, [`reconstructs`]'s and
/// [`is_orthonormal`]'s. LAPACK's SVD tests accept
/// `‖A − U·diag(S)·V†‖ / (‖A‖ · max(m, n) · ε)` and
/// `‖U†U − I‖ / (max(m, n) · ε)` below 50 (in the 1-norm; here the
/// Frobenius norm). A backward-stable SVD is the exact SVD of a matrix
/// within rounding of `A`. Its errors grow with the matrix's size, and its
/// reconstruction error grows with `A`'s norm. So the bounds scale with
/// both.
const TEST_RATIO: f64 = 50.0;

/// Whether `svd`'s factors give back `a`: `‖A − U·diag(S)·V†‖_F` at most
/// [`TEST_RATIO`] `· max(m, n) · ε · ‖A‖_F`. Never when a factor is not
/// finite, since the error is then NaN or infinite. Both sides are
/// computed on `A` and `S` divided by `A`'s largest entry, the largest of
/// its real and imaginary parts. So `‖A‖_F²`, divided, is at most
/// `2·m·n`, and at least 1 unless `A` is zero: it neither underflows to 0
/// nor overflows, where the test would refuse or take any factors.
fn reconstructs<K: Kernels>(a: MatrixRef<'_>, svd: &Svd) -> bool {
    let scale = largest_part_or_one(a);
    let us = u_times_values(svd, scale);
    let product = K::product(us.view(), svd.v_adjoint.view());
    let error = distance(a, &product, scale);
    let norm = norm_divided_by(a.elements(), scale);
    error <= rounding_bound(a) * norm
}

/// The largest of the real and imaginary parts of `a`'s entries, in
/// magnitude, or 1 for the zero matrix.
fn largest_part_or_one(a: MatrixRef<'_>) -> f64 {
    let largest = largest_part(a.elements());
    if largest > 0.0 { largest } else { 1.0 }
}

/// `U·diag(S / scale)`, `svd`'s left singular vectors times its singular
/// values divided by `scale`.
fn u_times_values(svd: &Svd, scale: f64) -> Matrix {
    let (rows, rank) = (svd.u.rows(), svd.singular_values.len());
    let u = svd.u.elements();
    Matrix::from_fn(rows, rank, |i, j| {
        u[i + rows * j] * (svd.singular_values[j] / scale)
    })
}

/// `‖A / scale − product‖_F`.
fn distance(a: MatrixRef<'_>, product: &Matrix, scale: f64) -> f64 {
    a.elements()
        .iter()
        .zip(product.elements())
        .map(|(x, y)| (x / scale - y).norm_sqr())
        .sum::<f64>()
        .sqrt()
}

/// [`TEST_RATIO`] `· max(m, n) · ε` for the `m × n` matrix `a`: the
/// relative error the checks allow.
fn rounding_bound(a: MatrixRef<'_>) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
    let size = a.rows().max(a.cols()) as f64;
    TEST_RATIO * size * f64::EPSILON
}

/// Whether `svd`'s singular vectors, of `a`, are orthonormal: `‖U†U − I‖_F`
/// and `‖V†V − I‖_F` each at most [`TEST_RATIO`] `· max(m, n) · ε`. Never
/// when a vector is not finite.
fn is_orthonormal<K: Kernels>(a: MatrixRef<'_>, svd: &Svd) -> bool {
    let bound = rounding_bound(a);
    let u = svd.u.view();
    let v_adjoint = svd.v_adjoint.view();
    distance_from_identity(&K::product(u.adjoint().view(), u)) <= bound
        && distance_from_identity(&K::product(v_adjoint, v_adjoint.adjoint().view())) <= bound
}

/// `‖G − I‖_F` of the square matrix `g`.
fn distance_from_identity(g: &Matrix) -> f64 {
    let n = g.rows();
    g.elements()
        .iter()
        .enumerate()
        .map(|(at, x)| {
            let one = if at % n == at / n { 1.0 } else { 0.0 };
            (x - one).norm_sqr()
        })
        .sum::<f64>()
        .sqrt()
}

#[cfg(test)]
mod tests;
