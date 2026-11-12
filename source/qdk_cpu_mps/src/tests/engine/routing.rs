// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Gates on distant sites (`swap-back-2x`): they act in the caller's
//! order, a route keeps a state that fits the capacity, applies the
//! cutoff to its splits, and carries the centre without a QR.

use super::*;

/// Gates on distant sites, in both operand orders: every read agrees with
/// the reference in the caller's order, so the routing leaves no site
/// moved.
#[test]
fn gates_on_distant_sites_act_in_the_callers_order() {
    let reference = ExactReference::new();
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let (state, exact_state) = circuit(&engine, 6);
        for observable in [
            z_string(&[0]),
            z_string(&[1, 4]),
            z_string(&[0, 5]),
            mixed_string(),
        ] {
            let value = engine
                .expectation(&state, &observable)
                .expect("expectation");
            let expected = reference
                .expectation(&exact_state, &observable)
                .expect("expectation");
            assert!(
                (value - expected).norm() < 1e-12,
                "{name}: {value} vs {expected}"
            );
        }
        assert_reads_agree(&engine, &state, &exact_state, 1e-12);
        let weight = engine.retained_weight(&state).expect("weight");
        assert!((weight - 1.0).abs() < 1e-12, "{name}: {weight}");
    }
}

/// Two Bell pairs, (0, 1) and (2, 3), then a CZ on sites 0 and 2: the state
/// has bond 2 at every cut in the caller's order, but 4 at the middle cut
/// while site 2 visits site 1. The outbound SWAP's split keeps the 4, so
/// the gate loses nothing on a chain of bond 2, where a route that kept
/// only the capacity's bond on the way out would discard about 4% of the
/// weight.
#[test]
fn a_route_keeps_a_state_that_fits_the_capacity() {
    let capacity = chain(4, 2);
    let reference = ExactReference::new();
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&capacity).expect("prepare");
        let mut exact_state = reference.prepare(&exact(4)).expect("prepare");
        for (on, gate) in [
            (&[0][..], ry(0.6)),
            (&[0, 1], cnot()),
            (&[2], ry(0.4)),
            (&[2, 3], cnot()),
            (&[0, 2], cz()),
        ] {
            engine.apply(&mut state, on, &gate).expect("apply");
            reference.apply(&mut exact_state, on, &gate).expect("apply");
        }
        let weight = engine.retained_weight(&state).expect("weight");
        assert!((weight - 1.0).abs() < 1e-12, "{name}: {weight}");
        assert_eq!(
            state.split_statistics().largest_transient_bond(),
            4,
            "{name}"
        );
        assert!(
            engine.shape(&state).expect("shape").fits_within(&capacity),
            "{name}"
        );
        assert_reads_agree(&engine, &state, &exact_state, 1e-12);
    }
}

/// X on each of `sites`.
fn x_string(sites: &[usize]) -> PauliSum {
    let factors = sites.iter().map(|&s| (s, Pauli::X)).collect();
    PauliSum::new(
        c(0.0),
        vec![PauliTerm::new(c(1.0), factors).expect("a term")],
    )
    .expect("an observable")
}

/// Bell pairs on sites (0, 1) and (2, 3) fit a capacity whose bonds are
/// 2, 1 and 2: the middle cut separates the pairs. A Z⊗Z on sites 0 and 3
/// moves site 3 down to site 1; after the outbound SWAP at cut 1, the
/// sites left of it, 0 and 3, hold half of each pair, a rank of 4. That is
/// twice the capacity's bond at cut 0, the cut below, though four times
/// the one at cut 1 itself; so the route keeps all the weight, and the
/// Z⊗Z, which commutes with X⊗X⊗X⊗X, leaves its value at 1.
#[test]
fn a_route_keeps_a_state_that_fits_uneven_bonds() {
    let capacity = MpsShape::new(vec![vec![2, 2], vec![2, 2, 1], vec![1, 2, 2], vec![2, 2]])
        .expect("a chain of bonds 2, 1, 2");
    let half_turn = ry(std::f64::consts::FRAC_PI_2);
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&capacity).expect("prepare");
        for (on, gate) in [
            (&[0][..], &half_turn),
            (&[0, 1], &cnot()),
            (&[2], &half_turn),
            (&[2, 3], &cnot()),
        ] {
            engine.apply(&mut state, on, gate).expect("a Bell pair");
        }
        engine.apply(&mut state, &[0, 3], &zz()).expect("apply");
        let weight = engine.retained_weight(&state).expect("weight");
        assert!((weight - 1.0).abs() < 1e-12, "{name}: {weight}");
        let xxxx = engine
            .expectation(&state, &x_string(&[0, 1, 2, 3]))
            .expect("expectation");
        assert!((xxxx - c(1.0)).norm() < 1e-12, "{name}: {xxxx}");
        assert_eq!(
            state.split_statistics().largest_transient_bond(),
            4,
            "{name}"
        );
        assert!(
            engine.shape(&state).expect("shape").fits_within(&capacity),
            "{name}"
        );
    }
}

/// A Bell pair on sites (0, 1) and a pair on (2, 3) of Schmidt weights 0.98
/// and 0.02, at a cutoff of 0.015 that no cut of the state reaches in the
/// caller's order; then a Z⊗Z on sites 0 and 3, which moves site 3 down to
/// site 1, next to site 0. The cut right of the two then separates half of
/// each pair, weights 0.49, 0.49, 0.01 and 0.01, and the outbound SWAP's
/// split drops a 0.01 there, as every split drops its tail below the
/// cutoff, and the retained weight counts it: the route ends with a weight
/// of 0.979974491787900 (measured, the same with and without fusion), where
/// leaving the outbound split's loss out of the weight would end near 0.99.
#[test]
fn the_cutoff_applies_to_a_routes_splits() {
    let cutoff = 0.015;
    let half_turn = ry(std::f64::consts::FRAC_PI_2);
    let uneven = ry(2.0 * 0.98_f64.sqrt().acos());
    for (engine, name) in both_fusions(cutoff) {
        let mut state = engine.prepare(&chain(4, 4)).expect("prepare");
        for (on, gate) in [
            (&[0][..], &half_turn),
            (&[0, 1], &cnot()),
            (&[2], &uneven),
            (&[2, 3], &cnot()),
        ] {
            engine.apply(&mut state, on, gate).expect("apply");
        }
        let before = engine.retained_weight(&state).expect("weight");
        assert!((before - 1.0).abs() < 1e-12, "{name}: {before}");
        engine.apply(&mut state, &[0, 3], &zz()).expect("apply");
        let after = engine.retained_weight(&state).expect("weight");
        assert!(
            (after - 0.979_974_491_787_900).abs() < 1e-12,
            "{name}: {after}"
        );
    }
}

/// A read of site 3 leaves the centre there; then a CNOT on sites 0 and 4
/// routes site 4 down to site 1 and back. Each SWAP's split leaves the
/// centre on the site the next SWAP needs, so the route takes no QR.
#[test]
fn a_route_carries_the_centre_without_a_qr() {
    for fuses in [true, false] {
        let counts = Rc::new(Counts::default());
        let engine = Engine::new(Counting(Rc::clone(&counts))).with_fusion(fuses);
        let mut state = engine.prepare(&chain(6, 8)).expect("prepare");
        for site in 0..6 {
            engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
        }
        engine.probability_of_one(&state, 3).expect("P(One)");
        let before = counts.qrs.get();
        engine.apply(&mut state, &[0, 4], &cnot()).expect("apply");
        assert_eq!(
            state.split_statistics().swaps(),
            6,
            "fuses {fuses}: a route"
        );
        assert_eq!(counts.qrs.get(), before, "fuses {fuses}");
    }
}
