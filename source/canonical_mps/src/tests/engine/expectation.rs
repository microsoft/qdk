// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Expectations: the value of an observable wherever the centre and its
//! terms lie, terms that share their walk each counted, an observable of
//! no terms, and a value as large as an `f64` holds.

use super::*;
use Pauli::{X, Y, Z};

/// The observable `identity + Σ coefficient · factors`.
fn observable(identity: Complex64, terms: &[(Complex64, &[(usize, Pauli)])]) -> PauliSum {
    let terms = terms
        .iter()
        .map(|(coefficient, factors)| {
            PauliTerm::new(*coefficient, factors.to_vec()).expect("a term")
        })
        .collect();
    PauliSum::new(identity, terms).expect("an observable")
}

/// `circuit` on 6 sites, then one-site gates left pending on sites 0, 2
/// and 5, and a read that moves the centre to `centre`.
fn entangled_with_the_centre_on(engine: &Engine, centre: usize) -> (State, ReferenceState) {
    let reference = ExactReference::new();
    let (mut state, mut exact_state) = circuit(engine, 6);
    engine
        .probability_of_one(&state, centre)
        .expect("a read moves the centre");
    for (site, angle) in [(0, 0.3), (2, -0.8), (5, 1.1)] {
        engine
            .apply(&mut state, &[site], &ry(angle))
            .expect("apply");
        reference
            .apply(&mut exact_state, &[site], &ry(angle))
            .expect("apply");
    }
    (state, exact_state)
}

/// `observable`'s value on `state` and on the reference's `exact_state`
/// agree within 1e-12.
fn assert_value_agrees(
    engine: &Engine,
    state: &State,
    exact_state: &ReferenceState,
    observable: &PauliSum,
    context: &str,
) {
    let value = engine.expectation(state, observable).expect("expectation");
    let expected = ExactReference::new()
        .expectation(exact_state, observable)
        .expect("expectation");
    assert!(
        (value - expected).norm() < 1e-12,
        "{context}: {value} vs {expected}"
    );
}

/// An observable of terms left of the centre, right of it and across it,
/// with an identity coefficient, on a state whose centre a read left at
/// one end, in the middle or at the other end, with one-site gates
/// pending: its value is the reference's wherever the centre lies.
#[test]
fn an_expectation_reads_the_value_wherever_the_centre_lies() {
    let o = observable(
        Complex64::new(0.7, -0.2),
        &[
            (real(0.5), &[(0, Z)]),
            (Complex64::new(0.0, 1.5), &[(1, X), (2, Y)]),
            (real(-0.8), &[(4, Z), (5, X)]),
            (real(1.2), &[(2, X), (3, Z), (4, Y)]),
            (real(0.3), &[(0, Y), (5, Y)]),
        ],
    );
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        for centre in [0, 3, 5] {
            let (state, exact_state) = entangled_with_the_centre_on(&engine, centre);
            assert_value_agrees(
                &engine,
                &state,
                &exact_state,
                &o,
                &format!("{name}, centre {centre}"),
            );
        }
    }
}

/// A chemistry reading's terms share their first factors and their last
/// ones: terms that start with the same factor, the same site with
/// another Pauli, or a term that is the prefix of another, and terms that
/// end on the same factor. Each counts once, with its own coefficient, as
/// the reference reads them.
#[test]
fn terms_that_share_their_walk_each_count() {
    let o = observable(
        real(0.0),
        &[
            (real(1.0), &[(1, Z)]),
            (real(2.0), &[(1, Z), (2, X)]),
            (real(3.0), &[(1, Z), (2, X), (4, Y)]),
            (real(5.0), &[(1, Z), (2, Y)]),
            (real(7.0), &[(1, X), (2, X)]),
            (real(11.0), &[(0, X), (4, Y)]),
            (real(13.0), &[(3, Z), (4, Y)]),
        ],
    );
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        for centre in [0, 2, 5] {
            let (state, exact_state) = entangled_with_the_centre_on(&engine, centre);
            assert_value_agrees(
                &engine,
                &state,
                &exact_state,
                &o,
                &format!("{name}, centre {centre}"),
            );
        }
    }
}

/// An observable of no terms is its identity coefficient, whatever the
/// state.
#[test]
fn an_observable_without_terms_reads_its_identity_coefficient() {
    let identity = Complex64::new(-0.4, 2.5);
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let (state, _) = circuit(&engine, 6);
        let value = engine
            .expectation(&state, &observable(identity, &[]))
            .expect("expectation");
        assert_eq!(value, identity, "{name}");
    }
}

/// One qubit grown by the gate (1 + 1e-10)·I, which the unitarity check
/// accepts, read with the observable `f64::MAX`·Z: the normalised value is
/// `f64::MAX`, finite, and the engine returns it, as it divides each term
/// by the norm before its coefficient multiplies it.
#[test]
fn an_expectation_is_finite_whenever_its_normalised_value_is() {
    let s = 1.0 + 1e-10;
    let grow = [real(s), real(0.0), real(0.0), real(s)];
    let largest = observable(real(0.0), &[(real(f64::MAX), &[(0, Z)])]);
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(1, 1)).expect("prepare");
        engine
            .apply(&mut state, &[0], &grow)
            .expect("a gate within the unitarity tolerance");
        let value = engine.expectation(&state, &largest).expect("expectation");
        assert!(value.is_finite(), "{name}: {value}");
        assert!(
            (value.re / f64::MAX - 1.0).abs() < 1e-15 && value.im == 0.0,
            "{name}: {value}"
        );
    }
}
