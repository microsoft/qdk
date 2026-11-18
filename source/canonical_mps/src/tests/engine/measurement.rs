// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A measurement's operations on one site: reads keep the state, a
//! projection makes its value certain and returns its probability without
//! recompressing, a conditional probability is the probability after its
//! conditions, and forks once.

use super::*;

/// Reads move the orthogonality centre, never the state they read.
#[test]
fn reads_keep_the_state() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let (state, _) = circuit(&engine, 6);
        let before = engine
            .expectation(&state, &mixed_string())
            .expect("expectation");
        for site in [5, 0, 3] {
            engine.probability_of_one(&state, site).expect("P(One)");
            engine
                .conditional_probability_of_one(&state, 1, &[(site, true)])
                .expect("a possible condition");
            let after = engine
                .expectation(&state, &mixed_string())
                .expect("expectation");
            assert!(
                (after - before).norm() < 1e-12,
                "{name}: {after} vs {before}"
            );
        }
    }
}

/// A measurement of site 2 that draws One leaves the site certain of it.
#[test]
fn a_projection_makes_its_value_certain() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let (mut state, _) = circuit(&engine, 6);
        engine.project(&mut state, 2, true).expect("project");
        let p = engine.probability_of_one(&state, 2).expect("P(One)");
        assert!((p - 1.0).abs() < 1e-12, "{name}: {p}");
    }
}

/// A turn of site 0 by 0.9, which waits pending on the site, the centre,
/// then a measurement of it that draws One: the projection applies the
/// turn first, so it returns the turn's P(One), sin²(0.45), and only once,
/// so the site reads One with certainty after it.
#[test]
fn a_projection_applies_the_sites_pending_gate_once() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(3, 2)).expect("prepare");
        engine.apply(&mut state, &[0], &ry(0.9)).expect("apply");
        let p = engine.project(&mut state, 0, true).expect("project");
        let expected = 0.45f64.sin().powi(2);
        assert!((p - expected).abs() < 1e-12, "{name}: {p} vs {expected}");
        let after = engine.probability_of_one(&state, 0).expect("P(One)");
        assert!((after - 1.0).abs() < 1e-12, "{name}: {after}");
    }
}

/// A method that records each measurement's probability: the projection
/// on either value returns that value's probability before it.
#[test]
fn a_projection_returns_the_probability_of_its_value() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let (state, _) = circuit(&engine, 6);
        let p1 = engine.probability_of_one(&state, 2).expect("P(One)");
        assert!(p1 > 0.1 && p1 < 0.9, "{name}: an uncertain site, {p1}");
        for (one, expected) in [(true, p1), (false, 1.0 - p1)] {
            let mut fork = engine.fork(&state).expect("fork");
            let p = engine.project(&mut fork, 2, one).expect("project");
            assert!((p - expected).abs() < 1e-12, "{name}, {one}: {p}");
        }
    }
}

/// A sampler's conditional probability of site 5 given values of sites 0
/// and 3 is the probability of site 5 on a fork projected on those values
/// in order.
#[test]
fn a_conditional_probability_is_the_probability_after_its_conditions() {
    let given = [(0, true), (3, false)];
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let (state, _) = circuit(&engine, 6);
        let p = engine
            .conditional_probability_of_one(&state, 5, &given)
            .expect("possible conditions");
        let mut fork = engine.fork(&state).expect("fork");
        for (site, one) in given {
            engine
                .project(&mut fork, site, one)
                .expect("a possible value");
        }
        let expected = engine.probability_of_one(&fork, 5).expect("P(One)");
        assert!((p - expected).abs() < 1e-12, "{name}: {p} vs {expected}");
    }
}

/// A measurement of one qubit of a GHZ state leaves a product state, whose
/// Schmidt ranks are all 1; the engine does not recompress, so `shape`
/// still reports the bonds of 2 it held before.
#[test]
fn a_projection_does_not_recompress_the_bonds() {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let hadamard = [real(h), real(h), real(h), real(-h)];
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut ghz = engine.prepare(&chain(4, 4)).expect("prepare");
        engine.apply(&mut ghz, &[0], &hadamard).expect("apply");
        for site in 1..4 {
            engine
                .apply(&mut ghz, &[site - 1, site], &cnot())
                .expect("apply");
        }
        let before = engine.shape(&ghz).expect("shape");
        engine.project(&mut ghz, 1, true).expect("project");
        let after = engine.shape(&ghz).expect("shape");
        assert_eq!(after, before, "{name}");
        assert_eq!(after.bond_dim(0), Some(2), "{name}");
    }
}

/// Grouped shots read a site's probability given the values drawn on
/// others: a conditional probability forks at most once, as many
/// duplicates of the chain's sites as one fork makes, whatever its number
/// of conditions; without conditions it reads the state itself and
/// duplicates nothing.
#[test]
fn a_conditional_probability_forks_at_most_once() {
    let (engine, state, calls) = spied_entangled_chain(6);
    engine.fork(&state).expect("fork");
    let one_fork = calls.duplicates.get();
    assert!(one_fork > 0, "precondition: a fork duplicates the sites");
    for (given, forks) in [
        (&[][..], 0),
        (&[(1, true)], 1),
        (&[(1, true), (3, false)], 1),
    ] {
        let before = calls.duplicates.get();
        engine
            .conditional_probability_of_one(&state, 5, given)
            .expect("possible conditions");
        assert_eq!(
            calls.duplicates.get() - before,
            forks * one_fork,
            "given {given:?}"
        );
    }
}
