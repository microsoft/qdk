// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The algebra of Pauli operators and their canonical form.

use crate::Pauli;

/// `X_0 · Y_0 = i Z_0`, and `Y_0 · X_0 = -i Z_0`.
#[test]
fn pauli_factors_on_one_qubit_multiply_into_one() {
    assert_eq!((Pauli::x(0) * Pauli::y(0)).to_string(), "i Z_0");
    assert_eq!((Pauli::y(0) * Pauli::x(0)).to_string(), "-i Z_0");
}

#[test]
fn pauli_squared_is_the_identity() {
    let xz = Pauli::x(0) * Pauli::z(3);
    assert_eq!((xz.clone() * xz).to_string(), "I");
}

/// `Z_2 · X_0` is written `X_0 Z_2`, so products in either order are equal.
#[test]
fn a_pauli_is_written_in_qubit_order() {
    assert_eq!(Pauli::z(2) * Pauli::x(0), Pauli::x(0) * Pauli::z(2));
    assert_eq!((Pauli::z(2) * Pauli::x(0)).to_string(), "X_0 Z_2");
}

/// Negated twice, a Pauli is unchanged; the identity has a sign too:
/// `-(X_0 · X_0)` is `-I`.
#[test]
fn pauli_negation_flips_its_sign() {
    assert_eq!((-Pauli::z(0)).to_string(), "-Z_0");
    assert_eq!(-(-Pauli::z(0)), Pauli::z(0));
    assert_eq!((-(Pauli::x(0) * Pauli::x(0))).to_string(), "-I");
}
