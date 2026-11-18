// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the engine's splits discard: the tail below the engine's cutoff,
//! a fraction of the split's own weight, which leaves the retained weight,
//! and what exceeds the capacity's bond. The reference algebra's rule of
//! truncation itself is `Truncation`'s
//! (`src/algebra/cpu_algebra/truncation.rs`), tested in
//! `src/tests/algebra/cpu_algebra/truncation.rs`.

use super::*;

/// A caller that takes the engine by `new`: Schmidt weights of 0.9
/// and 1.1 times `DEFAULT_CUTOFF` between two sites, on either side of the
/// default cutoff; the split drops the first and keeps the second.
#[test]
fn new_truncates_at_the_default_cutoff() {
    let engine = Engine::new(CpuAlgebra);
    for (tail, bond) in [(0.9 * DEFAULT_CUTOFF, 1), (1.1 * DEFAULT_CUTOFF, 2)] {
        let theta = 2.0 * tail.sqrt().asin();
        let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
        engine.apply(&mut state, &[0], &ry(theta)).expect("apply");
        engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
        let shape = engine.shape(&state).expect("shape");
        assert_eq!(shape.largest_bond(), bond, "a tail of {tail:e}");
    }
}

/// Entangling gates, distant ones included, on a chain whose bond is 2:
/// once each call returns, every bond is at most the capacity's, and the
/// weight the splits discard shows in the retained weight.
#[test]
fn truncates_to_the_capacity() {
    let capacity = chain(6, 2);
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&capacity).expect("prepare");
        for site in 0..6 {
            engine.apply(&mut state, &[site], &ry(1.1)).expect("apply");
        }
        for (a, b) in [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 4),
            (4, 5),
            (0, 5),
            (2, 4),
            (1, 3),
        ] {
            engine
                .apply(&mut state, &[a, b], &entangler(1.2))
                .expect("apply");
        }
        let shape = engine.shape(&state).expect("shape");
        assert!(shape.fits_within(&capacity), "{name}: {shape:?}");
        assert_eq!(shape.largest_bond(), 2, "{name}");
        let weight = engine.retained_weight(&state).expect("weight");
        assert!(weight < 1.0 - 1e-6 && weight > 0.0, "{name}: {weight}");
    }
}

/// Schmidt weights 0.8 and 0.2 between two sites, on engines built
/// `with_cutoff`: the engine's cutoff reaches the gate's split, and the
/// weight it drops leaves the retained weight. A cutoff above 0.2 keeps
/// one value and a weight of 0.8; one below keeps both and a weight of 1.
#[test]
fn the_engines_cutoff_drops_a_splits_tail_from_the_retained_weight() {
    let theta = 2.0 * 0.2f64.sqrt().asin();
    for (cutoff, bond, weight) in [(0.25, 1, 0.8), (0.15, 2, 1.0)] {
        for (engine, name) in both_fusions(cutoff) {
            let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
            engine.apply(&mut state, &[0], &ry(theta)).expect("apply");
            engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
            assert_eq!(
                engine.shape(&state).expect("shape").largest_bond(),
                bond,
                "{name}, cutoff {cutoff}"
            );
            let kept = engine.retained_weight(&state).expect("weight");
            assert!(
                (kept - weight).abs() < 1e-12,
                "{name}, cutoff {cutoff}: {kept} vs {weight}"
            );
        }
    }
}

/// Projects `site` on its likelier value, then turns it to Zero.
fn settle_on_zero(engine: &Engine, state: &mut State, site: usize) {
    let one = engine.probability_of_one(state, site).expect("P(One)") > 0.5;
    engine.project(state, site, one).expect("project");
    if one {
        let x = [real(0.0), real(1.0), real(1.0), real(0.0)];
        engine.apply(state, &[site], &x).expect("apply");
    }
}

/// The cutoff is a fraction of the split's own weight, not of the exact
/// state's. Two scenes. A state that kept 0.6 of its weight through a bond
/// of 1, then a pair whose Schmidt weights are 1 − 1.2e-3 and 1.2e-3 of
/// what is left: at a cutoff of 1e-3 the split keeps the smaller value,
/// above the cutoff as a fraction of the split's weight, though below it as
/// a fraction of the exact state's (7.2e-4). And a pair truncated to a bond
/// of 1 for 997 rounds (a retained weight of about 7.5e-301), then Schmidt
/// weights 0.9 and 0.1 between sites 1 and 2: the split keeps both values,
/// at a cutoff of zero and at the default.
#[test]
fn the_cutoff_is_a_fraction_of_the_splits_own_weight() {
    let capacity = MpsShape::new(vec![vec![2, 1], vec![1, 2, 2], vec![2, 2]]).expect("a shape");
    let first = 2.0 * 0.6f64.sqrt().acos();
    let second = 2.0 * 1.2e-3f64.sqrt().asin();
    for (engine, name) in both_fusions(1e-3) {
        let mut state = engine.prepare(&capacity).expect("prepare");
        engine.apply(&mut state, &[0], &ry(first)).expect("apply");
        engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
        let weight = engine.retained_weight(&state).expect("weight");
        assert!((weight - 0.6).abs() < 1e-12, "{name}: {weight}");
        engine.apply(&mut state, &[2], &ry(second)).expect("apply");
        engine.apply(&mut state, &[2, 1], &cnot()).expect("apply");
        let shape = engine.shape(&state).expect("shape");
        assert_eq!(shape.bond_dim(1), Some(2), "{name}");
    }
    let tenth = 2.0 * 0.1f64.sqrt().asin();
    for (engine, name) in both_fusions_and_cutoffs() {
        let mut state = engine.prepare(&capacity).expect("prepare");
        for _ in 0..997 {
            halve_the_weight(&engine, &mut state, 0).expect("a round");
        }
        settle_on_zero(&engine, &mut state, 1);
        engine.apply(&mut state, &[1], &ry(tenth)).expect("apply");
        engine.apply(&mut state, &[1, 2], &cnot()).expect("apply");
        let shape = engine.shape(&state).expect("shape");
        assert_eq!(shape.bond_dim(1), Some(2), "{name}, 7.5e-301");
    }
}

/// A cutoff is a fraction of a split's weight: one below 0, of 1 or more,
/// or NaN is a caller's error, and the engine panics on it.
#[test]
fn a_cutoff_must_lie_in_zero_to_one() {
    for cutoff in [-1e-300, 1.0, 2.0, f64::INFINITY, f64::NAN] {
        let payload = std::panic::catch_unwind(|| Engine::new(CpuAlgebra).with_cutoff(cutoff))
            .expect_err("a cutoff outside [0, 1)");
        let message = payload
            .downcast_ref::<String>()
            .expect("a formatted panic message");
        assert!(message.contains("in [0, 1)"), "{cutoff}: {message}");
    }
}
