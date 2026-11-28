// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the thin QR (`qr`): it gives back the matrix, with
//! orthonormal columns in `Q` and `R` upper trapezoidal, at any scale and
//! for an empty matrix of any size. On an entry that is not finite, it
//! still answers factors of the thin shapes rather than panic.

use crate::test_utils::{
    adjoint, c, distance, identity, matrix, naive_product, scenes_at_each_scale,
};
use crate::{Matrix, Qr};

/// Checks that `Qr::of(a)` gives back `a` to rounding of its largest entry,
/// with orthonormal columns in `Q` and `R` upper trapezoidal.
fn check_qr(a: &Matrix, scene: &str) {
    let (rows, cols) = (a.rows(), a.cols());
    let Qr { q, r } = Qr::of(a.view());
    let k = rows.min(cols);
    let shape = (q.rows(), q.cols(), r.rows(), r.cols());
    assert_eq!(shape, (rows, k, k, cols), "{scene}");
    let largest = a.elements().iter().map(|x| x.norm()).fold(0.0, f64::max);
    let error = distance(&naive_product(q.view(), r.view()), a);
    assert!(
        error <= 1e-13 * largest,
        "{scene}, {rows} × {cols}: {error:e} of {largest:e}"
    );
    let gram = naive_product(adjoint(&q).view(), q.view());
    assert!(
        distance(&gram, &identity(k)) < 1e-13,
        "{scene}, {rows} × {cols}: Q's columns"
    );
    for j in 0..cols {
        for i in j + 1..k {
            assert_eq!(
                r.elements()[i + k * j],
                c(0.0, 0.0),
                "{scene}, {rows} × {cols}: R[{i}, {j}]"
            );
        }
    }
}

/// A thin QR gives back its matrix, with orthonormal columns in `Q` and
/// `R` upper trapezoidal, in each of the [`scenes_at_each_scale`]: on
/// tall, wide, square and single-row shapes, at every scale of the `f64`
/// range, as it scales by the largest entry.
#[test]
fn a_thin_qr_gives_back_the_matrix_with_orthonormal_q_at_any_scale() {
    for (scene, matrices) in &scenes_at_each_scale() {
        for a in matrices {
            check_qr(a, scene);
        }
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
