// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Gates on sites far apart, and windows on three sites, applied as a
//! matrix product operator: they act in the caller's order, keep a state
//! that fits the capacity, truncate only at the caller's cuts, grow the
//! span by the operator's rank, less the values at or below its rank
//! cutoff, and cost, once the centre is in a span of
//! `d + 1` sites, `d` splits, `d − 1` moves, and one read and one upload
//! of each site.

use super::*;

/// Gates on distant sites, in both operand orders: every read agrees with
/// the reference in the caller's order, so no site is left moved.
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

/// An MPO whose result fits the capacity loses nothing, though it grows
/// its span's bonds on the way: the weight stays 1, the shape fits, and
/// every read agrees with the reference. Each row: the capacity and the
/// gates, the last on sites far apart.
#[test]
fn an_mpo_keeps_a_state_that_fits_its_bonds() {
    type Row = (&'static str, MpsShape, Gates);
    let half_turn = ry(std::f64::consts::FRAC_PI_2);
    let rows: [Row; 2] = [
        // Two Bell pairs, (0, 1) and (2, 3), then a CZ on sites 0 and 2.
        // Unfused, the state has bond 2 at every cut before the CZ; fused,
        // the CNOT on (2, 3) joins the CZ's window. Either way the splits
        // keep every value, so the gates lose nothing on a chain of bond
        // 2.
        (
            "a CZ on two Bell pairs",
            chain(4, 2),
            vec![
                (&[0], ry(0.6)),
                (&[0, 1], cnot()),
                (&[2], ry(0.4)),
                (&[2, 3], cnot()),
                (&[0, 2], cz()),
            ],
        ),
        // A Bell pair on sites (0, 1), site 2 in |0⟩ and site 3 in |+⟩ fit
        // a capacity whose bonds are 2, 1 and 2. A CNOT from site 0 to site
        // 3, of operator rank 2, grows the middle cut's bond to 2, above
        // its capacity of 1, and its splits cut it back: X on |+⟩ is the
        // identity, so the state is unchanged and the cut keeps all the
        // weight.
        (
            "uneven bonds",
            MpsShape::new(vec![vec![2, 2], vec![2, 2, 1], vec![1, 2, 2], vec![2, 2]])
                .expect("a chain of bonds 2, 1, 2"),
            vec![
                (&[0], half_turn.clone()),
                (&[0, 1], cnot()),
                (&[3], half_turn),
                (&[0, 3], cnot()),
            ],
        ),
    ];
    let reference = ExactReference::new();
    for (row, capacity, gates) in &rows {
        for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
            let mut state = engine.prepare(capacity).expect("prepare");
            let mut exact_state = reference.prepare(&exact(4)).expect("prepare");
            for (on, gate) in gates {
                engine.apply(&mut state, on, gate).expect("apply");
                reference.apply(&mut exact_state, on, gate).expect("apply");
            }
            let weight = engine.retained_weight(&state).expect("weight");
            assert!(
                state.split_statistics().largest_transient_bond() >= 2,
                "{row}, {name}: precondition, the MPO grew the span's bonds"
            );
            assert!((weight - 1.0).abs() < 1e-12, "{row}, {name}: {weight}");
            assert!(
                engine.shape(&state).expect("shape").fits_within(capacity),
                "{row}, {name}"
            );
            assert_reads_agree(&engine, &state, &exact_state, 1e-12);
            for sites in [&[0, 1][..], &[3]] {
                let value = engine
                    .expectation(&state, &x_string(sites))
                    .expect("expectation");
                let expected = reference
                    .expectation(&exact_state, &x_string(sites))
                    .expect("expectation");
                assert!(
                    (value - expected).norm() < 1e-12,
                    "{row}, {name}: X on {sites:?}, {value} vs {expected}"
                );
            }
        }
    }
}

/// X on each of `sites`.
fn x_string(sites: &[usize]) -> PauliSum {
    let factors = sites.iter().map(|&s| (s, Pauli::X)).collect();
    PauliSum::new(
        real(0.0),
        vec![PauliTerm::new(real(1.0), factors).expect("a term")],
    )
    .expect("an observable")
}

/// A Bell pair on sites (0, 1) and a pair on (2, 3) of Schmidt weights 0.98
/// and 0.02, at a cutoff of 0.015 that no cut of the state reaches in the
/// caller's order; then a Z⊗Z on sites 0 and 3, which changes no Schmidt
/// value at any of the caller's cuts. The MPO's splits are all at the
/// caller's cuts, so none drops the 0.02 and the weight stays 1, where a
/// route by SWAPs, whose outbound split sees half of each pair, drops about
/// 0.01 at each of two of its splits, and ends near 0.98.
#[test]
fn an_mpo_truncates_only_at_the_callers_cuts() {
    // COMPATIBILITY NOTE: the earlier rule for distant sites, the route by
    // SWAPs `swap-back-2x` (`docs/distant_sites.md`), kept a retained weight of
    // 0.979974491787900 here, which its test pinned. The 0.02 it lost was the
    // route's artefact, not the state's, hence we keep the MPO's whole weight.
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
        engine.apply(&mut state, &[0, 3], &zz()).expect("apply");
        let weight = engine.retained_weight(&state).expect("weight");
        assert!((weight - 1.0).abs() < 1e-12, "{name}: {weight}");
    }
}

/// Six sites, sites 2 and 3 entangled (a bond of 2 at rest just right of
/// the span) and the others in a product state, then a gate on sites 0 and
/// 2; the span's largest transient bond.
fn transient_bond_of_a_gate_on_sites_0_and_2(engine: &Engine, gate: &[Complex64]) -> usize {
    let mut state = engine.prepare(&chain(6, 8)).expect("prepare");
    for site in 0..6 {
        engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
    }
    engine.apply(&mut state, &[2, 3], &cnot()).expect("apply");
    let bonds = engine.shape(&state).expect("shape");
    assert_eq!(bonds.bond_dim(2), Some(2), "precondition: a bond of 2");
    engine.apply(&mut state, &[0, 2], gate).expect("apply");
    engine.retained_weight(&state).expect("weight");
    let statistics = state.split_statistics();
    assert_eq!(
        statistics.mpo_applications(),
        1,
        "precondition: one MPO application"
    );
    statistics.largest_transient_bond()
}

/// A gate on sites 0 and 2 of six, sites 2 and 3 entangled (a bond of 2 at
/// rest just right of the span): the CNOT, of operator rank 2, grows the
/// span's bonds to 2, the entangler, a partial iSWAP with a phase, of rank
/// 4, to 4, and Z⊗Z, of rank 1, leaves them at 1; only the cuts inside the
/// span count, not the bond of 2 beside it. `Svd::of` already leaves out
/// these operators' rounding values.
#[test]
fn an_mpo_grows_the_span_by_the_operators_rank() {
    for (gate, rank) in [(cnot(), 2), (entangler(0.9), 4), (zz(), 1)] {
        for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
            let bond = transient_bond_of_a_gate_on_sites_0_and_2(&engine, &gate);
            assert_eq!(bond, rank, "{name}");
        }
    }
}

/// The rotations `exp(−iε Z⊗Z)` on sites 0 and 2, set as in
/// `an_mpo_grows_the_span_by_the_operators_rank`: their operator values
/// are `2 cos ε` and `2 sin ε`, so the second is `tan ε` relative to the
/// first. At `ε` of `1e-9` and `1.1e-12` it is kept, a bond of 2; at
/// `9e-13` and `1e-13` it is dropped, a bond of 1. The largest value is
/// 2, so a cutoff not scaled by it would keep the second value at `9e-13`,
/// `1.8e-12`. The rows next to the cutoff are 10% from it, about a
/// thousand times what rounding moves `tan ε` there (a few `1e-16`
/// absolute).
#[test]
fn an_mpos_factorisation_drops_operator_values_at_or_below_1e_12_of_the_largest() {
    for (epsilon, rank) in [(1e-9, 2), (1.1e-12, 2), (9e-13, 1), (1e-13, 1)] {
        for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
            let bond = transient_bond_of_a_gate_on_sites_0_and_2(&engine, &zz_rotation(epsilon));
            assert_eq!(bond, rank, "{name}, ε {epsilon:e}");
        }
    }
}

/// `exp(−iε Z⊗Z)`, whose operator values are `cos ε` and `sin ε`, times 2.
fn zz_rotation(epsilon: f64) -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    for (at, sign) in [-1.0, 1.0, 1.0, -1.0].into_iter().enumerate() {
        m[5 * at] = Complex64::from_polar(1.0, sign * epsilon);
    }
    m
}

/// A window's MPO on a span of `d + 1` sites, flushed by the retained
/// weight toward the centre a read left: a CNOT on sites 0 and 4 of 6 with
/// the centre on site 2, inside the span, fused or not; a CNOT on sites 1
/// and 5 of 7 with the centre on site 0 or 6, outside it; and a window of
/// sites 0, 2 and 6 of 7 with the centre on site 3. The centre makes the
/// moves to reach the span's nearer end, if any, then `d − 1`; the span
/// takes `d` splits, and one read and one upload of each of its sites, so
/// a backend that keeps its tensors on a device moves each across once.
#[test]
fn an_mpo_costs_d_minus_1_moves_d_splits_and_one_transfer_per_site() {
    for (fuses, sites, gates, read, expected) in [
        (true, 6, &[&[0, 4][..]][..], 2, (3, 4, 5, 5)),
        (false, 6, &[&[0, 4]], 2, (3, 4, 5, 5)),
        (true, 7, &[&[1, 5]], 0, (1 + 3, 4, 5, 5)),
        (true, 7, &[&[1, 5]], 6, (1 + 3, 4, 5, 5)),
        (true, 7, &[&[0, 2], &[6, 2]], 3, (5, 6, 7, 7)),
    ] {
        let (algebra, calls) = SpyAlgebra::new();
        let engine = CanonicalMpsEngine::new(algebra).with_fusion(fuses);
        let mut state = engine.prepare(&chain(sites, 8)).expect("prepare");
        for site in 0..sites {
            engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
        }
        engine.probability_of_one(&state, read).expect("P(One)");
        let before = (
            calls.qr_moves.get(),
            state.split_statistics().splits(),
            calls.reads.get(),
            calls.uploads.borrow().len(),
        );
        for gate in gates {
            engine.apply(&mut state, gate, &cnot()).expect("apply");
        }
        engine.retained_weight(&state).expect("weight");
        assert_eq!(
            (
                calls.qr_moves.get() - before.0,
                state.split_statistics().splits() - before.1,
                calls.reads.get() - before.2,
                calls.uploads.borrow().len() - before.3,
            ),
            expected,
            "fuses {fuses}, gates {gates:?}, centre on {read}"
        );
    }
}

/// A CNOT on sites 0 and 4 of 6 left pending, then a read of site 5 or of
/// site 0: the window's splits leave the centre on the span's end nearer
/// the read, site 4 or site 0, so the read of site 5 takes one move more
/// than the span's 3, and the read of site 0 none.
#[test]
fn a_read_finds_the_centre_at_the_spans_end_nearer_its_site() {
    for (read, moves) in [(5, 4), (0, 3)] {
        let (algebra, calls) = SpyAlgebra::new();
        let engine = CanonicalMpsEngine::new(algebra);
        let mut state = engine.prepare(&chain(6, 8)).expect("prepare");
        for site in 0..6 {
            engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
        }
        engine.apply(&mut state, &[0, 4], &cnot()).expect("apply");
        let before = calls.qr_moves.get();
        engine.probability_of_one(&state, read).expect("P(One)");
        assert_eq!(calls.qr_moves.get() - before, moves, "a read of {read}");
    }
}
