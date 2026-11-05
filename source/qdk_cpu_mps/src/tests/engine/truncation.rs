// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a split discards: the tail below the cutoff, a fraction of the
//! split's own weight, then what exceeds the capacity's bond; never the
//! largest value, and nothing but zeros at a cutoff of 0.

use super::*;

/// A caller that takes the engine by `default()`: Schmidt weights of 0.9
/// and 1.1 times `DEFAULT_CUTOFF` between two sites, on either side of the
/// default cutoff; the split drops the first and keeps the second.
#[test]
fn default_truncates_at_the_default_cutoff() {
    let engine = CpuMpsEngine::default();
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

/// Schmidt weights 0.8 and 0.2 between two sites: a cutoff above 0.2 keeps
/// one value and a weight of 0.8, one below keeps both.
#[test]
fn cuts_off_the_tail_below_the_cutoff() {
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

/// Two pairs, (0, 1) and (2, 3), each of Schmidt weights 0.9 and 0.1, then
/// a caller's SWAP of sites 1 and 2 on a chain of bond 2: the middle cut's
/// weights are 0.81, 0.09, 0.09 and 0.01. The cutoff 0.095 keeps three,
/// then the capacity two, a weight of 0.9; the capacity applied first
/// would leave the cutoff to drop a 0.09, a bond of 1 and a weight of 0.81.
#[test]
fn the_capacity_caps_the_bond_the_cutoff_keeps() {
    let theta = 2.0 * 0.9f64.sqrt().acos();
    for (engine, name) in both_fusions(0.095) {
        let mut state = engine.prepare(&chain(4, 2)).expect("prepare");
        for (a, b) in [(0, 1), (2, 3)] {
            engine.apply(&mut state, &[a], &ry(theta)).expect("apply");
            engine.apply(&mut state, &[a, b], &cnot()).expect("apply");
        }
        engine.apply(&mut state, &[1, 2], &swap()).expect("apply");
        let shape = engine.shape(&state).expect("shape");
        assert_eq!(shape.bond_dim(1), Some(2), "{name}");
        let weight = engine.retained_weight(&state).expect("weight");
        assert!((weight - 0.9).abs() < 1e-12, "{name}: {weight}");
    }
}

/// Projects `site` on its likelier value, then turns it to Zero.
fn settle_on_zero(engine: &CpuMpsEngine, state: &mut CanonicalMps, site: usize) {
    let one = engine.probability_of_one(state, site).expect("P(One)") > 0.5;
    engine.project(state, site, one).expect("project");
    if one {
        let x = [c(0.0), c(1.0), c(1.0), c(0.0)];
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

/// A Bell pair at a cutoff just below 1: the split keeps one of its two
/// equal values, a bond of 1 and half the weight. (The two values' sum is
/// exact, so the cutoff alone stops at one here; the rule that never drops
/// the first value acts only where three values or more round at a cutoff
/// within a few ulps of 1, which gates cannot reach: from a cutoff of 3/4
/// every bond stays 1.)
#[test]
fn a_split_keeps_its_largest_value_at_any_cutoff() {
    let cutoff = 1.0 - f64::EPSILON / 2.0;
    for (engine, name) in both_fusions(cutoff) {
        let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
        engine
            .apply(&mut state, &[0], &ry(std::f64::consts::FRAC_PI_2))
            .expect("apply");
        engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
        assert_eq!(
            engine.shape(&state).expect("shape").largest_bond(),
            1,
            "{name}"
        );
        let weight = engine.retained_weight(&state).expect("weight");
        assert!((weight - 0.5).abs() < 1e-12, "{name}: {weight}");
    }
}

/// A cutoff is a fraction of a split's weight: one below 0, of 1 or more,
/// or NaN is a caller's error, and the engine panics on it.
#[test]
fn a_cutoff_must_lie_in_zero_to_one() {
    for cutoff in [-1e-300, 1.0, 2.0, f64::INFINITY, f64::NAN] {
        let payload = std::panic::catch_unwind(|| CpuMpsEngine::with_cutoff(cutoff))
            .expect_err("a cutoff outside [0, 1)");
        let message = payload
            .downcast_ref::<String>()
            .expect("a formatted panic message");
        assert!(message.contains("in [0, 1)"), "{cutoff}: {message}");
    }
}

/// The Bell-basis change, which takes |00⟩ to (|00⟩ + |11⟩)/√2 with two
/// Schmidt values that are the same float, at a cutoff of 1/2: the smaller
/// value's square is exactly the cutoff's share of the sum, and the split
/// discards it, since the cutoff drops a tail whose squares sum to at most
/// that share.
#[test]
fn a_tail_equal_to_the_cutoffs_share_is_discarded() {
    let a = c(std::f64::consts::FRAC_1_SQRT_2);
    let z = c(0.0);
    #[rustfmt::skip]
    let bell = [
        a, z, z, a,
        z, a, a, z,
        z, a, -a, z,
        a, z, z, -a,
    ];
    for (engine, name) in both_fusions(0.5) {
        let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
        engine.apply(&mut state, &[0, 1], &bell).expect("apply");
        assert_eq!(
            engine.shape(&state).expect("shape").largest_bond(),
            1,
            "{name}"
        );
    }
}

/// A cutoff of zero: a gate that leaves a product state keeps a bond of 1,
/// and a Schmidt weight of 1e-20, which the default cutoff drops, is kept.
/// The product state's second value never reaches the cutoff: the SVD
/// leaves out a value that is exactly zero (`LinearAlgebra::svd`), so its
/// case pins that the engine keeps no zero value, not the cutoff's
/// boundary, which `a_tail_equal_to_the_cutoffs_share_is_discarded` pins.
#[test]
fn a_cutoff_of_zero_discards_only_zeros() {
    let tiny = 2.0 * 1e-10f64.asin();
    for (angle, bond) in [(0.0, 1), (tiny, 2)] {
        for (engine, name) in both_fusions(0.0) {
            let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
            engine.apply(&mut state, &[0], &ry(angle)).expect("apply");
            engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
            let shape = engine.shape(&state).expect("shape");
            assert_eq!(shape.largest_bond(), bond, "{name}, angle {angle}");
        }
    }
}
