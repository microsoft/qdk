// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of `NormedColumns` (`svd::jacobi::normed_columns`), on
//! hand-made columns.
//!
//! - The threshold below which a pair of columns needs no rotation. A pair
//!   is rotated only when the cosine of its angle,
//!   `|w_p† w_q| / (‖w_p‖·‖w_q‖)`, exceeds `√n · ε` for columns of length
//!   `n`: at the threshold, the pair is orthogonal; the product's modulus
//!   counts, not its real part; the threshold scales with both norms; a
//!   NaN column is never rotated.
//! - A squared norm updated by a rotation is kept until the update has lost
//!   half of it, then measured again (`updated_norm_sqr`).
//! - A rotation that leaves a column small keeps its measured norm, not
//!   the update, whose digits cancellation took.

use super::{NormedColumns, updated_norm_sqr};
use crate::Matrix;
use crate::svd::jacobi::SplitComplexColumns;
use crate::svd::jacobi::tests::split_columns;
use crate::test_utils::c;
use num_complex::Complex64;

/// Columns `w_p = a·e₀` and `w_q = b·(e₁ + z·e₀)` of length 4, whose
/// product `w_p† w_q = a·b·z` and norms `a` and `b` (`|z|²` is lost to
/// rounding) are exact for `a` and `b` powers of two. The threshold is
/// `√4 · ε = 2ε`, so the pair is rotated when `|z|` exceeds `2ε`. Each
/// case: `a`, `b`, `z`, and whether it is rotated:
///
/// - `3ε` is above the threshold, as `n · ε` (`4ε`) would not have it.
///   A threshold of `n · ε` left a degenerate pair of a 64 × 64 matrix
///   coupled by `30ε` unrotated, its values `15ε` off;
/// - `2ε` is at the threshold, so the pair is orthogonal;
/// - `3ε·i` is above: the product's modulus counts, not its real part;
/// - the threshold scales with both norms, `2³⁰` and `2⁻³⁰` either way
///   round, and both large or small;
/// - a NaN column is never rotated, so that the rotations end (and the
///   SVD's checks refuse the result).
#[test]
fn rotates_a_pair_only_above_the_threshold() {
    let eps = f64::EPSILON;
    let (large, small) = (2_f64.powi(30), 2_f64.powi(-30));
    let cases = [
        (1.0, 1.0, c(3.0 * eps, 0.0), true),
        (1.0, 1.0, c(2.0 * eps, 0.0), false),
        (1.0, 1.0, c(0.0, 3.0 * eps), true),
        (large, small, c(3.0 * eps, 0.0), true),
        (small, large, c(3.0 * eps, 0.0), true),
        (small, small, c(3.0 * eps, 0.0), true),
        (large, large, c(2.0 * eps, 0.0), false),
        (f64::NAN, 1.0, c(3.0 * eps, 0.0), false),
    ];
    for (a, b, z, rotated) in cases {
        let columns = two_columns(a, b, z);
        assert_eq!(
            columns.rotation_making_orthogonal(0, 1).is_some(),
            rotated,
            "a {a}, b {b}, z {z}"
        );
    }
}

/// `[a·e₀, b·(e₁ + z·e₀)]`, of length 4.
fn two_columns(a: f64, b: f64, z: Complex64) -> NormedColumns {
    let m = Matrix::from_fn(4, 2, |i, j| match (i, j) {
        (0, 0) => c(a, 0.0),
        (0, 1) => z * b,
        (1, 1) => c(b, 0.0),
        _ => c(0.0, 0.0),
    });
    NormedColumns::new(SplitComplexColumns::new(4, 2, split_columns(&m)))
}

/// A squared norm of 4 before a rotation. The update is kept while it is at
/// least half of that, 2, and replaced by the measured norm (here 7, to
/// tell it apart) below it, or when it is NaN. Each case: the update, and
/// the norm kept:
///
/// - 5, an increase, is kept;
/// - 2, exactly half, is kept;
/// - 1.9, below half, is measured again;
/// - NaN is measured again.
#[test]
fn measures_a_norm_again_once_its_update_has_lost_half_of_it() {
    let measured = 7.0;
    let cases = [
        (5.0, 5.0),
        (2.0, 2.0),
        (1.9, measured),
        (f64::NAN, measured),
    ];
    for (updated, kept) in cases {
        assert_eq!(
            updated_norm_sqr(4.0, updated, || measured),
            kept,
            "update {updated}"
        );
    }
}

/// Columns `w_p = e₀` and `w_q = e₀ + δ·e₁`, nearly parallel, for `δ` of
/// 1e-6, 1e-7 and 1e-8. Their Gram matrix `[[1, 1], [1, 1 + δ²]]` has the
/// eigenvalues `λ₊ = (2 + δ² + √(4 + δ⁴)) / 2` and `λ₋ = δ² / λ₊`
/// (their product is the determinant, `δ²`). The rotation that makes them
/// orthogonal leaves `w_p` of squared norm `λ₋`, about `δ²/2`, and `w_q` of
/// `λ₊`. The rotation moves a squared norm of about `1 − δ²/2` from `w_p`
/// to `w_q`, so the update of `w_p`'s, 1 less that, keeps only the digits
/// of `λ₋` above `ε`: none at 1e-8. The rotated `w_p`'s entries, about `δ`,
/// err by about `ε` each: its measured norm is within a relative `ε/δ`, at
/// most about 2e-8, of `λ₋`. The norms kept are within `1e-6` of `λ₋` and `λ₊`.
#[test]
fn a_rotation_that_leaves_a_column_small_keeps_its_measured_norm() {
    for delta in [1e-6, 1e-7, 1e-8] {
        let m = Matrix::from_fn(2, 2, |i, j| match (i, j) {
            (0, _) => c(1.0, 0.0),
            (1, 1) => c(delta, 0.0),
            _ => c(0.0, 0.0),
        });
        let mut columns = NormedColumns::new(SplitComplexColumns::new(2, 2, split_columns(&m)));
        let rotation = columns
            .rotation_making_orthogonal(0, 1)
            .expect("precondition: nearly parallel columns are coupled");
        columns.rotate(0, 1, &rotation);
        let pair = columns.pair(0, 1);
        let larger = (2.0 + delta * delta + (4.0 + delta.powi(4)).sqrt()) / 2.0;
        let smaller = delta * delta / larger;
        for (kept, expected) in [(pair.p_norm_sqr, smaller), (pair.q_norm_sqr, larger)] {
            assert!(
                (kept - expected).abs() <= 1e-6 * expected,
                "δ {delta}: kept {kept:e}, expected {expected:e}"
            );
        }
    }
}
