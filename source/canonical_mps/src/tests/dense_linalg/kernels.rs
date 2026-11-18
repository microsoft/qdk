// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the contract of
//! [`Kernels`](crate::dense_linalg::kernels::Kernels) that the SVD relies on
//! beyond the public product and QR: the column-pivoted QR, `A·P = Q·R` with
//! its permutation `forward`, the product of `Q`'s chosen columns, and the fast
//! route's SVD, run on the implementation the module ships.

use super::{SHAPES, adjoint, check_svd_of, distance, identity, matrix, naive_product};
use crate::dense_linalg::{
    Matrix, Svd,
    kernels::{Kernels, PivotedQr, Shipped},
};
use num_complex::Complex64;

/// `Q`, thin, of `qr`: its first `min(rows, cols)` columns times the
/// identity.
fn thin_q(qr: &PivotedQr<<Shipped as Kernels>::Q>, rows: usize, k: usize) -> Matrix {
    let kept: Vec<usize> = (0..k).collect();
    let q = Shipped::q_kept_times(qr, &kept, identity(k).elements(), k);
    Matrix::new(rows, k, q)
}

/// The column-pivoted QR of tall, wide, square and single-row matrices,
/// and of one with two equal columns: `R` is upper trapezoidal,
/// `min(rows, cols) × cols`, `Q` has orthonormal columns, `forward` is a
/// permutation, and column `j` of `Q·R` is column `forward[j]` of `A`.
#[test]
fn decomposes_by_column_pivoted_qr() {
    let three = matrix(5, 3, 9);
    let repeated = Matrix::from_fn(5, 4, |i, j| three.elements()[i + 5 * (j % 3)]);
    let mut cases: Vec<Matrix> = (1..)
        .zip(&SHAPES)
        .map(|(seed, &(rows, cols))| matrix(rows, cols, seed))
        .collect();
    cases.push(repeated);
    for a in cases {
        let (rows, cols) = (a.rows(), a.cols());
        let k = rows.min(cols);
        let qr = Shipped::pivoted_qr(a.view());
        assert_eq!((qr.r.rows(), qr.r.cols()), (k, cols));
        let r = qr.r.elements();
        assert!(
            (0..cols).all(|j| (j + 1..k).all(|i| r[i + k * j] == Complex64::new(0.0, 0.0))),
            "{rows} × {cols}: R is upper trapezoidal"
        );
        let mut sorted = qr.forward.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..cols).collect::<Vec<_>>(), "{rows} × {cols}");
        let q = thin_q(&qr, rows, k);
        let gram = naive_product(adjoint(&q).view(), q.view());
        assert!(distance(&gram, &identity(k)) < 1e-13, "{rows} × {cols}");
        let a_p = Matrix::from_fn(rows, cols, |i, j| a.elements()[i + rows * qr.forward[j]]);
        let q_r = naive_product(q.view(), qr.r.view());
        assert!(distance(&q_r, &a_p) < 1e-13, "{rows} × {cols}");
    }
}

/// The product of `Q`'s columns 0, 2 and 3 of a 7 × 5 matrix's pivoted QR,
/// not contiguous and short of the last, with a 3 × 2 matrix: it is
/// `Q_kept·x`, with `Q_kept` those columns of the thin `Q`.
#[test]
fn multiplies_the_kept_columns_of_q() {
    let a = matrix(7, 5, 11);
    let qr = Shipped::pivoted_qr(a.view());
    let q = thin_q(&qr, 7, 5);
    let kept = [0, 2, 3];
    let x = matrix(3, 2, 12);
    let product = Shipped::q_kept_times(&qr, &kept, x.elements(), 2);
    let q_kept = Matrix::from_fn(7, 3, |i, j| q.elements()[i + 7 * kept[j]]);
    let expected = naive_product(q_kept.view(), x.view());
    assert!(distance(&Matrix::new(7, 2, product), &expected) < 1e-14);
}

/// The fast route on the full-rank matrices the route sends it, tall and
/// square, complex and real, with columns graded over five orders of
/// magnitude, and on the smallest shapes its contract allows: it answers,
/// and its answer is an SVD with one value per column, so that the
/// rotations are its fallback, not its replacement.
#[test]
fn the_fast_route_decomposes_tall_matrices() {
    let complex = [(128, 128), (96, 48), (40, 32), (5, 1), (2, 2), (1, 1)];
    let mut cases: Vec<Matrix> = (1..)
        .zip(complex)
        .map(|(seed, (rows, cols))| matrix(rows, cols, seed))
        .collect();
    let a = matrix(64, 32, 20);
    cases.push(Matrix::from_fn(64, 32, |i, j| {
        Complex64::new(a.elements()[i + 64 * j].re, 0.0)
    }));
    cases.push(Matrix::from_fn(64, 32, |i, j| {
        #[expect(clippy::cast_precision_loss, reason = "a column index")]
        let grade = 10_f64.powf(-5.0 * j as f64 / 31.0);
        a.elements()[i + 64 * j] * grade
    }));
    for w in cases {
        let (rows, cols) = (w.rows(), w.cols());
        let (u, singular_values, v) =
            Shipped::tall_svd(rows, cols, w.elements()).expect("the fast route answers");
        let svd = Svd {
            u: Matrix::new(rows, cols, u),
            singular_values,
            v_adjoint: adjoint(&Matrix::new(cols, cols, v)),
        };
        let values = check_svd_of(&w, svd);
        assert_eq!(values.len(), cols, "{rows} × {cols}");
    }
}
