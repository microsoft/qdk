// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of [`LinearAlgebra`]'s contract on `CpuLinearAlgebra`, the one
//! implementation: products, a thin QR and a thin SVD that give back their
//! matrix, with orthonormal factors, including on the rank-deficient
//! matrices splits meet. The tests of the SVD's own guarantees are in
//! `svd`, mirroring the `linear_algebra::cpu::svd` module; the helpers
//! here serve both.

mod svd;

use crate::linear_algebra::{CpuLinearAlgebra, LinearAlgebra, Matrix, MatrixRef, Qr, Svd};
use num_complex::Complex64;

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
    Matrix::from_fn(rows, cols, |_, _| Complex64::new(next(), next()))
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
    a.view()
        .elements()
        .iter()
        .zip(b.view().elements())
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
    let e = m.view().elements();
    Matrix::from_fn(m.cols(), m.rows(), |i, j| e[j + m.rows() * i].conj())
}

fn identity(n: usize) -> Matrix {
    Matrix::from_fn(n, n, |i, j| {
        Complex64::new(if i == j { 1.0 } else { 0.0 }, 0.0)
    })
}

/// Tall, wide, square and single-row shapes.
const SHAPES: [(usize, usize); 4] = [(6, 4), (4, 6), (5, 5), (1, 3)];

/// The product of two matrices is the naive product, to rounding.
#[test]
fn multiplies() {
    let algebra = &CpuLinearAlgebra;
    for (seed, &(rows, cols)) in (1..).zip(&SHAPES) {
        let a = matrix(rows, cols, seed);
        let b = matrix(cols, 3, seed + 10);
        let product = algebra.multiply(a.view(), b.view());
        assert!(distance(&product, &naive_product(a.view(), b.view())) < 1e-14);
    }
}

/// A thin QR gives back its matrix, with orthonormal columns in Q.
#[test]
fn decomposes_by_qr() {
    let algebra = &CpuLinearAlgebra;
    for (seed, &(rows, cols)) in (1..).zip(&SHAPES) {
        let a = matrix(rows, cols, seed);
        let Qr { q, r } = algebra.qr(a.view()).expect("QR");
        let k = rows.min(cols);
        assert_eq!((q.rows(), q.cols(), r.rows(), r.cols()), (rows, k, k, cols));
        assert!(distance(&naive_product(q.view(), r.view()), &a) < 1e-13);
        let gram = naive_product(adjoint(&q).view(), q.view());
        assert!(distance(&gram, &identity(k)) < 1e-13);
    }
}

/// Checks that `CpuLinearAlgebra` gives a thin SVD of `a`, as
/// [`check_svd_of`] does, and returns its values.
fn check_svd(a: &Matrix) -> Vec<f64> {
    check_svd_of(a, CpuLinearAlgebra.svd(a.view()).expect("SVD"))
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
        u.view().elements()[i + rows * j] * singular_values[j]
    });
    assert!(distance(&naive_product(us.view(), v_adjoint.view()), a) < 1e-13);
    assert!(distance(&naive_product(adjoint(&u).view(), u.view()), &identity(r)) < 1e-13);
    let rows_gram = naive_product(v_adjoint.view(), adjoint(&v_adjoint).view());
    assert!(distance(&rows_gram, &identity(r)) < 1e-13);
    singular_values
}

/// A thin SVD gives back its matrix, with orthonormal U and V and
/// nonincreasing values.
#[test]
fn decomposes_by_svd() {
    for (seed, &(rows, cols)) in (1..).zip(&SHAPES) {
        check_svd(&matrix(rows, cols, seed));
    }
}

/// Rank-deficient matrices, with repeated and zero singular values, as a
/// split meets them all the time. Some libraries' SVDs return a wrong
/// singular value on such matrices (nalgebra 0.35's does), which a split
/// would then truncate wrongly. The zero matrix gives one value, exactly 0:
/// the trim would keep none of its rows.
#[test]
fn decomposes_rank_deficient_matrices_by_svd() {
    let algebra = &CpuLinearAlgebra;
    for (rows, cols) in [(4, 3), (3, 4)] {
        let zero = Matrix::from_fn(rows, cols, |_, _| Complex64::new(0.0, 0.0));
        assert_eq!(check_svd(&zero), [0.0], "{rows} × {cols}");
    }
    for (seed, case) in (0_u64..60).zip(0_usize..) {
        let n = 2 + case % 15;
        let rank = 1 + (case / 15) % n;
        let u = algebra.qr(matrix(n, n, 2 * seed + 1).view()).expect("QR").q;
        let v = algebra.qr(matrix(n, n, 2 * seed + 2).view()).expect("QR").q;
        let diagonal = Matrix::from_fn(n, n, |i, j| {
            Complex64::new(
                if i == j && i < rank {
                    0.5_f64.sqrt()
                } else {
                    0.0
                },
                0.0,
            )
        });
        let degenerate = naive_product(naive_product(u.view(), diagonal.view()).view(), v.view());
        check_svd(&degenerate);
        let (rows, cols) = (n + case % 3, n + (case / 3) % 3);
        let product = naive_product(
            matrix(rows, rank, 3 * seed + 1).view(),
            matrix(rank, cols, 3 * seed + 2).view(),
        );
        check_svd(&product);
    }
}

/// `m` times `2^exponent`, in two steps so that each factor is finite;
/// rounded where the product is subnormal, exact otherwise.
fn times_power_of_two(m: &Matrix, exponent: i32) -> Matrix {
    let factors = [
        2_f64.powi(exponent / 2),
        2_f64.powi(exponent - exponent / 2),
    ];
    let elements = m.view().elements().iter();
    let scaled = elements.map(|x| x * factors[0] * factors[1]).collect();
    Matrix::new(m.rows(), m.cols(), scaled)
}
