// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the contract of [`Kernels`] that the SVD relies on beyond
//! the public product and QR: the column-pivoted QR, `A·P = Q·R` with its
//! permutation `forward` and its pivot rule, and the product of `Q`'s
//! chosen columns, run on the implementation the crate ships. The parts of
//! that implementation, each called directly, have their tests beside
//! them: the column norms that pick the pivots (`ColumnNorms`).

use crate::test_utils::{SHAPES, adjoint, c, distance, identity, matrix, naive_product};
use crate::{
    Matrix, Qr,
    kernels::{DefaultKernels, Kernels, PivotedQr},
};
use num_complex::Complex64;

/// `Q`, thin, of `qr`: its first `min(rows, cols)` columns times the
/// identity.
fn thin_q(qr: &PivotedQr<<DefaultKernels as Kernels>::Q>, rows: usize, k: usize) -> Matrix {
    let kept: Vec<usize> = (0..k).collect();
    let q = DefaultKernels::q_kept_times(qr, &kept, identity(k).elements(), k);
    Matrix::new(rows, k, q)
}

/// Matrices for the pivoted QR, named: random ones, tall, wide, square and
/// a single row; and those whose pivoting an MPS simulation tests, two
/// equal columns, a zero column, ranks 1 and 5 with the columns graded
/// over nine orders of magnitude, singular values graded over three, and a
/// subnormal entry beside 1.
fn pivoted_qr_cases() -> Vec<(String, Matrix)> {
    let mut cases: Vec<(String, Matrix)> = (1..)
        .zip(SHAPES.iter().chain(&[(5, 9)]))
        .map(|(seed, &(rows, cols))| (format!("random {rows} × {cols}"), matrix(rows, cols, seed)))
        .collect();
    cases.push(("two equal columns".into(), with_two_equal_columns()));
    cases.push(("a zero column".into(), with_a_zero_column()));
    for (rank, rows, cols, seed) in [(1, 17, 33, 13), (5, 33, 17, 14)] {
        cases.push((
            format!("rank {rank}, graded columns, {rows} × {cols}"),
            of_rank_with_graded_columns(rank, rows, cols, seed),
        ));
    }
    cases.push((
        "singular values graded".into(),
        with_graded_singular_values(),
    ));
    cases.push((
        "a subnormal entry beside 1".into(),
        with_a_subnormal_entry_beside_one(),
    ));
    cases
}

/// The 2 × 2 matrix of columns `[t + it, 1]ᵀ` and zero, `t` the smallest
/// subnormal `f64`, as a gate may leave on a product state. A QR that took
/// the phase of `t + it` to be `1 + i`, of modulus `√2`, gave an `R` that
/// does not give the matrix back.
fn with_a_subnormal_entry_beside_one() -> Matrix {
    let t = f64::from_bits(1);
    Matrix::new(2, 2, vec![c(t, t), c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0)])
}

/// A 5 × 4 matrix whose last column repeats its first.
fn with_two_equal_columns() -> Matrix {
    let three = matrix(5, 3, 9);
    Matrix::from_fn(5, 4, |i, j| three.elements()[i + 5 * (j % 3)])
}

/// A random 6 × 4 matrix with its column 2 zero.
fn with_a_zero_column() -> Matrix {
    let a = matrix(6, 4, 10);
    Matrix::from_fn(6, 4, |i, j| {
        if j == 2 {
            c(0.0, 0.0)
        } else {
            a.elements()[i + 6 * j]
        }
    })
}

/// A random `rows × cols` matrix of rank `rank`, its column `j` scaled by
/// `10^-(j mod 9)`.
fn of_rank_with_graded_columns(rank: usize, rows: usize, cols: usize, seed: u64) -> Matrix {
    let low = naive_product(
        matrix(rows, rank, seed).view(),
        matrix(rank, cols, seed + 100).view(),
    );
    Matrix::from_fn(rows, cols, |i, j| {
        let exponent = i32::try_from(j % 9).expect("below 9");
        low.elements()[i + rows * j] * 10_f64.powi(-exponent)
    })
}

/// A 20 × 16 matrix `U·diag(s)·V†` of rank 12, with `s_j = 10^(-j/4)`.
fn with_graded_singular_values() -> Matrix {
    let (rows, cols, n) = (20, 16, 12);
    let u = Qr::of(matrix(rows, n, 15).view()).q;
    let v = Qr::of(matrix(cols, n, 16).view()).q;
    let u_s = Matrix::from_fn(rows, n, |i, j| {
        #[expect(clippy::cast_precision_loss, reason = "a column index")]
        let value = 10_f64.powf(-(j as f64) / 4.0);
        u.elements()[i + rows * j] * value
    });
    naive_product(u_s.view(), adjoint(&v).view())
}

/// The column-pivoted QR of each of [`pivoted_qr_cases`], and of the
/// scales the SVD's scaling leaves (a largest entry of `2⁻¹⁰⁰` or `2¹⁰⁰`,
/// and a column `1e-125` times smaller, whose squared norm is subnormal,
/// or `1e-170` times, whose squared norm underflows):
///
/// - `R` is upper trapezoidal, `min(rows, cols) × cols`;
/// - `Q` has orthonormal columns;
/// - `forward` is a permutation;
/// - up to rounding of `A`'s largest entry, column `j` of `Q·R` is column
///   `forward[j]` of `A`.
#[test]
fn a_pivoted_qr_gives_back_the_permuted_matrix_with_orthonormal_q() {
    let cases = pivoted_qr_cases().into_iter().chain(at_extreme_scales());
    for (name, a) in cases {
        let (rows, cols) = (a.rows(), a.cols());
        let k = rows.min(cols);
        let largest = a.elements().iter().map(|x| x.norm()).fold(0.0, f64::max);
        let qr = DefaultKernels::pivoted_qr(a.view());
        assert_eq!((qr.r.rows(), qr.r.cols()), (k, cols), "{name}");
        assert!(
            is_upper_trapezoidal(&qr.r),
            "{name}: R is upper trapezoidal"
        );
        assert!(is_permutation_of(&qr.forward, cols), "{name}");
        let q = thin_q(&qr, rows, k);
        let gram = naive_product(adjoint(&q).view(), q.view());
        assert!(distance(&gram, &identity(k)) < 1e-13, "{name}");
        let a_p = Matrix::from_fn(rows, cols, |i, j| a.elements()[i + rows * qr.forward[j]]);
        let q_r = naive_product(q.view(), qr.r.view());
        assert!(distance(&q_r, &a_p) < 1e-13 * largest, "{name}");
    }
}

/// A random 6 × 4 matrix at the scales the SVD's scaling leaves: a
/// largest entry of `2⁻¹⁰⁰` or `2¹⁰⁰`, and, at `2⁻¹⁰⁰`, a column `1e-125`
/// or `1e-170` times smaller.
fn at_extreme_scales() -> Vec<(String, Matrix)> {
    let a = matrix(6, 4, 17);
    let mut cases = Vec::new();
    for exponent in [-100, 100] {
        let scale = 2_f64.powi(exponent);
        cases.push((
            format!("6 × 4 at 2^{exponent}"),
            Matrix::from_fn(6, 4, |i, j| a.elements()[i + 6 * j] * scale),
        ));
    }
    let tiny = 2_f64.powi(-100);
    for factor in [1e-125, 1e-170] {
        cases.push((
            format!("a column {factor:e} times the largest entry 2^-100"),
            Matrix::from_fn(6, 4, |i, j| {
                a.elements()[i + 6 * j] * if j == 1 { tiny * factor } else { tiny }
            }),
        ));
    }
    cases
}

/// Whether every entry of `r` below its diagonal is zero.
fn is_upper_trapezoidal(r: &Matrix) -> bool {
    let (rows, cols, elements) = (r.rows(), r.cols(), r.elements());
    (0..cols).all(|j| (j + 1..rows).all(|i| elements[i + rows * j] == c(0.0, 0.0)))
}

/// Whether `forward` holds each of `0..cols` once.
fn is_permutation_of(forward: &[usize], cols: usize) -> bool {
    let mut sorted = forward.to_vec();
    sorted.sort_unstable();
    sorted == (0..cols).collect::<Vec<_>>()
}

/// The pivot of each step is the remaining column of largest norm. So, to
/// within about `√ε/2` of the pivot, `|R_kk|` is at least the norm of
/// every later column of `R` from row `k` on. The SVD's trim reads the
/// rank from `R`. The cases are [`pivoted_qr_cases`]. How the remaining
/// norms are kept, and when they are measured again, is tested with
/// `ColumnNorms`.
#[test]
fn pivots_on_the_remaining_column_of_largest_norm() {
    for (name, a) in pivoted_qr_cases() {
        let (rows, cols) = (a.rows(), a.cols());
        let qr = DefaultKernels::pivoted_qr(a.view());
        #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
        let rounding = rows.max(cols) as f64 * f64::EPSILON * frobenius_norm(&a);
        for k in 0..rows.min(cols) {
            let pivot = qr.r.elements()[k + qr.r.rows() * k].norm();
            for j in k + 1..cols {
                let rest = norm_from_row(&qr.r, k, j);
                // A downdated squared norm errs by rounding on the one last
                // computed, and the recompute rule keeps it above `√ε` times
                // that: its relative error stays near `√ε` (1.5e-8), about
                // 7e-9 on the norm, by which a pivot may lose to a column.
                assert!(
                    rest <= pivot * (1.0 + 1e-8) + rounding,
                    "{name}: column {j} of R from row {k} has norm {rest}, above the pivot {pivot}"
                );
            }
        }
    }
}

/// The Frobenius norm of `a`.
fn frobenius_norm(a: &Matrix) -> f64 {
    a.elements()
        .iter()
        .map(Complex64::norm_sqr)
        .sum::<f64>()
        .sqrt()
}

/// The norm of column `j` of `r` from row `k` down.
fn norm_from_row(r: &Matrix, k: usize, j: usize) -> f64 {
    let rows = r.rows();
    (k..rows)
        .map(|i| r.elements()[i + rows * j].norm_sqr())
        .sum::<f64>()
        .sqrt()
}

/// `Q_kept·x` is the product of `Q`'s columns `kept` with `x`, whichever
/// columns are kept: not contiguous and short of the last (0, 2 and 3 of
/// 5), the last alone, all of them, and two of a wide matrix's `Q`.
#[test]
fn multiplies_the_kept_columns_of_q() {
    let cases: [(usize, usize, &[usize]); 4] = [
        (7, 5, &[0, 2, 3]),
        (7, 5, &[4]),
        (7, 5, &[0, 1, 2, 3, 4]),
        (5, 9, &[1, 4]),
    ];
    for (seed, (rows, cols, kept)) in (11..).zip(cases) {
        let a = matrix(rows, cols, seed);
        let qr = DefaultKernels::pivoted_qr(a.view());
        let q = thin_q(&qr, rows, rows.min(cols));
        let x = matrix(kept.len(), 2, seed + 100);
        let product = DefaultKernels::q_kept_times(&qr, kept, x.elements(), 2);
        let q_kept = Matrix::from_fn(rows, kept.len(), |i, j| q.elements()[i + rows * kept[j]]);
        let expected = naive_product(q_kept.view(), x.view());
        assert!(
            distance(&Matrix::new(rows, 2, product), &expected) < 1e-14,
            "{rows} × {cols}, kept {kept:?}"
        );
    }
}
