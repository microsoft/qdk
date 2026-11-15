// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the matrices (`matrix`): column-major, holding exactly
//! `rows·cols` elements, with their product and adjoint.

use super::{SHAPES, c, distance, matrix, naive_product};
use crate::{Matrix, MatrixRef};

/// A matrix built element by element is stored column by column.
#[test]
fn from_fn_fills_column_by_column() {
    let index = [0.0, 1.0, 2.0];
    let m = Matrix::from_fn(2, 3, |i, j| c(index[i], index[j]));
    let expected =
        [(0, 0), (1, 0), (0, 1), (1, 1), (0, 2), (1, 2)].map(|(i, j)| c(index[i], index[j]));
    assert_eq!(m.elements(), expected);
}

/// Five elements are not a 2 × 3 matrix.
#[test]
#[should_panic(expected = "a 2 × 3 matrix")]
fn a_matrix_holds_rows_times_cols_elements() {
    let _ = Matrix::new(2, 3, vec![c(0.0, 0.0); 5]);
}

/// Five borrowed elements are not a 2 × 3 matrix either.
#[test]
#[should_panic(expected = "a 2 × 3 matrix")]
fn a_borrowed_matrix_holds_rows_times_cols_elements() {
    let elements = [c(0.0, 0.0); 5];
    let _ = MatrixRef::new(2, 3, &elements);
}

/// The product of two matrices is the naive product, to rounding, on tall,
/// wide, square and single-row shapes.
#[test]
fn multiplies() {
    for (seed, &(rows, cols)) in (1..).zip(&SHAPES) {
        let a = matrix(rows, cols, seed);
        let b = matrix(cols, 3, seed + 10);
        let product = Matrix::product(a.view(), b.view());
        assert!(distance(&product, &naive_product(a.view(), b.view())) < 1e-14);
    }
}

/// A 2 × 3 matrix cannot multiply a 2 × 3 matrix: the product panics
/// rather than read past the first one's columns.
#[test]
#[should_panic(expected = "the product of a 2 × 3 and a 2 × 3 matrix")]
fn multiplies_only_matching_shapes() {
    let a = matrix(2, 3, 1);
    let _ = Matrix::product(a.view(), a.view());
}

/// The adjoint of a 2 × 3 matrix is its conjugate transpose, 3 × 2.
#[test]
fn the_adjoint_is_the_conjugate_transpose() {
    let a = matrix(2, 3, 1);
    let adjoint = a.view().adjoint();
    assert_eq!((adjoint.rows(), adjoint.cols()), (3, 2));
    for i in 0..3 {
        for j in 0..2 {
            assert_eq!(
                adjoint.elements()[i + 3 * j],
                a.elements()[j + 2 * i].conj()
            );
        }
    }
}
