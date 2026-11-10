// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `MeetHalfway`: its route takes half the SWAPs and leaves the sites where
//! they met; every call still answers in the caller's sites; its deferred
//! cut keeps a state that fits the capacity, and the first two-qubit gate,
//! read or fork after a bond over the capacity restores the caller's order
//! and cuts it there; its splits out of that order count as transient; and
//! its guard falls back to `swap-back-2x` when the capacity binds or the
//! order drifts past one half, and only then.

use super::*;

/// The engine routing by `MeetHalfway`, with and without fusion, each with
/// its setting's name.
fn meet_halfway(cutoff: f64) -> [(CpuMpsEngine, &'static str); 2] {
    both_fusions(cutoff).map(|(engine, name)| (engine.with_routing(Routing::MeetHalfway), name))
}

/// The SWAPs the state's history took.
fn swaps(state: &CanonicalMps) -> u64 {
    state.split_statistics().swaps()
}

/// The X gate.
fn x() -> Vec<Complex64> {
    vec![c(0.0), c(1.0), c(1.0), c(0.0)]
}

/// A state of 4 qubits where `MeetHalfway` fell back because the capacity
/// bound: a CNOT on sites 0 and 3 after a turn by π/2 on site 0 makes a
/// Bell pair, whose bond of 2 exceeds the capacity of 1; the read of P(One)
/// that follows restores the order and cuts the bond to 1.
fn fallen_back_by_capacity(engine: &CpuMpsEngine) -> CanonicalMps {
    let mut state = engine.prepare(&chain(4, 1)).expect("prepare");
    engine
        .apply(&mut state, &[0], &ry(std::f64::consts::FRAC_PI_2))
        .expect("turn");
    engine.apply(&mut state, &[0, 3], &cnot()).expect("CNOT");
    engine.probability_of_one(&state, 0).expect("P(One)");
    state
}

/// A turn that leaves P(One) at 0.2: a CNOT from it makes a pair whose
/// larger Schmidt value is unique, so a cut to a bond of 1 keeps
/// `|0…0⟩` whatever the SVD's order of equal values.
fn leaning_turn() -> Vec<Complex64> {
    ry(2.0 * 0.2_f64.sqrt().asin())
}

/// A state of 4 qubits at a capacity of 1 whose `MeetHalfway` route left a
/// bond of 2 over it: a CNOT on sites 0 and 3 after [`leaning_turn`] on
/// site 0, with no read after it.
fn over_capacity(engine: &CpuMpsEngine) -> CanonicalMps {
    let mut state = engine.prepare(&chain(4, 1)).expect("prepare");
    engine
        .apply(&mut state, &[0], &leaning_turn())
        .expect("turn");
    engine.apply(&mut state, &[0, 3], &cnot()).expect("CNOT");
    state
}

/// On 4 sites, a gate on sites 0 and 3 moves each one step toward the
/// other: 2 SWAPs, where `swap-back-2x` takes 4. The two qubits then sit
/// side by side, so a second gate on them takes no SWAP.
#[test]
fn a_route_takes_half_the_swaps_and_leaves_the_sites_together() {
    for (engine, name) in meet_halfway(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(4, 4)).expect("prepare");
        engine.apply(&mut state, &[0, 3], &cnot()).expect("CNOT");
        assert_eq!(swaps(&state), 2, "{name}: the route");
        engine.apply(&mut state, &[3, 0], &cz()).expect("CZ");
        assert_eq!(swaps(&state), 2, "{name}: the gate that follows");
    }
}

/// A route moves the site on the centre's side first, so the centre rides
/// along and crosses the span once: from either end of 6 sites, a gate on
/// sites 0 and 5 takes 2 QRs, to carry the centre from the first site's
/// stop to the second site's start.
#[test]
fn a_route_crosses_the_span_once() {
    for (centre, fuses) in [(0, true), (5, true), (0, false), (5, false)] {
        let counts = Rc::new(Counts::default());
        let engine = Engine::new(Counting(Rc::clone(&counts)))
            .with_fusion(fuses)
            .with_routing(Routing::MeetHalfway);
        let mut state = engine.prepare(&chain(6, 8)).expect("prepare");
        for site in 0..6 {
            engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
        }
        engine.probability_of_one(&state, centre).expect("P(One)");
        let before = counts.qrs.get();
        engine.apply(&mut state, &[0, 5], &cnot()).expect("apply");
        assert_eq!(swaps(&state), 4, "centre {centre}, fuses {fuses}");
        assert_eq!(
            counts.qrs.get() - before,
            2,
            "centre {centre}, fuses {fuses}"
        );
    }
}

/// After a route leaves sites moved (X on 0, a CNOT on 0 and 5, X on 2,
/// so qubits 0, 2 and 5 hold One), every call names the caller's sites:
/// P(One), its conditional, the expectation, samples and projections.
#[test]
fn every_call_answers_in_the_callers_sites() {
    for (engine, name) in meet_halfway(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(6, 8)).expect("prepare");
        engine.apply(&mut state, &[0], &x()).expect("X");
        engine.apply(&mut state, &[0, 5], &cnot()).expect("CNOT");
        assert_eq!(swaps(&state), 4, "{name}: the sites moved");
        engine.apply(&mut state, &[2], &x()).expect("X");
        for (site, expected) in [(0, 1.0), (1, 0.0), (2, 1.0), (3, 0.0), (4, 0.0), (5, 1.0)] {
            let p = engine.probability_of_one(&state, site).expect("P(One)");
            assert!((p - expected).abs() < 1e-12, "{name}, site {site}: {p}");
        }
        let given = engine
            .conditional_probability_of_one(&state, 5, &[(0, true), (3, false)])
            .expect("P(One | given)");
        assert!((given - 1.0).abs() < 1e-12, "{name}: {given}");
        let value = engine.expectation(&state, &z_string(&[5])).expect("⟨Z₅⟩");
        assert!((value - c(-1.0)).norm() < 1e-12, "{name}: {value}");
        let values = engine.sample(&state, &[5, 1], 4, 7).expect("sample");
        assert_eq!(values, [true, false].repeat(4), "{name}");
        let kept = engine.project(&mut state, 1, false).expect("project");
        assert!((kept - 1.0).abs() < 1e-12, "{name}: {kept}");
        assert_eq!(swaps(&state), 4, "{name}: the reads moved no site");
    }
}

/// Two Bell pairs, (0, 1) and (2, 3), then a CZ on sites 0 and 2, on a
/// chain of bond 2: the route moves site 2 next to site 0, which takes a
/// bond of 4 at the middle cut while the order is not the caller's. The
/// route's split keeps it, and the restore cuts it back to 2 in the
/// caller's order, where the state fits: no weight is lost, and every read
/// agrees with the reference.
#[test]
fn a_route_keeps_a_state_that_fits_the_capacity() {
    let reference = ExactReference::new();
    for (engine, name) in meet_halfway(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(4, 2)).expect("prepare");
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
        assert_reads_agree(&engine, &state, &exact_state, 1e-12);
        let shape = engine.shape(&state).expect("shape");
        assert!(shape.fits_within(&chain(4, 2)), "{name}: {shape:?}");
    }
}

/// `shape` describes the caller's cuts: after a Bell pair on sites 0 and
/// 3, every cut of the caller's order has a bond of 2, though in the order
/// the route left (0 and 3 side by side) two cuts would have 1. So `shape`
/// moves the sites home (2 SWAPs), and, the capacity not binding, the
/// state keeps routing by `MeetHalfway`: the next gate on 0 and 3 takes 2
/// SWAPs again.
#[test]
fn shape_restores_the_callers_order() {
    for (engine, name) in meet_halfway(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(4, 4)).expect("prepare");
        engine
            .apply(&mut state, &[0], &ry(std::f64::consts::FRAC_PI_2))
            .expect("turn");
        engine.apply(&mut state, &[0, 3], &cnot()).expect("CNOT");
        let shape = engine.shape(&state).expect("shape");
        let bonds: Vec<_> = (0..3).map(|cut| shape.bond_dim(cut)).collect();
        assert_eq!(bonds, [Some(2); 3], "{name}");
        assert_eq!(swaps(&state), 4, "{name}: the restore");
        engine.apply(&mut state, &[0, 3], &cz()).expect("CZ");
        assert_eq!(swaps(&state), 6, "{name}: still meeting halfway");
    }
}

/// Once a bond exceeds the capacity, the first read restores the order and
/// cuts the bond in the caller's order, losing what `swap-back-2x` loses
/// (half the weight of a Bell pair cut to a bond of 1), and the state
/// routes by `swap-back-2x` from then on: the next gate on 0 and 3 takes 4
/// SWAPs, not 2.
#[test]
fn a_bond_over_the_capacity_falls_back_for_good() {
    for (engine, name) in meet_halfway(DEFAULT_CUTOFF) {
        let mut state = fallen_back_by_capacity(&engine);
        assert_eq!(swaps(&state), 4, "{name}: the route and the restore");
        let weight = engine.retained_weight(&state).expect("weight");
        assert!((weight - 0.5).abs() < 1e-12, "{name}: {weight}");
        let shape = engine.shape(&state).expect("shape");
        assert!(shape.fits_within(&chain(4, 1)), "{name}: {shape:?}");
        engine.apply(&mut state, &[0, 3], &cz()).expect("CZ");
        assert_eq!(swaps(&state), 8, "{name}: swap-back-2x");
    }
}

/// Every read after a bond over the capacity answers on the state cut in
/// the caller's order, as `swap-back-2x` does: after [`over_capacity`],
/// whose cut keeps `|0000⟩`, P(One) is 0, not the 0.2 of the uncut state,
/// and so on for the conditional, the expectation, samples and projections.
#[test]
fn a_read_after_a_bond_over_the_capacity_answers_as_swap_back() {
    for ((swap_back, name), (meeting, _)) in both_fusions(DEFAULT_CUTOFF)
        .into_iter()
        .zip(meet_halfway(DEFAULT_CUTOFF))
    {
        type Read = fn(&CpuMpsEngine, &mut CanonicalMps) -> Vec<f64>;
        let reads: [(&str, Read); 5] = [
            ("P(One)", |engine, state| {
                vec![engine.probability_of_one(state, 3).expect("P(One)")]
            }),
            // Given nothing: given values would fork the state, and a fork
            // restores it first.
            ("P(One | nothing)", |engine, state| {
                let given = engine
                    .conditional_probability_of_one(state, 3, &[])
                    .expect("P(One | nothing)");
                vec![given]
            }),
            ("⟨Z₀⟩", |engine, state| {
                let value = engine.expectation(state, &z_string(&[0])).expect("⟨Z₀⟩");
                vec![value.re, value.im]
            }),
            ("sample", |engine, state| {
                let values = engine.sample(state, &[0, 3], 16, 7).expect("sample");
                values.into_iter().map(f64::from).collect()
            }),
            ("project", |engine, state| {
                vec![engine.project(state, 3, false).expect("project")]
            }),
        ];
        for (read, answer) in reads {
            let expected = answer(&swap_back, &mut over_capacity(&swap_back));
            let actual = answer(&meeting, &mut over_capacity(&meeting));
            assert_eq!(expected.len(), actual.len(), "{name}, {read}");
            for (e, a) in expected.iter().zip(&actual) {
                assert!((e - a).abs() < 1e-12, "{name}, {read}: {actual:?}");
            }
        }
    }
}

/// The first gate after a bond over the capacity restores the order and
/// falls back: after [`over_capacity`] (2 SWAPs), a CZ on sites 0 and 3,
/// side by side in the chain, takes the restore's 2 SWAPs and
/// `swap-back-2x`'s 4. Without fusion only: with it, the CNOT's pair is
/// still unsplit, so the CZ fuses into it and the restore waits for the
/// next operation (Fusion and the deferred cut, in `routing.rs`).
#[test]
fn a_gate_after_a_bond_over_the_capacity_restores_and_falls_back() {
    let engine = CpuMpsEngine::with_cutoff(DEFAULT_CUTOFF)
        .with_fusion(false)
        .with_routing(Routing::MeetHalfway);
    let mut state = over_capacity(&engine);
    assert_eq!(swaps(&state), 2, "the route");
    engine.apply(&mut state, &[0, 3], &cz()).expect("CZ");
    assert_eq!(swaps(&state), 8, "the restore and swap-back-2x");
}

/// A restore that needs more than the capacity at a cut cuts it there and
/// falls back. On a chain of bonds 2, 2 and 1, a CNOT on sites 0 and 3
/// after [`leaning_turn`] leaves the qubits as `[1, 0, 3, 2]` with bonds
/// 1, 2 and 1, within the capacity; `shape`'s restore then moves site 3
/// across the last cut, whose bond would be 2: it is cut to 1 (`|0000⟩`
/// kept, weight 0.8), and the next gate on 0 and 3 takes 4 SWAPs.
#[test]
fn a_restore_over_the_capacity_cuts_it_and_falls_back() {
    let capacity = MpsShape::new(vec![vec![2, 2], vec![2, 2, 2], vec![2, 2, 1], vec![1, 2]])
        .expect("a chain of bonds 2, 2, 1");
    for (engine, name) in meet_halfway(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&capacity).expect("prepare");
        engine
            .apply(&mut state, &[0], &leaning_turn())
            .expect("turn");
        engine.apply(&mut state, &[0, 3], &cnot()).expect("CNOT");
        let shape = engine.shape(&state).expect("shape");
        assert!(shape.fits_within(&capacity), "{name}: {shape:?}");
        assert_eq!(swaps(&state), 4, "{name}: the route and the restore");
        let weight = engine.retained_weight(&state).expect("weight");
        assert!((weight - 0.8).abs() < 1e-12, "{name}: {weight}");
        engine.apply(&mut state, &[0, 3], &cz()).expect("CZ");
        assert_eq!(swaps(&state), 8, "{name}: swap-back-2x");
    }
}

/// A fork restores the state it forks, once, rather than leaving each of
/// its forks (a simulator's shots) to pay the restore: after
/// [`over_capacity`] (2 SWAPs), forking leaves the parent restored.
#[test]
fn a_fork_restores_its_parent_once_for_all_forks() {
    for (engine, name) in meet_halfway(DEFAULT_CUTOFF) {
        let parent = over_capacity(&engine);
        for _ in 0..2 {
            let fork = engine.fork(&parent).expect("fork");
            assert_eq!(swaps(&fork), 4, "{name}: the fork");
        }
        assert_eq!(swaps(&parent), 4, "{name}: the parent, restored once");
    }
}

/// A split while the chain is out of the caller's order counts as
/// transient: a Bell pair on sites 0 and 3, made where the route left them
/// side by side, holds a bond of 2 that no cut of the caller's order
/// rests at yet, so the largest bond at rest stays 1.
#[test]
fn a_split_out_of_the_callers_order_counts_as_transient() {
    for (engine, name) in meet_halfway(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(4, 4)).expect("prepare");
        engine
            .apply(&mut state, &[0], &ry(std::f64::consts::FRAC_PI_2))
            .expect("turn");
        engine.apply(&mut state, &[0, 3], &cnot()).expect("CNOT");
        engine
            .retained_weight(&state)
            .expect("weight, which splits");
        let statistics = state.split_statistics();
        assert_eq!(statistics.largest_transient_bond(), 2, "{name}");
        assert_eq!(statistics.largest_bond_at_rest(), 1, "{name}");
    }
}

/// On 4 sites whose capacity holds every state, so that only the drift can
/// make `MeetHalfway` fall back: gates on (0, 3), (1, 3) and (1, 2) leave
/// the qubits in the chain as `[1, 0, 3, 2]` (drift 1/3), `[1, 3, 0, 2]`
/// (drift exactly one half: no fallback) and `[3, 1, 2, 0]` (drift 5/6),
/// after which the state restores the caller's order (5 SWAPs) and routes
/// by `swap-back-2x`. Every read agrees with the reference on the way.
#[test]
fn a_drift_past_one_half_falls_back_for_good() {
    let reference = ExactReference::new();
    for (engine, name) in meet_halfway(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(4, 4)).expect("prepare");
        let mut exact_state = reference.prepare(&exact(4)).expect("prepare");
        for site in 0..4 {
            let x = f64::from(u32::try_from(site).expect("a small site"));
            engine
                .apply(&mut state, &[site], &ry(0.3 + 0.4 * x))
                .expect("turn");
            reference
                .apply(&mut exact_state, &[site], &ry(0.3 + 0.4 * x))
                .expect("turn");
        }
        for (on, swaps_after) in [([0, 3], 2), ([1, 3], 3), ([1, 2], 10), ([0, 3], 14)] {
            engine
                .apply(&mut state, &on, &entangler(0.7))
                .expect("apply");
            reference
                .apply(&mut exact_state, &on, &entangler(0.7))
                .expect("apply");
            assert_eq!(swaps(&state), swaps_after, "{name}: after {on:?}");
            assert_reads_agree(&engine, &state, &exact_state, 1e-12);
        }
    }
}

/// A fork inherits the parent's routing state: a fork of a state that fell
/// back routes by `swap-back-2x` (4 SWAPs on 0 and 3), and a fork of a
/// state whose route left 0 and 3 side by side takes no SWAP for a gate on
/// them, and answers in the caller's sites.
#[test]
fn a_fork_inherits_the_routing_state() {
    let reference = ExactReference::new();
    for (engine, name) in meet_halfway(DEFAULT_CUTOFF) {
        let parent = fallen_back_by_capacity(&engine);
        let mut fork = engine.fork(&parent).expect("fork");
        engine.apply(&mut fork, &[0, 3], &cz()).expect("CZ");
        assert_eq!(swaps(&fork), 8, "{name}: the fork fell back too");

        let mut parent = engine.prepare(&chain(4, 4)).expect("prepare");
        let mut exact_state = reference.prepare(&exact(4)).expect("prepare");
        for (on, gate) in [(&[0][..], ry(0.9)), (&[0, 3], cnot()), (&[1], ry(0.5))] {
            engine.apply(&mut parent, on, &gate).expect("apply");
            reference.apply(&mut exact_state, on, &gate).expect("apply");
        }
        let mut fork = engine.fork(&parent).expect("fork");
        engine
            .apply(&mut fork, &[3, 0], &entangler(0.4))
            .expect("apply");
        reference
            .apply(&mut exact_state, &[3, 0], &entangler(0.4))
            .expect("apply");
        assert_eq!(swaps(&fork), 2, "{name}: the sites stayed together");
        assert_reads_agree(&engine, &fork, &exact_state, 1e-12);
    }
}
