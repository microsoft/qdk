// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the thin QR (`qr`).

use super::{SHAPES, adjoint, c, distance, identity, matrix, naive_product, times_power_of_two};
use crate::dense_linalg::{Matrix, Qr};

/// Checks that `Qr::of(a)` gives back `a` to rounding of its largest entry,
/// with orthonormal columns in `Q` and `R` upper trapezoidal.
fn check_qr(a: &Matrix) {
    let (rows, cols) = (a.rows(), a.cols());
    let Qr { q, r } = Qr::of(a.view());
    let k = rows.min(cols);
    assert_eq!((q.rows(), q.cols(), r.rows(), r.cols()), (rows, k, k, cols));
    let largest = a.elements().iter().map(|x| x.norm()).fold(0.0, f64::max);
    let error = distance(&naive_product(q.view(), r.view()), a);
    assert!(error <= 1e-13 * largest, "{error:e} of {largest:e}");
    let gram = naive_product(adjoint(&q).view(), q.view());
    assert!(distance(&gram, &identity(k)) < 1e-13);
    for j in 0..cols {
        for i in j + 1..k {
            assert_eq!(r.elements()[i + k * j], c(0.0, 0.0), "R[{i}, {j}]");
        }
    }
}

/// A thin QR gives back its matrix, with orthonormal columns in `Q` and
/// `R` upper trapezoidal, on tall, wide, square and single-row shapes.
#[test]
fn decomposes_by_qr() {
    for (seed, &(rows, cols)) in (1..).zip(&SHAPES) {
        check_qr(&matrix(rows, cols, seed));
    }
}

/// The zero matrix, which has no largest entry to scale by, decomposes
/// with `R = 0`.
#[test]
fn decomposes_the_zero_matrix() {
    for &(rows, cols) in &SHAPES {
        check_qr(&Matrix::new(rows, cols, vec![c(0.0, 0.0); rows * cols]));
    }
}

/// A matrix without elements has empty factors of the thin shapes, even
/// with a dimension of `2⁶³` (`2³¹` on 32 bits), which no matrix with
/// elements can have.
#[test]
fn decomposes_an_empty_matrix_of_any_size() {
    let huge = 1 << (usize::BITS - 1);
    for (rows, cols) in [(huge, 0), (0, huge), (3, 0), (0, 3)] {
        let Qr { q, r } = Qr::of(Matrix::new(rows, cols, Vec::new()).view());
        assert_eq!((q.rows(), q.cols(), r.rows(), r.cols()), (rows, 0, 0, cols));
    }
}

/// A thin QR decomposes matrices at the extremes of the `f64` range, as
/// it scales by the largest entry: `[2⁻¹⁰²³]`, a subnormal number, is its
/// own decomposition, `[1]·[2⁻¹⁰²³]` to the sign (unscaled, the
/// reflection's squared norm underflows, and the QR answered `R = [0]`);
/// and matrices whose entries are all subnormal, about `2⁻¹⁰²⁶`, or about
/// `2¹⁰¹⁹`, whose squares overflow, on tall, wide, square and single-row
/// shapes.
#[test]
fn decomposes_matrices_at_extreme_scales() {
    check_qr(&Matrix::new(1, 1, vec![c(f64::MIN_POSITIVE / 2.0, 0.0)]));
    for exponent in [-1025, 1020] {
        for (seed, &(rows, cols)) in (1..).zip(&SHAPES) {
            check_qr(&times_power_of_two(&matrix(rows, cols, seed), exponent));
        }
    }
}

/// A matrix with an entry that is not finite has no QR decomposition, and
/// its factors are unspecified; the QR still answers, with the thin
/// shapes, rather than panic.
#[test]
fn an_entry_that_is_not_finite_gives_thin_factors_not_a_panic() {
    for x in [f64::NAN, f64::INFINITY] {
        let mut elements = matrix(4, 3, 1).into_elements();
        elements[5] = c(x, 0.0);
        let Qr { q, r } = Qr::of(Matrix::new(4, 3, elements).view());
        assert_eq!((q.rows(), q.cols(), r.rows(), r.cols()), (4, 3, 3, 3));
    }
}
