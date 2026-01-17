// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{id, x, y, z, All, Axis, DirectedAxis, PauliMatrix, PauliObservable};

#[test]
fn neg_neg_is_identity() {
    for a in PauliObservable::all() {
        assert_eq!(a, -(-a));
    }
    assert_eq!(x(0), -(-x(0)));
    assert_eq!(y(0), -(-y(0)));
    assert_eq!(z(0), -(-z(0)));
    assert_eq!(id(0), -(-id(0)));
}

#[test]
fn qubit_then_pauli_order() {
    assert!(x(1) < x(2));
    assert!(x(1) < y(1));
    assert!(y(1) < x(2));
}

#[test]
fn axis_xor() {
    let result = (Axis::X as isize) ^ (Axis::Z as isize);
    assert_eq!(result, Axis::Y as isize);
}

#[test]
fn pauli_matrix_from_axis() {
    assert_eq!(PauliMatrix::X as isize, Axis::X as isize);
    assert_eq!(PauliMatrix::Y as isize, Axis::Y as isize);
    assert_eq!(PauliMatrix::Z as isize, Axis::Z as isize);
}

#[test]
fn directed_axis_from_axis() {
    assert_eq!(DirectedAxis::PlusX as isize, Axis::X as isize);
    assert_eq!(DirectedAxis::PlusY as isize, Axis::Y as isize);
    assert_eq!(DirectedAxis::PlusZ as isize, Axis::Z as isize);
}
