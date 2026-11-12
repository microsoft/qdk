// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Reads and projections: reads keep the state, a projection makes its
//! value certain and returns its probability without recompressing, and
//! an expectation contracts only between the observable and the centre.

use super::*;

/// Reads move the orthogonality centre, never the state they read.
#[test]
fn reads_keep_the_state() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let (state, _) = circuit(&engine, 6);
        let before = engine
            .expectation(&state, &mixed_string())
            .expect("expectation");
        for site in [5, 0, 3] {
            engine.probability_of_one(&state, site).expect("P(One)");
            engine
                .conditional_probability_of_one(&state, 1, &[(site, true)])
                .expect("a possible condition");
            let after = engine
                .expectation(&state, &mixed_string())
                .expect("expectation");
            assert!(
                (after - before).norm() < 1e-12,
                "{name}: {after} vs {before}"
            );
        }
    }
}

/// A measurement of site 2 that draws One leaves the site certain of it.
#[test]
fn a_projection_makes_its_value_certain() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let (mut state, _) = circuit(&engine, 6);
        engine.project(&mut state, 2, true).expect("project");
        let p = engine.probability_of_one(&state, 2).expect("P(One)");
        assert!((p - 1.0).abs() < 1e-12, "{name}: {p}");
    }
}

/// A method that records each measurement's probability: the projection
/// on either value returns that value's probability before it.
#[test]
fn a_projection_returns_the_probability_of_its_value() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let (state, _) = circuit(&engine, 6);
        let p1 = engine.probability_of_one(&state, 2).expect("P(One)");
        assert!(p1 > 0.1 && p1 < 0.9, "{name}: an uncertain site, {p1}");
        for (one, expected) in [(true, p1), (false, 1.0 - p1)] {
            let mut fork = engine.fork(&state).expect("fork");
            let p = engine.project(&mut fork, 2, one).expect("project");
            assert!((p - expected).abs() < 1e-12, "{name}, {one}: {p}");
        }
    }
}

/// A measurement of one qubit of a GHZ state leaves a product state, whose
/// Schmidt ranks are all 1; the engine does not recompress, so `shape`
/// still reports the bonds of 2 it held before.
#[test]
fn a_projection_does_not_recompress_the_bonds() {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let hadamard = [c(h), c(h), c(h), c(-h)];
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut ghz = engine.prepare(&chain(4, 4)).expect("prepare");
        engine.apply(&mut ghz, &[0], &hadamard).expect("apply");
        for site in 1..4 {
            engine
                .apply(&mut ghz, &[site - 1, site], &cnot())
                .expect("apply");
        }
        let before = engine.shape(&ghz).expect("shape");
        engine.project(&mut ghz, 1, true).expect("project");
        let after = engine.shape(&ghz).expect("shape");
        assert_eq!(after, before, "{name}");
        assert_eq!(after.bond_dim(0), Some(2), "{name}");
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
    let hadamard = [c(h), c(h), c(h), c(-h)];
    let bonds = |engine: &CpuMpsEngine, state: &CanonicalMps| {
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

/// The products an expectation of Z on `observed` takes on a chain of
/// `sites` entangled sites whose centre a read has left on `centre`.
fn expectation_products(sites: usize, centre: usize, observed: usize) -> usize {
    let counts = Rc::new(Counts::default());
    let engine = Engine::new(Counting(Rc::clone(&counts)));
    let mut state = engine.prepare(&chain(sites, 4)).expect("prepare");
    for site in 0..sites {
        engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
    }
    for cut in 0..sites - 1 {
        engine
            .apply(&mut state, &[cut, cut + 1], &cnot())
            .expect("apply");
    }
    engine.probability_of_one(&state, centre).expect("P(One)");
    let before = counts.products.get();
    engine
        .expectation(&state, &z_string(&[observed]))
        .expect("expectation");
    counts.products.get() - before
}

/// A Z read next to the centre, at either end of a chain of 12 sites or of
/// one of 6, takes the same products: an expectation contracts only the
/// sites between the observable's sites and the centre, wherever they lie
/// on the chain and whatever its length; a Z further from the centre takes
/// more.
#[test]
fn an_expectation_contracts_only_between_the_observable_and_the_centre() {
    let near = expectation_products(12, 0, 1);
    assert_eq!(expectation_products(12, 10, 11), near, "at the far end");
    assert_eq!(expectation_products(12, 6, 5), near, "below the centre");
    assert_eq!(expectation_products(6, 4, 5), near, "on a shorter chain");
    assert!(
        expectation_products(12, 0, 5) > near,
        "further from the centre"
    );
}
