// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The shared layout of an [`OperatorMatrix`]: where its entries are, which
//! matrices are diagonal.

use crate::qir::{OperatorMatrix, QirGate};
use num_complex::Complex64;

pub(super) const O: Complex64 = Complex64::new(0.0, 0.0);
pub(super) const L: Complex64 = Complex64::new(1.0, 0.0);
pub(super) const I: Complex64 = Complex64::new(0.0, 1.0);

/// The matrix as rows `M[out][in]`, read through [`OperatorMatrix::entry`].
pub(super) fn rows(matrix: &OperatorMatrix) -> Vec<Vec<Complex64>> {
    let dimension = matrix.dimension();
    (0..dimension)
        .map(|output| {
            (0..dimension)
                .map(|input| matrix.entry(output, input))
                .collect()
        })
        .collect()
}

/// `matrix` applied to the column vector `state`.
pub(super) fn apply(matrix: &OperatorMatrix, state: &[Complex64]) -> Vec<Complex64> {
    rows(matrix)
        .iter()
        .map(|row| row.iter().zip(state).map(|(m, v)| m * v).sum())
        .collect()
}

/// A 2×2 and a 4×4 matrix with distinct entries: each entry `M[out][in]` is
/// read at `out · d + in`.
#[test]
fn an_entry_is_read_row_major_output_by_input() {
    let c = |value: usize| {
        Complex64::new(
            f64::from(u8::try_from(value).expect("an entry index fits in u8")),
            0.0,
        )
    };
    let one = OperatorMatrix::One([c(0), c(1), c(2), c(3)]);
    let two = OperatorMatrix::Two(std::array::from_fn(c));
    for (matrix, dimension) in [(one, 2), (two, 4)] {
        for output in 0..dimension {
            for input in 0..dimension {
                assert_eq!(
                    matrix.entry(output, input),
                    c(output * dimension + input),
                    "M[{output}][{input}] of a {dimension}×{dimension} matrix"
                );
                assert_eq!(
                    matrix.row_major()[output * dimension + input],
                    c(output * dimension + input)
                );
            }
        }
    }
}

/// An output index past a 2×2 matrix.
#[test]
#[should_panic(expected = "operator index out of range")]
fn an_output_outside_the_matrix_panics() {
    let _ = QirGate::X.matrix_at(None).entry(2, 0);
}

/// An input index past a 2×2 matrix, which a row-major index alone would
/// read from the next row.
#[test]
#[should_panic(expected = "operator index out of range")]
fn an_input_outside_the_matrix_panics() {
    let _ = QirGate::X.matrix_at(None).entry(0, 2);
}

/// Matrices whose off-diagonal entries are all zero, some with zeros on the
/// diagonal too, and one with a -0.0 off the diagonal.
#[test]
fn a_matrix_with_zero_off_diagonal_entries_is_diagonal() {
    let minus_zero = Complex64::new(-0.0, -0.0);
    let mut two = [O; 16];
    two[5] = I;
    two[15] = L;
    two[1] = minus_zero;
    for matrix in [
        QirGate::Z.matrix_at(None),
        QirGate::S.matrix_at(None),
        OperatorMatrix::One([O; 4]),
        OperatorMatrix::Two(two),
    ] {
        assert!(matrix.is_diagonal(), "{matrix:?}");
    }
}

/// Matrices with one nonzero entry off the diagonal, in each corner.
#[test]
fn a_matrix_with_a_nonzero_off_diagonal_entry_is_not_diagonal() {
    for at in [1, 2] {
        let mut one = [L, O, O, L];
        one[at] = I;
        assert!(!OperatorMatrix::One(one).is_diagonal(), "entry {at}");
    }
    for at in [1, 3, 4, 7, 11, 12, 14] {
        let mut two = [O; 16];
        two[at] = L;
        assert!(!OperatorMatrix::Two(two).is_diagonal(), "entry {at}");
    }
    assert!(!QirGate::X.matrix_at(None).is_diagonal());
}
