// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the QR algorithm on the real bidiagonal that nalgebra's
//! fast route runs ([`bidiagonal_qr`]), on the bidiagonals where general
//! QR algorithms stall or lose the small values.

use super::super::{adjoint, distance, identity, naive_product};
use crate::Matrix;
use crate::kernels::nalgebra::bidiagonal_qr::bidiagonal_qr;
use num_complex::Complex64;

/// The QR algorithm on the bidiagonal with diagonal `diagonal` and
/// superdiagonal `upper`, from identity factors: checks that it converges,
/// that the values are nonnegative and nonincreasing, that the factors are
/// orthogonal and their product is the bidiagonal, and returns the values.
fn check_bidiagonal_qr(diagonal: &[f64], upper: &[f64]) -> Vec<f64> {
    let size = diagonal.len();
    let unit = || {
        let mut m = vec![0.0; size * size];
        for i in 0..size {
            m[i + size * i] = 1.0;
        }
        m
    };
    let (mut values, mut above, mut u, mut v) = (diagonal.to_vec(), upper.to_vec(), unit(), unit());
    assert!(
        bidiagonal_qr(&mut values, &mut above, &mut u, &mut v),
        "converges"
    );
    assert!(
        values.windows(2).all(|w| w[0] >= w[1]) && values[size - 1] >= 0.0,
        "nonnegative and nonincreasing: {values:?}"
    );
    let real = |m: &[f64]| Matrix::from_fn(size, size, |i, j| Complex64::new(m[i + size * j], 0.0));
    let (u, v) = (real(&u), real(&v));
    let bidiagonal = Matrix::from_fn(size, size, |i, j| {
        let x = if i == j {
            diagonal[i]
        } else if j == i + 1 {
            upper[i]
        } else {
            0.0
        };
        Complex64::new(x, 0.0)
    });
    let sigma = Matrix::from_fn(size, size, |i, j| {
        Complex64::new(if i == j { values[i] } else { 0.0 }, 0.0)
    });
    let product = naive_product(
        naive_product(u.view(), sigma.view()).view(),
        adjoint(&v).view(),
    );
    assert!(
        distance(&product, &bidiagonal) < 1e-15 * values[0].max(1.0),
        "{product:?}"
    );
    for m in [&u, &v] {
        let gram = naive_product(adjoint(m).view(), m.view());
        assert!(distance(&gram, &identity(size)) < 1e-14, "{gram:?}");
    }
    values
}

/// Two values one unit in the last place apart, coupled by an
/// off-diagonal of ε, where a deflation test without a safety factor never
/// deflates and the iteration only flips the off-diagonal's sign: it
/// converges, and gives the pair to rounding.
#[test]
fn converges_on_a_degenerate_pair_one_ulp_apart() {
    let (d, d_next) = (9.282_350_173_388_935e-1, 9.282_350_173_388_936e-1);
    let values = check_bidiagonal_qr(&[1.0, d, d_next], &[0.0, f64::EPSILON]);
    for (value, expected) in values.iter().zip([1.0, d_next, d]) {
        assert!((value - expected).abs() <= 2.0 * f64::EPSILON, "{values:?}");
    }
}

/// Bidiagonals whose values grow down the diagonal, which the QR algorithm
/// chases from the bottom up: mildly graded, where it shifts, and graded
/// over five orders of magnitude, where a shift would spoil the small
/// values and it takes the zero shift. Both give orthogonal factors whose
/// product is the bidiagonal.
#[test]
fn decomposes_bidiagonals_graded_upward() {
    check_bidiagonal_qr(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[0.5; 5]);
    check_bidiagonal_qr(
        &[1e-5, 1e-4, 1e-3, 1e-2, 1e-1, 1.0],
        &[1e-5, 1e-4, 1e-3, 1e-2, 1e-1],
    );
}
