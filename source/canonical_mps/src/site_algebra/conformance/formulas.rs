// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The contract's formulas the cases compare with:
//!
//! - the gated pair `θ` and a gated site, each by the formula's own loops;
//! - `θ`'s singular values, and the fractions of their squares kept and
//!   dropped;
//! - the distances from a product and from an isometry, and the
//!   normalisation they are measured after.
//!
//! The singular values come from the crate's own SVD, the one the CPU
//! algebra splits with. So on the CPU algebra, a fault in that SVD would be
//! shared by the algebra and the kit. The SVD's own tests check it against
//! values known in closed form.

use super::inputs::Pair;
use dense_linalg::{Matrix, Svd, norm};
use num_complex::Complex64;

/// `θ[a, s, t, b] = Σ G[(s, t), (p, q)] · A[a, p, j] · B[j, q, b]`, as
/// [`GateSplitCall`](crate::site_algebra::GateSplitCall) defines it. It is
/// the `(2·l) × (2·r)` matrix of rows `a + l·s` and columns `t + 2·b`:
///
/// ```text
///   a ── A ── j ── B ── b      A: [l, 2, m]   B: [m, 2, r]
///        │         │
///        p         q
///      ┌─┴─────────┴─┐
///      │      G      │         G: 4 × 4, row (s, t), column (p, q)
///      └─┬─────────┬─┘
///        s         t
///
///   θ: row a + l·s, column t + 2·b; the bond j and p, q summed
/// ```
#[expect(clippy::many_single_char_names, reason = "the formula's indices")]
pub(super) fn gated_pair(pair: &Pair, gate: &[Complex64; 16]) -> Matrix {
    let (l, m) = (pair.l, pair.m);
    let left_site = |a: usize, p: usize, j: usize| pair.left[a + l * (p + 2 * j)];
    let right_site = |j: usize, q: usize, b: usize| pair.right[j + m * (q + 2 * b)];
    Matrix::from_fn(2 * l, 2 * pair.r, |row, col| {
        let (a, s, t, b) = (row % l, row / l, col % 2, col / 2);
        let mut sum = Complex64::ZERO;
        for p in 0..2 {
            for q in 0..2 {
                for j in 0..m {
                    sum += gate[(2 * s + t) * 4 + 2 * p + q]
                        * left_site(a, p, j)
                        * right_site(j, q, b);
                }
            }
        }
        sum
    })
}

/// The site `x[a, p, b]`, of extents `[left, 2, right]`, with the one-site
/// `gate` (`G[s, p]` at `2s + p`) applied, as
/// [`QrMoveCall`](crate::site_algebra::QrMoveCall) defines it.
pub(super) fn gated_site(
    x: &[Complex64],
    left: usize,
    right: usize,
    gate: &[Complex64; 4],
) -> Vec<Complex64> {
    (0..right)
        .flat_map(|b| (0..2).flat_map(move |s| (0..left).map(move |a| (a, s, b))))
        .map(|(a, s, b)| {
            (0..2)
                .map(|p| gate[2 * s + p] * x[a + left * (p + 2 * b)])
                .sum()
        })
        .collect()
}

/// `θ`'s singular values, nonincreasing, those of rounding's size left
/// out. Or why the kit's own SVD failed (on the kit's inputs, it does
/// not).
pub(super) fn singular_values(theta: &Matrix) -> Result<Vec<f64>, String> {
    Svd::of(theta.view())
        .map(|svd| svd.singular_values)
        .map_err(|error| format!("the kit's SVD of θ failed: {error}"))
}

/// The fraction of the squares of `values` in their first `k`. It is 1
/// when `k` holds them all.
pub(super) fn retained_fraction_of(values: &[f64], k: usize) -> f64 {
    1.0 - dropped_fraction_of(values, k)
}

/// The fraction of the squares of `values` past their first `k`. It is 0
/// when `k` holds them all.
pub(super) fn dropped_fraction_of(values: &[f64], k: usize) -> f64 {
    let total: f64 = values.iter().map(|s| s * s).sum();
    let dropped: f64 = values.iter().skip(k).map(|s| s * s).sum();
    dropped / total
}

/// The Frobenius norm of `x − expected` over that of `expected`. It is
/// infinite when a difference is not finite, so that no tolerance accepts
/// it.
pub(super) fn relative_distance(x: &[Complex64], expected: &[Complex64]) -> f64 {
    let difference = x
        .iter()
        .zip(expected)
        .map(|(x, y)| (x - y).norm_sqr())
        .sum::<f64>()
        .sqrt();
    or_infinite(difference / norm(expected))
}

/// `|x − expected|`. It is infinite when it is not finite, so that no
/// tolerance accepts a NaN.
pub(super) fn distance(x: f64, expected: f64) -> f64 {
    or_infinite((x - expected).abs())
}

/// `d`, or infinity when `d` is not finite. A NaN compared with a
/// tolerance is never above it. So every distance the cases compare with a
/// tolerance goes through this.
fn or_infinite(d: f64) -> f64 {
    if d.is_finite() { d } else { f64::INFINITY }
}

/// `matrix` divided by its Frobenius norm.
pub(super) fn normalised(matrix: &Matrix) -> Matrix {
    let frobenius = norm(matrix.elements());
    let elements = matrix.elements().iter().map(|x| x / frobenius).collect();
    Matrix::new(matrix.rows(), matrix.cols(), elements)
}

/// The largest entry of `m†·m − I`. It is zero when `m`'s columns are
/// orthonormal.
pub(super) fn columns_isometry_error(m: &Matrix) -> f64 {
    identity_error(&Matrix::product(m.view().adjoint().view(), m.view()))
}

/// The largest entry of `m·m† − I`. It is zero when `m`'s rows are
/// orthonormal.
pub(super) fn rows_isometry_error(m: &Matrix) -> f64 {
    identity_error(&Matrix::product(m.view(), m.view().adjoint().view()))
}

/// The largest entry of the square `g − I`. It is infinite when an entry
/// is not finite.
fn identity_error(g: &Matrix) -> f64 {
    let n = g.rows();
    (0..n)
        .flat_map(|i| (0..n).map(move |j| (i, j)))
        .map(|(i, j)| {
            let one = if i == j { 1.0 } else { 0.0 };
            or_infinite((g.elements()[i + n * j] - one).norm())
        })
        .fold(0.0, f64::max)
}
