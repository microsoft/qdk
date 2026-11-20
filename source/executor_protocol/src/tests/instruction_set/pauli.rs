// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The algebra of Pauli operators, their canonical form, and their text.

use std::sync::Arc;

use crate::{Argument, Pauli};

/// Factors on one qubit multiply into one, with its phase, and drop out
/// when they cancel: `X_0 · Y_0 = i Z_0`, `Y_0 · X_0 = -i Z_0`, and
/// `(X_0 Z_3)²` is the identity.
#[test]
fn pauli_factors_on_one_qubit_multiply_into_one() {
    let xz = Pauli::x(0) * Pauli::z(3);
    for (row, product, expected) in [
        ("X_0 · Y_0", Pauli::x(0) * Pauli::y(0), "i Z_0"),
        ("Y_0 · X_0", Pauli::y(0) * Pauli::x(0), "-i Z_0"),
        ("(X_0 Z_3)²", xz.clone() * xz, "I"),
    ] {
        assert_eq!(product.to_string(), expected, "{row}");
    }
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

/// What a Pauli writes, it reads back: messages and loaders agree.
#[test]
fn pauli_text_reads_back_as_the_same_pauli() {
    for p in [
        Pauli::x(0) * Pauli::z(2),
        -Pauli::z(0),
        Pauli::x(1) * Pauli::z(1),
        Pauli::x(0) * Pauli::x(0),
        -(Pauli::z(1) * Pauli::z(1)),
        Pauli::x(0) * Pauli::y(0) * Pauli::z(0),
    ] {
        assert_eq!(
            Argument::pauli(&p.to_string()),
            Ok(Argument::Pauli(Arc::new(p.clone()))),
            "{p}"
        );
    }
}

/// A peer's set names its Paulis in Qodec's text: any other text is
/// refused when read, naming it, rather than read as some other operator.
#[cfg(feature = "serde")]
#[test]
fn a_text_that_is_not_a_pauli_is_refused_when_read_naming_it() {
    for (row, text) in [
        ("an axis Qodec lacks", "W_0"),
        ("a qubit named twice", "X_0 Z_0"),
        ("a qubit that is not a number", "X_a"),
    ] {
        let refusal = serde_json::from_value::<Pauli>(serde_json::json!(text))
            .expect_err("a text that is not a Pauli is refused")
            .to_string();
        assert!(refusal.contains(text), "{row}: {refusal:?} names {text:?}");
    }
}
