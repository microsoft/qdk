// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the contract of
//! [`Kernels`](crate::dense_linalg::kernels::Kernels) that the SVD relies on
//! beyond the public product and QR: the column-pivoted QR, `A·P = Q·R` with
//! its permutation `forward` and its pivot rule, and the product of `Q`'s
//! chosen columns, run on the implementation the module ships.

use super::{SHAPES, adjoint, c, distance, identity, matrix, naive_product};
use crate::dense_linalg::{
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
/// over nine orders of magnitude, and singular values graded over three.
fn pivoted_qr_cases() -> Vec<(String, Matrix)> {
    let mut cases: Vec<(String, Matrix)> = (1..)
        .zip(SHAPES.iter().chain(&[(5, 9)]))
        .map(|(seed, &(rows, cols))| (format!("random {rows} × {cols}"), matrix(rows, cols, seed)))
        .collect();
    let three = matrix(5, 3, 9);
    let repeated = Matrix::from_fn(5, 4, |i, j| three.elements()[i + 5 * (j % 3)]);
    cases.push(("two equal columns".into(), repeated));
    let a = matrix(6, 4, 10);
    let zero_column = Matrix::from_fn(6, 4, |i, j| {
        if j == 2 {
            c(0.0, 0.0)
        } else {
            a.elements()[i + 6 * j]
        }
    });
    cases.push(("a zero column".into(), zero_column));
    for (rank, rows, cols, seed) in [(1, 17, 33, 13), (5, 33, 17, 14)] {
        let low = naive_product(
            matrix(rows, rank, seed).view(),
            matrix(rank, cols, seed + 100).view(),
        );
        let graded = Matrix::from_fn(rows, cols, |i, j| {
            let exponent = i32::try_from(j % 9).expect("below 9");
            low.elements()[i + rows * j] * 10_f64.powi(-exponent)
        });
        cases.push((
            format!("rank {rank}, graded columns, {rows} × {cols}"),
            graded,
        ));
    }
    let (rows, cols, n) = (20, 16, 12);
    let u = Qr::of(matrix(rows, n, 15).view()).q;
    let v = Qr::of(matrix(cols, n, 16).view()).q;
    let u_s = Matrix::from_fn(rows, n, |i, j| {
        #[expect(clippy::cast_precision_loss, reason = "a column index")]
        let value = 10_f64.powf(-(j as f64) / 4.0);
        u.elements()[i + rows * j] * value
    });
    let graded = naive_product(u_s.view(), adjoint(&v).view());
    cases.push(("singular values graded".into(), graded));
    cases
}

/// The column-pivoted QR of each of [`pivoted_qr_cases`], and of the
/// scales the SVD's scaling leaves (a largest entry of `2⁻¹⁰⁰` or `2¹⁰⁰`,
/// and a column `1e-125` times smaller, whose squared norm is subnormal,
/// or `1e-170` times, whose squared norm underflows):
/// `R` is upper trapezoidal, `min(rows, cols) × cols`, `Q` has orthonormal
/// columns, `forward` is a permutation, and column `j` of `Q·R` is column
/// `forward[j]` of `A`, to rounding of `A`'s largest entry.
#[test]
fn decomposes_by_column_pivoted_qr() {
    let mut cases = pivoted_qr_cases();
    let a = matrix(6, 4, 17);
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
    for (name, a) in cases {
        let (rows, cols) = (a.rows(), a.cols());
        let k = rows.min(cols);
        let largest = a.elements().iter().map(|x| x.norm()).fold(0.0, f64::max);
        let qr = DefaultKernels::pivoted_qr(a.view());
        assert_eq!((qr.r.rows(), qr.r.cols()), (k, cols), "{name}");
        let r = qr.r.elements();
        assert!(
            (0..cols).all(|j| (j + 1..k).all(|i| r[i + k * j] == c(0.0, 0.0))),
            "{name}: R is upper trapezoidal"
        );
        let mut sorted = qr.forward.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..cols).collect::<Vec<_>>(), "{name}");
        let q = thin_q(&qr, rows, k);
        let gram = naive_product(adjoint(&q).view(), q.view());
        assert!(distance(&gram, &identity(k)) < 1e-13, "{name}");
        let a_p = Matrix::from_fn(rows, cols, |i, j| a.elements()[i + rows * qr.forward[j]]);
        let q_r = naive_product(q.view(), qr.r.view());
        assert!(distance(&q_r, &a_p) < 1e-13 * largest, "{name}");
    }
}

/// The pivot of each step is the remaining column of largest norm, so
/// that `|R_kk|` is at least the norm of every later column of `R` from
/// row `k` on, to rounding: the SVD's trim reads the rank from `R`. On
/// [`pivoted_qr_cases`], and on matrices where a QR that kept the
/// columns' first norms, or downdated them without recomputing them
/// against their own last computed norm, would pivot on the wrong column:
/// one mostly along the first pivot, whose norm stays large while what
/// remains of it is 0.1, beside one of 0.5; a remaining norm of 1e-10
/// that downdating rounds to 0, beside one of 1e-11; and a column swapped
/// into the place of one whose last computed norm was smaller, whose
/// downdate then runs past the point where it must be recomputed.
#[test]
fn pivots_on_the_remaining_column_of_largest_norm() {
    fn real<const N: usize>(columns: [[f64; N]; N]) -> Matrix {
        Matrix::from_fn(N, N, |i, j| c(columns[j][i], 0.0))
    }
    let mut cases = pivoted_qr_cases();
    cases.push((
        "a column mostly along the first pivot".into(),
        real([[2.0, 0.0, 0.0], [1.5, 0.1, 0.0], [0.0, 0.0, 0.5]]),
    ));
    cases.push((
        "a remaining norm that downdating rounds to 0".into(),
        real([[1.0, 0.0, 0.0], [1.0, 1e-10, 0.0], [0.0, 0.0, 1e-11]]),
    ));
    cases.push((
        "a column swapped past one of smaller last computed norm".into(),
        real([
            [2.0, 0.0, 0.0, 0.0],
            [
                1.0,
                0.001_382_413_486_245_529,
                7.843_482_461_178_048e-7,
                0.0,
            ],
            [0.0, 0.002_122_620_547_090_723_4, 0.0, 0.0],
            [0.0, 0.0, 0.0, 7.844e-7],
        ]),
    ));
    for (name, a) in cases {
        let (rows, cols) = (a.rows(), a.cols());
        let k_max = rows.min(cols);
        let qr = DefaultKernels::pivoted_qr(a.view());
        let r = qr.r.elements();
        let norm = a
            .elements()
            .iter()
            .map(Complex64::norm_sqr)
            .sum::<f64>()
            .sqrt();
        #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
        let rounding = rows.max(cols) as f64 * f64::EPSILON * norm;
        for k in 0..k_max {
            let pivot = r[k + k_max * k].norm();
            for j in k + 1..cols {
                let rest = (k..k_max)
                    .map(|i| r[i + k_max * j].norm_sqr())
                    .sum::<f64>()
                    .sqrt();
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
