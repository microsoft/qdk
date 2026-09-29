// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_complex::Complex64;
use qdk_simulators::execution::{Pauli, PauliSum, PauliSumError};

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

fn apply(matrix: [Complex64; 4], state: [Complex64; 2]) -> [Complex64; 2] {
    [
        matrix[0] * state[0] + matrix[1] * state[1],
        matrix[2] * state[0] + matrix[3] * state[1],
    ]
}

#[test]
fn matrices_are_textbook_row_major() {
    let zero = [c(1.0, 0.0), c(0.0, 0.0)];
    let one = [c(0.0, 0.0), c(1.0, 0.0)];
    assert_eq!(apply(Pauli::X.matrix(), zero), one);
    // Y|0⟩ = i|1⟩ and Y|1⟩ = −i|0⟩ distinguish Y from its transpose.
    assert_eq!(apply(Pauli::Y.matrix(), zero), [c(0.0, 0.0), c(0.0, 1.0)]);
    assert_eq!(apply(Pauli::Y.matrix(), one), [c(0.0, -1.0), c(0.0, 0.0)]);
    assert_eq!(apply(Pauli::Z.matrix(), one), [c(0.0, 0.0), c(-1.0, 0.0)]);
}

#[test]
fn labels_drop_identities_and_keep_factor_order() {
    let mut sum = PauliSum::new();
    sum.push_labels(c(0.5, 0.0), "ZIX", &[4, 0, 2])
        .expect("term should be valid");
    sum.push_labels(c(0.0, 2.0), "Y", &[7])
        .expect("term should be valid");

    let terms = sum.terms();
    assert_eq!(terms.len(), 2);
    assert_eq!(terms[0].coefficient(), c(0.5, 0.0));
    assert_eq!(terms[0].factors(), [(4, Pauli::Z), (2, Pauli::X)]);
    assert_eq!(terms[1].factors(), [(7, Pauli::Y)]);
    assert_eq!(sum.identity_coefficient(), c(0.0, 0.0));
    assert_eq!(sum.max_qubit(), Some(7));
}

#[test]
fn identity_terms_accumulate_into_one_coefficient() {
    let mut sum = PauliSum::new();
    sum.push_labels(c(1.5, 0.0), "II", &[0, 3])
        .expect("identity term should be valid");
    sum.push(c(-0.5, 1.0), [])
        .expect("empty product should be the identity");

    assert!(sum.terms().is_empty());
    assert_eq!(sum.identity_coefficient(), c(1.0, 1.0));
    assert_eq!(sum.max_qubit(), None);
}

#[test]
fn invalid_terms_are_rejected_without_changing_the_sum() {
    let mut sum = PauliSum::new();
    sum.push_labels(c(1.0, 0.0), "Z", &[0])
        .expect("term should be valid");
    let before = sum.clone();

    assert_eq!(
        sum.push_labels(c(1.0, 0.0), "", &[]),
        Err(PauliSumError::EmptyTerm)
    );
    assert_eq!(
        sum.push_labels(c(1.0, 0.0), "ZZ", &[0]),
        Err(PauliSumError::LengthMismatch {
            labels: 2,
            qubits: 1
        })
    );
    assert_eq!(
        sum.push_labels(c(1.0, 0.0), "ZA", &[0, 1]),
        Err(PauliSumError::UnknownLabel { label: 'A' })
    );
    assert_eq!(
        sum.push_labels(c(1.0, 0.0), "zX", &[0, 1]),
        Err(PauliSumError::UnknownLabel { label: 'z' })
    );
    // A repeated qubit is rejected even when one of its labels is I.
    assert_eq!(
        sum.push_labels(c(1.0, 0.0), "IZ", &[3, 3]),
        Err(PauliSumError::RepeatedQubit { qubit: 3 })
    );
    assert_eq!(
        sum.push(c(1.0, 0.0), [(1, Pauli::X), (1, Pauli::Z)]),
        Err(PauliSumError::RepeatedQubit { qubit: 1 })
    );
    assert_eq!(
        sum.push(c(f64::NAN, 0.0), [(1, Pauli::X)]),
        Err(PauliSumError::NonFiniteCoefficient)
    );
    assert_eq!(
        sum.push(c(0.0, f64::INFINITY), []),
        Err(PauliSumError::NonFiniteCoefficient)
    );
    assert_eq!(sum, before);
}
