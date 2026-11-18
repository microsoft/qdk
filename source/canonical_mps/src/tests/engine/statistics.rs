// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `CanonicalMpsStatistics`: the largest bonds at rest and inside a matrix
//! product operator's span, the splits, operators and moves of the centre
//! counted, and a fork's history.

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
                state.statistics().largest_bond_at_rest(),
                bond,
                "{name}, cutoff {cutoff}"
            );
        }
    }
}

/// Only a matrix product operator grows a transient bond: a gate on
/// neighbours leaves the largest transient bond at 1, while an operator's
/// factors multiply a bond of its span, which counts as transient only,
/// the largest bond at rest staying at what the splits kept. Each row: the
/// sites, the gates, and the largest bonds at rest and transient.
#[test]
fn only_an_operator_grows_a_transient_bond() {
    type Row = (&'static str, usize, Gates, (usize, usize));
    let half_turn = ry(std::f64::consts::FRAC_PI_2);
    let rows: [Row; 2] = [
        // A Bell pair made by a gate on neighbours, and no operator: the
        // split kept a bond of 2 at rest, and no bond was transient.
        (
            "a gate on neighbours",
            3,
            vec![(&[0], half_turn.clone()), (&[0, 1], cnot())],
            (2, 1),
        ),
        // Two Bell pairs, (0, 1) and (2, 3), at bond 2, then a CZ on sites
        // 0 and 2, an operator (fused, the CNOT on (2, 3) joins the CZ's
        // window): its factors multiply a bond of the span to 4, and its
        // splits compress it back to 2, the capacity.
        (
            "an operator",
            4,
            vec![
                (&[0], half_turn.clone()),
                (&[0, 1], cnot()),
                (&[2], half_turn),
                (&[2, 3], cnot()),
                (&[0, 2], cz()),
            ],
            (2, 4),
        ),
    ];
    for (row, sites, gates, (at_rest, transient)) in &rows {
        for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
            let mut state = engine.prepare(&chain(*sites, 2)).expect("prepare");
            for (on, gate) in gates {
                engine.apply(&mut state, on, gate).expect("apply");
            }
            engine
                .retained_weight(&state)
                .expect("a read applies the pending gates");
            let statistics = state.statistics();
            assert_eq!(statistics.largest_bond_at_rest(), *at_rest, "{row}, {name}");
            assert_eq!(
                statistics.largest_transient_bond(),
                *transient,
                "{row}, {name}"
            );
        }
    }
}

/// A state of one-site gates only, no split yet: both its largest bonds are
/// 1, the bond of a product state.
#[test]
fn a_state_without_splits_reports_largest_bonds_of_1() {
    let engine = Engine::new(CpuAlgebra);
    let mut state = engine.prepare(&chain(3, 4)).expect("prepare");
    engine.apply(&mut state, &[1], &ry(0.7)).expect("apply");
    let statistics = state.statistics();
    assert_eq!(statistics.splits(), 0, "no split yet");
    assert_eq!(
        (
            statistics.largest_bond_at_rest(),
            statistics.largest_transient_bond()
        ),
        (1, 1)
    );
}

/// A gate on sites 3 and 0 of 5, unfused: a matrix product operator,
/// which splits each of the three cuts of its span; then a gate on
/// neighbours takes one split.
#[test]
fn counts_the_splits_and_matrix_product_operators() {
    let engine = Engine::new(CpuAlgebra).with_fusion(false);
    let mut state = engine.prepare(&chain(5, 8)).expect("prepare");
    for site in 0..5 {
        engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
    }
    assert_eq!(
        state.statistics(),
        CanonicalMpsStatistics::default(),
        "no split yet"
    );
    engine.apply(&mut state, &[3, 0], &cnot()).expect("apply");
    engine
        .apply(&mut state, &[1, 2], &entangler(0.4))
        .expect("apply");
    let statistics = state.statistics();
    assert_eq!((statistics.splits(), statistics.mpo_applications()), (4, 1));
}

/// Reads of site 4 of 5, then of site 1, from the centre on site 0: the
/// centre moves by four sites, then by three, each move counted, and a
/// gate on neighbours, whose split carries the centre, adds none.
#[test]
fn counts_the_moves_of_the_centre() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(5, 4)).expect("prepare");
        engine.probability_of_one(&state, 4).expect("P(One)");
        assert_eq!(state.statistics().centre_moves(), 4, "{name}");
        engine.probability_of_one(&state, 1).expect("P(One)");
        assert_eq!(state.statistics().centre_moves(), 7, "{name}");
        engine.apply(&mut state, &[1, 2], &cnot()).expect("apply");
        engine
            .retained_weight(&state)
            .expect("a read splits the pair");
        let statistics = state.statistics();
        assert_eq!(statistics.splits(), 1, "{name}: precondition, a split");
        assert_eq!(statistics.centre_moves(), 7, "{name}");
    }
}

/// Grouped shots fork a state after a gate on distant sites: each fork
/// starts with the parent's splits, operators and largest bonds.
#[test]
fn a_fork_starts_with_its_parents_history() {
    let engine = Engine::new(CpuAlgebra).with_fusion(false);
    let mut state = engine.prepare(&chain(5, 8)).expect("prepare");
    for site in 0..5 {
        engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
    }
    engine.apply(&mut state, &[3, 0], &cnot()).expect("apply");
    let statistics = state.statistics();
    assert_ne!(statistics, CanonicalMpsStatistics::default(), "a history");
    let fork = engine.fork(&state).expect("fork");
    assert_eq!(fork.statistics(), statistics);
}
