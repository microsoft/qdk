// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::PauliTerm;

const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

fn hadamard() -> [Complex64; 4] {
    [c(H, 0.0), c(H, 0.0), c(H, 0.0), c(-H, 0.0)]
}

fn cx() -> Vec<Complex64> {
    let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
    vec![l, o, o, o, o, l, o, o, o, o, o, l, o, o, l, o]
}

fn chain(sites: usize, bond: usize) -> Mps {
    let sites = (0..sites)
        .map(|s| match (s, sites) {
            (_, 1) => vec![2],
            (0, _) => vec![2, bond],
            (s, n) if s + 1 == n => vec![bond, 2],
            _ => vec![bond, 2, bond],
        })
        .collect();
    Mps::new(sites).unwrap_or_else(|e| panic!("{e}"))
}

fn bell(engine: ExactReference, sites: usize) -> ReferenceState {
    let mut state = engine
        .prepare(&chain(sites, 2))
        .unwrap_or_else(|e| panic!("{e}"));
    engine
        .apply(&mut state, &[0], &hadamard())
        .unwrap_or_else(|e| panic!("{e}"));
    engine
        .apply(&mut state, &[0, 1], &cx())
        .unwrap_or_else(|e| panic!("{e}"));
    state
}

#[test]
fn a_prepared_state_is_all_zeros_and_a_product_chain() {
    let engine = ExactReference::new();
    let state = engine
        .prepare(&chain(3, 4))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(state.sites(), 3);
    assert_eq!(state.amplitude(0), c(1.0, 0.0));
    assert!((1..8).all(|i| state.amplitude(i) == c(0.0, 0.0)));
    assert_eq!(engine.shape(&state).map(|m| m.max_bond()), Ok(1));
    assert_eq!(engine.probability_of_one(&state, 2), Ok(0.0));
}

#[test]
fn the_first_site_of_an_operator_is_its_most_significant_bit() {
    // CX on [0, 1] flips site 1 when site 0 is set; on [1, 0], the reverse.
    let engine = ExactReference::new();
    let x = Pauli::X.matrix();
    let mut state = engine
        .prepare(&chain(2, 2))
        .unwrap_or_else(|e| panic!("{e}"));
    engine
        .apply(&mut state, &[0], &x)
        .unwrap_or_else(|e| panic!("{e}"));
    engine
        .apply(&mut state, &[0, 1], &cx())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(state.amplitude(0b11), c(1.0, 0.0));
    engine
        .apply(&mut state, &[1, 0], &cx())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(state.amplitude(0b10), c(1.0, 0.0));
}

#[test]
fn a_bell_pair_has_bond_two_correlated_values_and_known_expectations() {
    let engine = ExactReference::new();
    let state = bell(engine, 3);
    let shape = engine.shape(&state).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((shape.bond_dim(0), shape.bond_dim(1)), (Some(2), Some(1)));
    let p1 = engine
        .probability_of_one(&state, 1)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!((p1 - 0.5).abs() < 1e-12);
    assert_eq!(
        engine.conditional_probability_of_one(&state, 1, &[(0, true)]),
        Ok(1.0)
    );
    assert_eq!(
        engine.conditional_probability_of_one(&state, 1, &[(0, false)]),
        Ok(0.0)
    );
    assert_eq!(
        engine.conditional_probability_of_one(&state, 2, &[(2, true)]),
        Err(ReferenceError::ZeroProbabilityCondition)
    );
    let term =
        |f: Vec<(usize, Pauli)>| PauliTerm::new(c(1.0, 0.0), f).unwrap_or_else(|e| panic!("{e}"));
    // ⟨XX⟩ = 1, ⟨ZZ⟩ = 1, ⟨YY⟩ = −1, ⟨Z₀⟩ = 0 on a Bell pair.
    for (factors, expected) in [
        (vec![(0, Pauli::X), (1, Pauli::X)], 1.0),
        (vec![(0, Pauli::Z), (1, Pauli::Z)], 1.0),
        (vec![(0, Pauli::Y), (1, Pauli::Y)], -1.0),
        (vec![(0, Pauli::Z)], 0.0),
    ] {
        let sum = PauliSum::new(c(0.5, 0.0), vec![term(factors)]).unwrap_or_else(|e| panic!("{e}"));
        let value = engine
            .expectation(&state, &sum)
            .unwrap_or_else(|e| panic!("{e}"));
        assert!((value - c(0.5 + expected, 0.0)).norm() < 1e-12, "{value}");
    }
}

#[test]
fn a_projection_renormalizes_and_a_fork_is_independent() {
    let engine = ExactReference::new();
    let state = bell(engine, 2);
    let mut copy = engine.fork(&state).unwrap_or_else(|e| panic!("{e}"));
    engine
        .project(&mut copy, 0, true, 0.5)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!((copy.amplitude(0b11) - c(1.0, 0.0)).norm() < 1e-12);
    assert_eq!(copy.amplitude(0b00), c(0.0, 0.0));
    assert_eq!(engine.shape(&copy).map(|m| m.max_bond()), Ok(1));
    assert!(
        (state.amplitude(0b00) - c(H, 0.0)).norm() < 1e-12,
        "the original is untouched"
    );
    for p in [0.0, -0.5, 1.5, f64::NAN] {
        assert_eq!(
            engine
                .project(&mut copy.clone(), 0, true, p)
                .map_err(|e| e.to_string()),
            Err(ReferenceError::InvalidProbability(p).to_string())
        );
    }
}

#[test]
fn samples_are_correlated_seeded_and_shot_major() {
    let engine = ExactReference::new();
    let state = bell(engine, 3);
    let seed = SampleSeed::try_from(7).unwrap_or_else(|e| panic!("{e}"));
    let values = engine
        .sample(&state, &[1, 0, 2], 1000, seed)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(values.len(), 3000);
    assert!(values.chunks_exact(3).all(|s| s[0] == s[1] && !s[2]));
    let ones = values.chunks_exact(3).filter(|s| s[0]).count();
    assert!((400..=600).contains(&ones), "{ones} of 1000");
    assert_eq!(engine.sample(&state, &[1, 0, 2], 1000, seed), Ok(values));
    let other = SampleSeed::try_from(8).unwrap_or_else(|e| panic!("{e}"));
    assert_ne!(
        engine.sample(&state, &[0], 64, seed),
        engine.sample(&state, &[0], 64, other)
    );
    assert_eq!(engine.sample(&state, &[], 10, seed), Ok(Vec::new()));
    assert_eq!(engine.sample(&state, &[0], 0, seed), Ok(Vec::new()));
    assert_eq!(
        engine.sample(&state, &[0, 0], 1, seed),
        Err(ReferenceError::RepeatedSite(0))
    );
}

#[test]
fn what_the_reference_refuses() {
    let engine = ExactReference::new();
    assert_eq!(
        engine.prepare(&chain(21, 2)).err(),
        Some(ReferenceError::TooManySites {
            requested: 21,
            max: 20
        })
    );
    let qutrit = Mps::new(vec![vec![2, 1], vec![1, 3]]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        engine.prepare(&qutrit).err(),
        Some(ReferenceError::NotQubit { site: 1, extent: 3 })
    );
    let mut state = engine
        .prepare(&chain(2, 2))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        engine.apply(&mut state, &[2], &hadamard()),
        Err(ReferenceError::SiteOutOfRange { site: 2, sites: 2 })
    );
    assert_eq!(
        engine.apply(&mut state, &[1, 1], &cx()),
        Err(ReferenceError::RepeatedSite(1))
    );
    assert_eq!(
        engine.apply(&mut state, &[0, 1], &hadamard()),
        Err(ReferenceError::MatrixSize {
            sites: 2,
            expected: 16,
            actual: 4
        })
    );
    let far = PauliTerm::new(c(1.0, 0.0), vec![(5, Pauli::Z)]).unwrap_or_else(|e| panic!("{e}"));
    let far = PauliSum::new(c(0.0, 0.0), vec![far]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        engine.expectation(&state, &far),
        Err(ReferenceError::SiteOutOfRange { site: 5, sites: 2 })
    );
}

#[test]
fn the_exact_chain_has_the_schmidt_rank_at_each_cut() {
    let engine = ExactReference::new();
    let mut ghz = engine
        .prepare(&chain(5, 4))
        .unwrap_or_else(|e| panic!("{e}"));
    engine
        .apply(&mut ghz, &[0], &hadamard())
        .unwrap_or_else(|e| panic!("{e}"));
    for s in 0..4 {
        engine
            .apply(&mut ghz, &[s, s + 1], &cx())
            .unwrap_or_else(|e| panic!("{e}"));
    }
    let shape = engine.shape(&ghz).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        (0..4).map(|cut| shape.bond_dim(cut)).collect::<Vec<_>>(),
        vec![Some(2); 4]
    );
    // Two Bell pairs across the middle cut of four sites: rank 4 there,
    // reported though the capacity's bond is 2: the reference never
    // truncates.
    let mut pairs = engine
        .prepare(&chain(4, 2))
        .unwrap_or_else(|e| panic!("{e}"));
    for (a, b) in [(0, 2), (1, 3)] {
        engine
            .apply(&mut pairs, &[a], &hadamard())
            .unwrap_or_else(|e| panic!("{e}"));
        engine
            .apply(&mut pairs, &[a, b], &cx())
            .unwrap_or_else(|e| panic!("{e}"));
    }
    let shape = engine.shape(&pairs).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(shape.bond_dim(1), Some(4));
    assert!(!shape.fits_within(&chain(4, 2)));
}
