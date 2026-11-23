// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`checked`]: the checks every SVD answer passes before [`Svd::of`]
//! returns it.
//!
//! [`Svd::of`] answers no SVD unchecked (why: [`Svd`]'s module), the
//! rotations' included, whose answer passes through the kernels' QR and
//! products; so each answer is checked against the matrix, and
//! [`Svd::of`] fails with an [`SvdError`] rather than answer values that
//! are not finite, factors whose product is not the matrix, or singular
//! vectors that are not orthonormal.
//!
//! The bounds are those of LAPACK's SVD tests ([`TEST_RATIO`]), in the
//! Frobenius norm, and hold at any scale: the reconstruction is measured
//! relative to the matrix's largest entry. The checks cost three more
//! products, each at most of the matrix's size.
//!
//! [`Svd::of`]: crate::dense_linalg::Svd::of

use crate::dense_linalg::kernels::Kernels;
use crate::dense_linalg::matrix::{Matrix, MatrixRef};
use crate::dense_linalg::svd::{Svd, SvdError};

/// The SVD `answer` of `a` when it is one (the rotations report a failure
/// with `None`), its factors give back `a` ([`reconstructs`], which also
/// refuses factors that are not finite) and its singular vectors are
/// orthonormal ([`is_orthonormal`]); else [`SvdError::DidNotConverge`] for
/// `None`, [`SvdError::FailedChecks`] otherwise. The products are `K`'s.
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
/// [`is_orthonormal`]'s: LAPACK's SVD tests accept
/// `‖A − U·diag(S)·V†‖ / (‖A‖ · max(m, n) · ε)` and
/// `‖U†U − I‖ / (max(m, n) · ε)` below 50 (in the 1-norm; here the
/// Frobenius norm). A backward-stable SVD's errors grow with the size, and
/// the reconstruction's with the norm of `A`.
const TEST_RATIO: f64 = 50.0;

/// Whether `svd`'s factors give back `a`: `‖A − U·diag(S)·V†‖_F` at most
/// [`TEST_RATIO`] `· max(m, n) · ε · ‖A‖_F`; never when a factor
/// is not finite, since the error is then NaN or infinite. Both sides are
/// computed on `A` and `S` divided by `A`'s largest entry, the largest of
/// its real and imaginary parts, so that the squares neither underflow to
/// 0 nor overflow, where the test would take any factors.
fn reconstructs<K: Kernels>(a: MatrixRef<'_>, svd: &Svd) -> bool {
    let (rows, cols) = (a.rows(), a.cols());
    let rank = svd.singular_values.len();
    let largest = a
        .elements()
        .iter()
        .fold(0.0, |m: f64, x| m.max(x.re.abs()).max(x.im.abs()));
    let scale = if largest > 0.0 { largest } else { 1.0 };
    let u = svd.u.elements();
    let us = Matrix::from_fn(rows, rank, |i, j| {
        u[i + rows * j] * (svd.singular_values[j] / scale)
    });
    let product = K::product(us.view(), svd.v_adjoint.view());
    let error = a
        .elements()
        .iter()
        .zip(product.elements())
        .map(|(x, y)| (x / scale - y).norm_sqr())
        .sum::<f64>()
        .sqrt();
    let norm = a
        .elements()
        .iter()
        .map(|x| (x / scale).norm_sqr())
        .sum::<f64>()
        .sqrt();
    #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
    let size = rows.max(cols) as f64;
    error <= TEST_RATIO * size * f64::EPSILON * norm
}

/// Whether `svd`'s singular vectors, of `a`, are orthonormal: `‖U†U − I‖_F`
/// and `‖V†V − I‖_F` each at most [`TEST_RATIO`] `· max(m, n) · ε`;
/// never when a vector is not finite.
fn is_orthonormal<K: Kernels>(a: MatrixRef<'_>, svd: &Svd) -> bool {
    #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
    let size = a.rows().max(a.cols()) as f64;
    let bound = TEST_RATIO * size * f64::EPSILON;
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
