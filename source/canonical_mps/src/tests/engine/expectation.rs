// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Expectations: the value of an observable wherever the centre and its
//! terms lie, terms that share their walk each counted, an observable of
//! no terms, a value as large as an `f64` holds, and the sites an
//! expectation reads: only those between the observable and the centre,
//! each once.

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

/// `observable`'s value on `state` and on the exact reference's
/// `exact_state`
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
/// pending: its value is the exact reference's wherever the centre lies.
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
/// the exact reference reads them.
#[test]
fn terms_that_share_factors_each_count() {
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

/// The sites an expectation of `observable` reads on a chain of `sites`
/// entangled sites whose centre a read has left on `centre`.
fn expectation_reads(sites: usize, centre: usize, observable: &PauliSum) -> usize {
    let (engine, state, calls) = spied_entangled_chain(sites);
    engine.probability_of_one(&state, centre).expect("P(One)");
    let before = calls.reads.get();
    engine.expectation(&state, observable).expect("expectation");
    calls.reads.get() - before
}

/// A Z read next to the centre, at either end of a chain of 12 sites or of
/// one of 6, reads the same sites: an expectation contracts only the
/// sites between the observable's sites and the centre, wherever they lie
/// on the chain and whatever its length; a Z further from the centre takes
/// more.
#[test]
fn an_expectation_contracts_only_between_the_observable_and_the_centre() {
    let z = |site| z_string(&[site]);
    let near = expectation_reads(12, 0, &z(1));
    assert_eq!(expectation_reads(12, 10, &z(11)), near, "at the far end");
    assert_eq!(expectation_reads(12, 6, &z(5)), near, "below the centre");
    assert_eq!(expectation_reads(6, 4, &z(5)), near, "on a shorter chain");
    assert!(
        expectation_reads(12, 0, &z(5)) > near,
        "further from the centre"
    );
}

/// A chemistry reading holds many Pauli strings that share their sites:
/// six terms of one to three factors on sites 1 to 3, the centre on site
/// 0, read each of the sites 0 to 3 once, as a single Z on site 3 does;
/// with the centre on site 4 and terms on both sides of it, on sites 2 to
/// 6, each of those sites is read once, as a single Z₂Z₆ does.
#[test]
fn an_expectation_reads_each_site_once_whatever_its_terms() {
    let term = |factors| PauliTerm::new(real(0.5), factors).expect("a term");
    let reading = PauliSum::new(
        real(0.1),
        vec![
            term(vec![(1, Pauli::Z)]),
            term(vec![(2, Pauli::Z)]),
            term(vec![(3, Pauli::Z)]),
            term(vec![(1, Pauli::X), (2, Pauli::X)]),
            term(vec![(2, Pauli::Y), (3, Pauli::Y)]),
            term(vec![(1, Pauli::Z), (2, Pauli::Z), (3, Pauli::Z)]),
        ],
    )
    .expect("an observable");
    assert_eq!(expectation_reads(8, 0, &reading), 4, "the reading");
    assert_eq!(expectation_reads(8, 0, &z_string(&[3])), 4, "one Z");
    let around = PauliSum::new(
        real(0.1),
        vec![
            term(vec![(2, Pauli::Z)]),
            term(vec![(6, Pauli::Z)]),
            term(vec![(3, Pauli::X), (5, Pauli::X)]),
            term(vec![(2, Pauli::Y), (3, Pauli::Y)]),
            term(vec![(5, Pauli::Z), (6, Pauli::Z)]),
            term(vec![(2, Pauli::Z), (4, Pauli::Z), (6, Pauli::Z)]),
        ],
    )
    .expect("an observable");
    assert_eq!(expectation_reads(8, 4, &around), 5, "around the centre");
    assert_eq!(expectation_reads(8, 4, &z_string(&[2, 6])), 5, "one Z₂Z₆");
}
