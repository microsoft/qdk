// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::c;
use crate::reference::ExactReference;
use crate::{MpsEngine, MpsShape, Pauli, PauliError, PauliSum, PauliTerm};

/// `pauli` applied to |0⟩ by the reference, which reads its matrix as
/// `M[out][in]`: the amplitudes of |0⟩ and |1⟩.
fn on_zero(pauli: Pauli) -> [num_complex::Complex64; 2] {
    let engine = ExactReference::new();
    let capacity = MpsShape::qubit_chain(1, 1).expect("one site forms a chain");
    let mut state = engine.prepare(&capacity).expect("one site is held");
    engine
        .apply(&mut state, &[0], &pauli.matrix())
        .expect("a Pauli matrix is unitary");
    [state.amplitude(0), state.amplitude(1)]
}

/// Y|0⟩ = i|1⟩, so in `M[out][in]` the entry M[1][0] is i, and the
/// transposed layout would give −i|1⟩.
#[test]
fn the_y_matrix_is_row_major_out_by_in() {
    assert_eq!(on_zero(Pauli::Y), [c(0.0, 0.0), c(0.0, 1.0)]);
}

#[test]
fn the_x_matrix_flips_zero_to_one() {
    assert_eq!(on_zero(Pauli::X), [c(0.0, 0.0), c(1.0, 0.0)]);
}

#[test]
fn the_z_matrix_keeps_zero() {
    assert_eq!(on_zero(Pauli::Z), [c(1.0, 0.0), c(0.0, 0.0)]);
}

#[test]
fn a_term_needs_a_factor() {
    assert_eq!(
        PauliTerm::new(c(1.0, 0.0), Vec::new()),
        Err(PauliError::NoFactors)
    );
}

#[test]
fn a_term_refuses_the_first_site_named_a_second_time() {
    assert_eq!(
        PauliTerm::new(
            c(1.0, 0.0),
            vec![(1, Pauli::X), (2, Pauli::Y), (2, Pauli::Z), (1, Pauli::Z)]
        ),
        Err(PauliError::RepeatedSite { site: 2 })
    );
}

/// Site 1 is named again after another site: every earlier factor is
/// compared, not only the previous one.
#[test]
fn a_term_refuses_a_site_named_again_after_another() {
    assert_eq!(
        PauliTerm::new(
            c(1.0, 0.0),
            vec![(1, Pauli::X), (2, Pauli::Y), (1, Pauli::Z)]
        ),
        Err(PauliError::RepeatedSite { site: 1 })
    );
}

#[test]
fn a_term_refuses_a_coefficient_that_is_not_finite() {
    for coefficient in [c(f64::NAN, 0.0), c(0.0, f64::INFINITY)] {
        assert!(
            matches!(
                PauliTerm::new(coefficient, vec![(0, Pauli::X)]),
                Err(PauliError::NonFinite(_))
            ),
            "{coefficient}"
        );
    }
}

#[test]
fn a_term_keeps_its_factors_in_the_order_given() {
    let term = PauliTerm::new(c(1.0, 0.0), vec![(3, Pauli::X), (0, Pauli::Z)])
        .expect("a term on distinct sites");
    assert_eq!(term.factors(), [(3, Pauli::X), (0, Pauli::Z)]);
}

#[test]
fn a_sum_refuses_an_identity_coefficient_that_is_not_finite() {
    assert!(matches!(
        PauliSum::new(c(f64::NEG_INFINITY, 0.0), Vec::new()),
        Err(PauliError::NonFinite(_))
    ));
}

/// A caller converting an observable may produce the same string twice,
/// with other coefficients.
#[test]
fn a_sum_keeps_terms_on_the_same_string_apart_in_the_order_given() {
    let z0 = PauliTerm::new(c(1.0, 0.0), vec![(0, Pauli::Z)]).expect("a term on one site");
    let half_z0 = PauliTerm::new(c(0.5, 0.0), vec![(0, Pauli::Z)]).expect("a term on one site");
    let sum = PauliSum::new(c(0.0, 0.0), vec![z0.clone(), half_z0.clone()]).expect("finite");
    assert_eq!(sum.terms(), [z0, half_z0]);
}
