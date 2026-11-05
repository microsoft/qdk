// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `SplitStatistics`: the largest bonds at rest and during routes, the
//! splits and SWAPs counted, and a fork's history.

use super::*;

/// Schmidt weights 0.8 and 0.2 between two neighbours: the largest bond at
/// rest is 1 at a cutoff above 0.2, which drops the smaller value, though
/// the SVD found two, and 2 at a cutoff below it.
#[test]
fn the_largest_bond_at_rest_is_the_bond_a_gates_split_kept() {
    let theta = 2.0 * 0.2f64.sqrt().asin();
    for (cutoff, bond) in [(0.25, 1), (0.15, 2)] {
        for (engine, name) in both_fusions(cutoff) {
            let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
            engine.apply(&mut state, &[0], &ry(theta)).expect("apply");
            engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
            engine
                .retained_weight(&state)
                .expect("a read splits the pair");
            assert_eq!(
                state.split_statistics().largest_bond_at_rest(),
                bond,
                "{name}, cutoff {cutoff}"
            );
        }
    }
}

/// A Bell pair made by a gate on neighbours, and no route: the split kept a
/// bond of 2 at rest, and no split was transient, so the largest transient
/// bond is 1.
#[test]
fn a_gate_on_neighbours_leaves_no_transient_bond() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(3, 2)).expect("prepare");
        engine
            .apply(&mut state, &[0], &ry(std::f64::consts::FRAC_PI_2))
            .expect("apply");
        engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
        engine
            .retained_weight(&state)
            .expect("a read splits the pair");
        let statistics = state.split_statistics();
        assert_eq!(
            statistics.largest_bond_at_rest(),
            2,
            "{name}: precondition, a split"
        );
        assert_eq!(statistics.largest_transient_bond(), 1, "{name}");
    }
}

/// Two Bell pairs, (0, 1) and (2, 3), at bond 2, then a CZ on sites 0 and 2:
/// the outbound SWAP's split keeps a bond of 4, the gate's and the return's
/// splits keep 2, the capacity. The 4 counts as transient only: the largest
/// bond at rest stays at the capacity.
#[test]
fn a_routes_outbound_bond_is_transient_not_at_rest() {
    let half_turn = ry(std::f64::consts::FRAC_PI_2);
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(4, 2)).expect("prepare");
        for (on, gate) in [
            (&[0][..], &half_turn),
            (&[0, 1], &cnot()),
            (&[2], &half_turn),
            (&[2, 3], &cnot()),
            (&[0, 2], &cz()),
        ] {
            engine.apply(&mut state, on, gate).expect("apply");
        }
        engine
            .retained_weight(&state)
            .expect("a read splits the pair");
        let statistics = state.split_statistics();
        assert_eq!(
            statistics.largest_transient_bond(),
            4,
            "{name}: the outbound split keeps twice the capacity"
        );
        assert_eq!(statistics.largest_bond_at_rest(), 2, "{name}");
    }
}

/// A state of one-site gates only, no split yet: both its largest bonds are
/// 1, the bond of a product state.
#[test]
fn a_state_without_splits_reports_largest_bonds_of_1() {
    let engine = CpuMpsEngine::default();
    let mut state = engine.prepare(&chain(3, 4)).expect("prepare");
    engine.apply(&mut state, &[1], &ry(0.7)).expect("apply");
    let statistics = state.split_statistics();
    assert_eq!(statistics.splits(), 0, "no split yet");
    assert_eq!(
        (
            statistics.largest_bond_at_rest(),
            statistics.largest_transient_bond()
        ),
        (1, 1)
    );
}

/// A gate on sites 3 and 0 of 5, unfused: two SWAPs bring site 3 down to
/// site 1, the gate splits, and two SWAPs take it back; then a gate on
/// neighbours takes one split.
#[test]
fn counts_the_splits_and_swaps_of_a_route() {
    let engine = CpuMpsEngine::default().with_fusion(false);
    let mut state = engine.prepare(&chain(5, 8)).expect("prepare");
    for site in 0..5 {
        engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
    }
    assert_eq!(
        state.split_statistics(),
        SplitStatistics::default(),
        "no split yet"
    );
    engine.apply(&mut state, &[3, 0], &cnot()).expect("apply");
    engine
        .apply(&mut state, &[1, 2], &entangler(0.4))
        .expect("apply");
    let statistics = state.split_statistics();
    assert_eq!((statistics.splits(), statistics.swaps()), (6, 4));
}

/// Grouped shots fork a state after a routed gate: each fork starts with
/// the parent's splits, SWAPs and largest bonds.
#[test]
fn a_fork_starts_with_its_parents_history() {
    let engine = CpuMpsEngine::default().with_fusion(false);
    let mut state = engine.prepare(&chain(5, 8)).expect("prepare");
    for site in 0..5 {
        engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
    }
    engine.apply(&mut state, &[3, 0], &cnot()).expect("apply");
    let statistics = state.split_statistics();
    assert_ne!(statistics, SplitStatistics::default(), "a history");
    let fork = engine.fork(&state).expect("fork");
    assert_eq!(fork.split_statistics(), statistics);
}
