// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The crate's tests, one file per module, and the helpers they share:
//! deterministic matrices and naive products and adjoints, written apart
//! from the code they check.

mod kernels;
mod matrix;
mod qr;
mod svd;

use crate::{Matrix, MatrixRef, Svd};
use num_complex::Complex64;

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

/// A deterministic pseudo-random `rows × cols` matrix.
fn matrix(rows: usize, cols: usize, seed: u64) -> Matrix {
    let mut state = seed;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        #[expect(clippy::cast_precision_loss, reason = "a test value")]
        let value = (state >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
        value
    };
    Matrix::from_fn(rows, cols, |_, _| c(next(), next()))
}

fn naive_product(a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix {
    Matrix::from_fn(a.rows(), b.cols(), |i, j| {
        (0..a.cols())
            .map(|k| a.elements()[i + a.rows() * k] * b.elements()[k + b.rows() * j])
            .sum()
    })
}

fn distance(a: &Matrix, b: &Matrix) -> f64 {
    assert_eq!((a.rows(), a.cols()), (b.rows(), b.cols()));
    a.elements()
        .iter()
        .zip(b.elements())
        .map(|(x, y)| (x - y).norm())
        // Not `f64::max`, which drops a NaN: a NaN entry must fail the
        // caller's `< tolerance` check.
        .fold(0.0, |m, d| {
            if m.is_nan() || d.is_nan() {
                f64::NAN
            } else {
                m.max(d)
            }
        })
}

fn adjoint(m: &Matrix) -> Matrix {
    let e = m.elements();
    Matrix::from_fn(m.cols(), m.rows(), |i, j| e[j + m.rows() * i].conj())
}

fn identity(n: usize) -> Matrix {
    Matrix::from_fn(n, n, |i, j| c(if i == j { 1.0 } else { 0.0 }, 0.0))
}

/// Tall, wide, square and single-row shapes.
const SHAPES: [(usize, usize); 4] = [(6, 4), (4, 6), (5, 5), (1, 3)];

/// Checks that [`Svd::of`] gives a thin SVD of `a`, as [`check_svd_of`]
/// does, and returns its values.
fn check_svd(a: &Matrix) -> Vec<f64> {
    check_svd_of(a, Svd::of(a.view()).expect("SVD"))
}

/// Checks that `svd` is a thin SVD of `a`, with `r` values,
/// `1 ≤ r ≤ min(m, n)`, and returns them.
fn check_svd_of(a: &Matrix, svd: Svd) -> Vec<f64> {
    let (rows, cols) = (a.rows(), a.cols());
    let Svd {
        u,
        singular_values,
        v_adjoint,
    } = svd;
    let r = singular_values.len();
    assert!((1..=rows.min(cols)).contains(&r), "{r} values");
    assert!(singular_values.windows(2).all(|w| w[0] >= w[1]));
    let us = Matrix::from_fn(rows, r, |i, j| {
        u.elements()[i + rows * j] * singular_values[j]
    });
    assert!(distance(&naive_product(us.view(), v_adjoint.view()), a) < 1e-13);
    assert!(distance(&naive_product(adjoint(&u).view(), u.view()), &identity(r)) < 1e-13);
    let rows_gram = naive_product(v_adjoint.view(), adjoint(&v_adjoint).view());
    assert!(distance(&rows_gram, &identity(r)) < 1e-13);
    singular_values
}

/// `m` times `2^exponent`, in two steps so that each factor is finite;
/// rounded where the product is subnormal, exact otherwise.
fn times_power_of_two(m: &Matrix, exponent: i32) -> Matrix {
    let factors = [
        2_f64.powi(exponent / 2),
        2_f64.powi(exponent - exponent / 2),
    ];
    let elements = m.elements().iter();
    let scaled = elements.map(|x| x * factors[0] * factors[1]).collect();
    Matrix::new(m.rows(), m.cols(), scaled)
}
