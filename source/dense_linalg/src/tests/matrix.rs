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

/// `2⁶³ × 2` on a 64-bit target (`2³¹ × 2` on 32 bits) has more elements
/// than `usize` counts: unchecked, the count would wrap to 0, which an
/// empty vector matches.
#[test]
#[should_panic(expected = "× 2 matrix has more elements than `usize` counts")]
fn a_matrix_whose_element_count_overflows_is_refused() {
    let _ = Matrix::new(1 << (usize::BITS - 1), 2, Vec::new());
}

/// A borrowed matrix too: an empty slice is not a `2⁶³ × 2` matrix.
#[test]
#[should_panic(expected = "× 2 matrix has more elements than `usize` counts")]
fn a_borrowed_matrix_whose_element_count_overflows_is_refused() {
    let _ = MatrixRef::new(1 << (usize::BITS - 1), 2, &[]);
}

/// A matrix built element by element is refused before its first element
/// is computed.
#[test]
#[should_panic(expected = "× 2 matrix has more elements than `usize` counts")]
fn from_fn_refuses_an_overflowing_element_count_before_any_element() {
    let _ = Matrix::from_fn(1 << (usize::BITS - 1), 2, |_, _| {
        panic!("no element is computed")
    });
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

/// Two empty matrices, `2⁶³ × 0` and `0 × 2` on a 64-bit target, have a
/// product of more elements than `usize` counts: the product panics as
/// `Matrix::new` does for that shape.
#[test]
#[should_panic(expected = "× 2 matrix has more elements than `usize` counts")]
fn a_product_whose_element_count_overflows_is_refused() {
    let huge = 1 << (usize::BITS - 1);
    let _ = Matrix::product(MatrixRef::new(huge, 0, &[]), MatrixRef::new(0, 2, &[]));
}

/// The product of matrices without elements is empty, whatever their
/// dimensions: `0 × 2⁶³` times `2⁶³ × 0` is `0 × 0`, and `2⁶³ × 0` times
/// `0 × 0` is `2⁶³ × 0`.
#[test]
fn an_empty_product_of_any_size_is_empty() {
    let huge = 1 << (usize::BITS - 1);
    let empty = |rows, cols| MatrixRef::new(rows, cols, &[]);
    let product = Matrix::product(empty(0, huge), empty(huge, 0));
    assert_eq!((product.rows(), product.cols()), (0, 0));
    let product = Matrix::product(empty(huge, 0), empty(0, 0));
    assert_eq!((product.rows(), product.cols()), (huge, 0));
}

/// A matrix with no rows has nothing in its columns: built element by
/// element, `0 × 2⁶³` returns at once, without visiting them.
#[test]
fn from_fn_visits_no_column_of_a_matrix_without_rows() {
    let huge = 1 << (usize::BITS - 1);
    let empty = Matrix::from_fn(0, huge, |_, _| unreachable!("no element"));
    assert_eq!((empty.rows(), empty.cols()), (0, huge));
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

/// The adjoint of a matrix without elements is empty, and returns at once
/// even for `2⁶³ × 0`, whose adjoint has `2⁶³` empty columns.
#[test]
fn the_adjoint_of_an_empty_matrix_of_any_size_is_empty() {
    let huge = 1 << (usize::BITS - 1);
    let adjoint = MatrixRef::new(huge, 0, &[]).adjoint();
    assert_eq!((adjoint.rows(), adjoint.cols()), (0, huge));
}
