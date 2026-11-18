// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Moves of the orthogonality centre: a move keeps the gate pending on
//! the site it leaves, and shrinks a bond above twice its neighbours.

use super::*;

/// A turn of site 0, the centre, which waits pending on the site; then a
/// read of site 2, which moves the centre away from site 0: a read of site
/// 0 gives the turn's P(One), as the move applied the gate pending on the
/// site it left.
#[test]
fn a_move_of_the_centre_keeps_the_gate_pending_on_the_site_it_leaves() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(3, 2)).expect("prepare");
        engine.apply(&mut state, &[0], &ry(0.9)).expect("apply");
        engine.probability_of_one(&state, 2).expect("P(One)");
        let p = engine.probability_of_one(&state, 0).expect("P(One)");
        let expected = 0.45f64.sin().powi(2);
        assert!((p - expected).abs() < 1e-12, "{name}: {p} vs {expected}");
    }
}

/// Two Bell pairs, sites 1–3 and 2–4, measured on 3 and 4, then a CNOT on
/// sites 1 and 2: the chain holds a bond of 4 between sites 2 and 3,
/// though site 2's other bond is 1. A projection of site 5 moves the
/// centre there by a QR per site, which leaves each bond it crosses at
/// most twice the bond on the site's other side: the bond of 4 comes out
/// 2, the others as they were. The same holds on the mirrored chain, the
/// centre moving left to site 0.
#[test]
fn moving_the_centre_shrinks_a_bond_above_twice_its_neighbours() {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let hadamard = [real(h), real(h), real(h), real(-h)];
    let bonds = |engine: &Engine, state: &State| {
        let shape = engine.shape(state).expect("a state of six sites");
        (0..5)
            .map(|cut| shape.bond_dim(cut).expect("a cut of six sites"))
            .collect::<Vec<_>>()
    };
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        for mirrored in [false, true] {
            let at = |site: usize| if mirrored { 5 - site } else { site };
            let in_order = |mut bonds: Vec<usize>| {
                if mirrored {
                    bonds.reverse();
                }
                bonds
            };
            let mut state = engine.prepare(&chain(6, 8)).expect("prepare");
            for (control, target) in [(1, 3), (2, 4)] {
                engine
                    .apply(&mut state, &[at(control)], &hadamard)
                    .expect("a site of the chain");
                engine
                    .apply(&mut state, &[at(control), at(target)], &cnot())
                    .expect("distinct sites of the chain");
            }
            for site in [3, 4] {
                engine
                    .project(&mut state, at(site), false)
                    .expect("zero has probability 1/2");
            }
            engine
                .apply(&mut state, &[at(1), at(2)], &cnot())
                .expect("distinct sites of the chain");
            assert_eq!(
                in_order(bonds(&engine, &state)),
                [1, 1, 4, 2, 1],
                "{name}, mirrored {mirrored}: before"
            );
            engine
                .project(&mut state, at(5), false)
                .expect("the site is zero");
            assert_eq!(
                in_order(bonds(&engine, &state)),
                [1, 1, 2, 2, 1],
                "{name}, mirrored {mirrored}: after"
            );
        }
    }
}
