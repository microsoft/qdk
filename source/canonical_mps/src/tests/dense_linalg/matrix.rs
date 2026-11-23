// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the matrices (`matrix`): column-major, holding exactly
//! `rows·cols` elements, with their product and adjoint.

use super::{SHAPES, c, distance, matrix, naive_product, panic_message};
use crate::dense_linalg::{Matrix, MatrixRef};

/// A matrix built element by element is stored column by column.
#[test]
fn from_fn_lays_entries_out_column_major() {
    let index = [0.0, 1.0, 2.0];
    let m = Matrix::from_fn(2, 3, |i, j| c(index[i], index[j]));
    let expected =
        [(0, 0), (1, 0), (0, 1), (1, 1), (0, 2), (1, 2)].map(|(i, j)| c(index[i], index[j]));
    assert_eq!(m.elements(), expected);
}

/// Five elements, owned or borrowed, are not a 2 × 3 matrix.
#[test]
fn a_matrix_holds_rows_times_cols_elements() {
    let rows: [(&str, fn()); 2] = [
        ("owned", || {
            let _ = Matrix::new(2, 3, vec![c(0.0, 0.0); 5]);
        }),
        ("borrowed", || {
            let elements = [c(0.0, 0.0); 5];
            let _ = MatrixRef::new(2, 3, &elements);
        }),
    ];
    for (name, build) in rows {
        let message = panic_message(build);
        assert!(message.contains("a 2 × 3 matrix"), "{name}: {message}");
    }
}

/// `2⁶³ × 2` on a 64-bit target (`2³¹ × 2` on 32 bits) has more elements
/// than `usize` counts: unchecked, the count would wrap to 0, which an
/// empty vector matches. Every way to make such a matrix is refused: owned
/// or borrowed, built element by element (before its first element is
/// computed), or as the product of two empty matrices, `2⁶³ × 0` and
/// `0 × 2`.
#[test]
fn an_overflowing_element_count_is_refused() {
    let rows: [(&str, fn()); 4] = [
        ("owned", || {
            let _ = Matrix::new(1 << (usize::BITS - 1), 2, Vec::new());
        }),
        ("borrowed", || {
            let _ = MatrixRef::new(1 << (usize::BITS - 1), 2, &[]);
        }),
        ("from_fn", || {
            let _ = Matrix::from_fn(1 << (usize::BITS - 1), 2, |_, _| {
                panic!("no element is computed")
            });
        }),
        ("product", || {
            let huge = 1 << (usize::BITS - 1);
            let _ = Matrix::product(MatrixRef::new(huge, 0, &[]), MatrixRef::new(0, 2, &[]));
        }),
    ];
    let shape = format!("{} × 2", 1_usize << (usize::BITS - 1));
    for (name, build) in rows {
        let message = panic_message(build);
        assert!(
            message.contains(&shape),
            "{name}: the shape is named: {message}"
        );
        assert!(
            message.contains("usize"),
            "{name}: the fault is named: {message}"
        );
    }
}

/// The product of two matrices sums each row of the first times each column
/// of the second, as the naive product does, to rounding, on tall, wide,
/// square and single-row shapes.
#[test]
fn a_product_sums_rows_times_columns() {
    for (seed, &(rows, cols)) in (1..).zip(&SHAPES) {
        let a = matrix(rows, cols, seed);
        let b = matrix(cols, 3, seed + 10);
        let product = Matrix::product(a.view(), b.view());
        assert!(distance(&product, &naive_product(a.view(), b.view())) < 1e-14);
    }
}

/// A 2 × 3 matrix cannot multiply a 2 × 3 matrix: the product panics
/// rather than read past the first one's columns, naming both shapes.
#[test]
fn a_product_of_mismatched_shapes_panics() {
    let message = panic_message(|| {
        let a = matrix(2, 3, 1);
        let _ = Matrix::product(a.view(), a.view());
    });
    assert_eq!(
        message.matches("2 × 3").count(),
        2,
        "both shapes are named: {message}"
    );
    assert!(message.contains("product"), "the fault is named: {message}");
}

/// A matrix without elements stays empty, whatever its dimensions, and
/// returns at once: each row's result and its shape. The product of
/// `0 × 2⁶³` and `2⁶³ × 0` is `0 × 0`, and of `2⁶³ × 0` and `0 × 0` is
/// `2⁶³ × 0`; built element by element, `0 × 2⁶³` visits none of its
/// columns; the adjoint of `2⁶³ × 0` has `2⁶³` empty columns.
#[test]
fn an_empty_matrix_of_any_size_stays_empty() {
    let huge = 1 << (usize::BITS - 1);
    let empty = |rows, cols| MatrixRef::new(rows, cols, &[]);
    let rows = [
        (
            "a product to 0 × 0",
            Matrix::product(empty(0, huge), empty(huge, 0)),
            (0, 0),
        ),
        (
            "a product to 2⁶³ × 0",
            Matrix::product(empty(huge, 0), empty(0, 0)),
            (huge, 0),
        ),
        (
            "from_fn",
            Matrix::from_fn(0, huge, |_, _| unreachable!("no element")),
            (0, huge),
        ),
        ("adjoint", empty(huge, 0).adjoint(), (0, huge)),
    ];
    for (name, matrix, shape) in rows {
        assert_eq!((matrix.rows(), matrix.cols()), shape, "{name}");
        assert!(matrix.elements().is_empty(), "{name}");
    }
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
