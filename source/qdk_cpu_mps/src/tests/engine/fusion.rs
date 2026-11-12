// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Fusion: the gates on a pair, routing SWAPs included, share one split,
//! taken by the next operation elsewhere, a read or a fork; and an
//! unfused gate leaves the centre where it came from.

use super::*;

/// A caller that takes the engine by `default()`: two CNOTs on one pair,
/// then a read, take one split.
#[test]
fn default_fuses() {
    let engine = CpuMpsEngine::default();
    let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
    engine.apply(&mut state, &[0], &ry(0.7)).expect("apply");
    engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
    engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
    engine.probability_of_one(&state, 1).expect("P(One)");
    assert_eq!(state.split_statistics().splits(), 1);
}

/// Grouped shots fork a state whose last gate left its pair unsplit: the
/// pair is split once, in the state forked, so neither that state nor its
/// forks split it again when read.
#[test]
fn a_fork_splits_a_pending_pair_once() {
    let engine = CpuMpsEngine::default();
    let mut state = engine.prepare(&chain(3, 4)).expect("prepare");
    engine.apply(&mut state, &[0], &ry(0.7)).expect("apply");
    engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
    assert_eq!(state.split_statistics().splits(), 0, "a pending pair");
    let forks = [
        engine.fork(&state).expect("fork"),
        engine.fork(&state).expect("fork"),
    ];
    for read in forks.iter().chain([&state]) {
        assert_eq!(read.split_statistics().splits(), 1, "after the forks");
        engine.probability_of_one(read, 1).expect("P(One)");
        assert_eq!(read.split_statistics().splits(), 1, "after a read");
    }
}

/// CNOT, a phase on its target, a turn of its control, CNOT again, all on
/// one pair: one split with fusion, two without, for the same state.
#[test]
fn fuses_the_gates_on_one_pair_into_one_split() {
    let mut results = Vec::new();
    for fuses in [false, true] {
        let engine = CpuMpsEngine::default().with_fusion(fuses);
        let mut state = engine.prepare(&chain(3, 4)).expect("prepare");
        for site in 0..3 {
            engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
        }
        engine.apply(&mut state, &[1, 2], &cnot()).expect("apply");
        engine.apply(&mut state, &[2], &phase(0.3)).expect("apply");
        engine.apply(&mut state, &[1], &ry(0.2)).expect("apply");
        engine.apply(&mut state, &[1, 2], &cnot()).expect("apply");
        let xy = PauliSum::new(
            c(0.0),
            vec![PauliTerm::new(c(1.0), vec![(1, Pauli::X), (2, Pauli::Y)]).expect("a term")],
        )
        .expect("an observable");
        let z = engine.expectation(&state, &xy).expect("expectation");
        let p = engine.probability_of_one(&state, 2).expect("P(One)");
        results.push((state.split_statistics().splits(), z, p));
    }
    let ((unfused, z0, p0), (fused, z1, p1)) = (results[0], results[1]);
    assert!(
        (z0 - z1).norm() < 1e-12 && (p0 - p1).abs() < 1e-12,
        "precondition: the fused and unfused runs reach the same state"
    );
    assert_eq!((unfused, fused), (2, 1));
}

/// A gate on sites 2 and 3, then a gate on sites 0 and 3: with fusion, the
/// first outbound SWAP lands on the pair the first gate left unsplit and
/// shares its split.
#[test]
fn fuses_a_routing_swap_into_the_pair_it_lands_on() {
    let mut results = Vec::new();
    for fuses in [false, true] {
        let engine = CpuMpsEngine::default().with_fusion(fuses);
        let mut state = engine.prepare(&chain(6, 4)).expect("prepare");
        for site in 0..6 {
            engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
        }
        engine
            .apply(&mut state, &[2, 3], &entangler(0.4))
            .expect("apply");
        engine.apply(&mut state, &[0, 3], &cnot()).expect("apply");
        let p: Vec<f64> = (0..6)
            .map(|site| engine.probability_of_one(&state, site).expect("P(One)"))
            .collect();
        let z = engine
            .expectation(&state, &mixed_string())
            .expect("expectation");
        let statistics = state.split_statistics();
        results.push(((statistics.splits(), statistics.swaps()), p, z));
    }
    let ((unfused, p0, z0), (fused, p1, z1)) = (&results[0], &results[1]);
    assert!(
        p0.iter().zip(p1).all(|(a, b)| (a - b).abs() < 1e-12) && (z0 - z1).norm() < 1e-12,
        "precondition: the fused and unfused runs reach the same state"
    );
    assert_eq!((*unfused, *fused), ((6, 4), (5, 4)));
}

/// A gate on sites 0 and 3, then a gate on sites 2 and 3: with fusion, the
/// last return SWAP leaves its pair, sites 2 and 3, unsplit, and the next
/// gate applies to it and shares its split: five splits for four SWAPs and
/// two gates, against six unfused, for the same state.
#[test]
fn fuses_a_gate_into_the_pair_a_route_returns_on() {
    let mut results = Vec::new();
    for fuses in [false, true] {
        let engine = CpuMpsEngine::default().with_fusion(fuses);
        let mut state = engine.prepare(&chain(5, 8)).expect("prepare");
        for site in 0..5 {
            engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
        }
        engine.apply(&mut state, &[0, 3], &cnot()).expect("apply");
        engine
            .apply(&mut state, &[2, 3], &entangler(0.4))
            .expect("apply");
        let p: Vec<f64> = (0..5)
            .map(|site| engine.probability_of_one(&state, site).expect("P(One)"))
            .collect();
        let statistics = state.split_statistics();
        results.push(((statistics.splits(), statistics.swaps()), p));
    }
    let ((unfused, p0), (fused, p1)) = (&results[0], &results[1]);
    assert!(
        p0.iter().zip(p1).all(|(a, b)| (a - b).abs() < 1e-12),
        "precondition: the fused and unfused runs reach the same state"
    );
    assert_eq!((*unfused, *fused), ((6, 4), (5, 4)));
}

/// Without fusion, a gate on sites 2 and 3 of 6 splits its pair and leaves
/// the centre on the site nearer where it came from, or on the site it was
/// when it was in the pair: on site 2 after a read of site 0, so a read of
/// site 0 again takes two QRs; on site 3 after a read of site 5, so a read
/// of site 5 again takes two; on site 2 or 3 after a read of that site, so
/// a read of it again takes none.
#[test]
fn an_unfused_gate_leaves_the_centre_on_the_side_it_came_from() {
    for (end, expected) in [(0, 2), (5, 2), (2, 0), (3, 0)] {
        let counts = Rc::new(Counts::default());
        let engine = Engine::new(Counting(Rc::clone(&counts))).with_fusion(false);
        let mut state = engine.prepare(&chain(6, 4)).expect("prepare");
        for site in 0..6 {
            engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
        }
        engine.probability_of_one(&state, end).expect("P(One)");
        engine.apply(&mut state, &[2, 3], &cnot()).expect("apply");
        let before = counts.qrs.get();
        engine.probability_of_one(&state, end).expect("P(One)");
        assert_eq!(counts.qrs.get() - before, expected, "back to site {end}");
    }
}

/// Bell pairs on sites (1, 2) and (3, 4) of 6, at bond 2, and a Z⊗Z on
/// sites 2 and 3, which fusion leaves unsplit; then a Z⊗Z on sites 0 and
/// 3, whose first outbound SWAP lands on that pair and moves site 3 next
/// to site 1, a rank of 4 at cut 2. The landing pair's split, like an
/// unfused outbound SWAP's, keeps every value above the cutoff, twice the
/// capacity's bond here, so the route keeps all the weight.
#[test]
fn an_outbound_swap_fused_into_a_pending_pair_keeps_twice_the_bond() {
    let half_turn = ry(std::f64::consts::FRAC_PI_2);
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(6, 2)).expect("prepare");
        for (on, gate) in [
            (&[1][..], &half_turn),
            (&[1, 2], &cnot()),
            (&[3], &half_turn),
            (&[3, 4], &cnot()),
            (&[2, 3], &zz()),
        ] {
            engine.apply(&mut state, on, gate).expect("apply");
        }
        engine.apply(&mut state, &[0, 3], &zz()).expect("apply");
        let weight = engine.retained_weight(&state).expect("weight");
        assert!((weight - 1.0).abs() < 1e-12, "{name}: {weight}");
        assert_eq!(
            state.split_statistics().largest_transient_bond(),
            4,
            "{name}"
        );
    }
}
