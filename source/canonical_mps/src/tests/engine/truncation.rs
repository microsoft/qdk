// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the engine's splits discard, and the weight a truncated state
//! keeps: the tail below the engine's cutoff, a fraction of the split's
//! own weight, which leaves the retained weight, and what exceeds the
//! capacity's bond; a fork keeps its parent's retained weight, reads
//! divide out the norm so that many truncations and a gate's drift on the
//! centre's site do not reach them, a drift elsewhere shifts them by at
//! most the unitarity check's tolerance, and a truncation fails before the
//! weight underflows. The CPU algebra's rule of truncation itself is
//! `Truncation`'s (`src/site_algebra/cpu_algebra/truncation.rs`), tested
//! through its splits in `src/tests/site_algebra/cpu_algebra/truncation.rs`.

use super::*;

/// A caller that takes the engine by `new`: Schmidt weights of 0.9
/// and 1.1 times `DEFAULT_CUTOFF` between two sites, on either side of the
/// default cutoff; the split drops the first and keeps the second.
#[test]
fn an_engine_truncates_at_the_default_cutoff() {
    let engine = Engine::new(CpuAlgebra);
    for (tail, bond) in [(0.9 * DEFAULT_CUTOFF, 1), (1.1 * DEFAULT_CUTOFF, 2)] {
        let theta = 2.0 * tail.sqrt().asin();
        let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
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
    let capacity = capacity(6, 2);
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

/// Schmidt weights between two sites, on engines built `with_cutoff`: the
/// engine's cutoff reaches the gate's split, and the weight it drops leaves
/// the retained weight. With weights 0.8 and 0.2, a cutoff above 0.2 keeps
/// one value and a weight of 0.8, and one below keeps both and a full
/// weight. A Bell pair's weights are 0.5 and 0.5, and the largest cutoff,
/// the largest `f64` below 1, keeps one value and a weight of 0.5.
#[test]
fn the_engines_cutoff_drops_a_splits_tail_from_the_retained_weight() {
    let rows = [
        (0.2f64, 0.25, 1, 0.8),
        (0.2, 0.15, 2, 1.0),
        (0.5, f64::next_down(1.0), 1, 0.5),
    ];
    for (tail, cutoff, bond, weight) in rows {
        let theta = 2.0 * tail.sqrt().asin();
        for (engine, name) in both_fusions(cutoff) {
            let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
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
/// or NaN is a caller's error, and the engine panics on it, while both
/// zeros and the largest value below 1 are accepted.
#[test]
fn a_cutoff_must_lie_in_zero_to_one() {
    for cutoff in [0.0, -0.0, f64::next_down(1.0)] {
        let _ = Engine::new(CpuAlgebra).with_cutoff(cutoff);
    }
    for cutoff in [-1e-300, 1.0, 2.0, f64::INFINITY, f64::NAN] {
        let payload = std::panic::catch_unwind(|| Engine::new(CpuAlgebra).with_cutoff(cutoff))
            .expect_err("a cutoff outside [0, 1)");
        let message = payload
            .downcast_ref::<String>()
            .expect("a formatted panic message");
        assert!(message.contains("in [0, 1)"), "{cutoff}: {message}");
    }
}

/// Grouped shots fork a state whose pair was truncated to a bond of 1,
/// keeping half the weight: the fork's retained weight is its parent's,
/// 0.5, as a fork is the same state.
#[test]
fn a_fork_keeps_its_parents_retained_weight() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&capacity(2, 1)).expect("prepare");
        engine
            .apply(&mut state, &[0], &ry(std::f64::consts::FRAC_PI_2))
            .expect("apply");
        engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
        let fork = engine.fork(&state).expect("fork");
        let weight = engine.retained_weight(&fork).expect("weight");
        assert!((weight - 0.5).abs() < 1e-12, "{name}: {weight}");
    }
}

/// A simulation that truncates a pair to a bond of 1 many times keeps a
/// weight as small as 2⁻¹⁰²⁰, just above the smallest normal `f64`, yet
/// its reads keep their value, however little weight the truncations kept,
/// as each split normalises the values it keeps. One row per read.
#[test]
fn reads_after_many_truncations_keep_their_value() {
    type Row = fn(&Engine, &str);
    let rows: [(&str, Row); 2] = [
        // 600 rounds (a retained weight of 2⁻⁶⁰⁰, about 2.4e-181), with each
        // split leaving the centre on the left site, then on the right,
        // then a turn of a site to a P(One) of 1e-200: the read gives
        // 1e-200, and a projection on it is accepted.
        ("a rare value's probability", |engine, name| {
            for centre in [0, 1] {
                let mut state = engine.prepare(&capacity(2, 1)).expect("prepare");
                for _ in 0..600 {
                    halve_the_weight(engine, &mut state, centre).expect("a round");
                }
                // A gate, not a projection, turns site 0 to Zero: a
                // projection would normalize the state and hide the
                // truncations' losses.
                if engine.probability_of_one(&state, 0).expect("P(One)") > 0.5 {
                    let x = [real(0.0), real(1.0), real(1.0), real(0.0)];
                    engine.apply(&mut state, &[0], &x).expect("apply");
                }
                engine.apply(&mut state, &[0], &ry(2e-100)).expect("apply");
                let p = engine.probability_of_one(&state, 0).expect("P(One)");
                assert!((p / 1e-200 - 1.0).abs() < 1e-9, "{name}, {centre}: {p:e}");
                let projected = engine.project(&mut state, 0, true);
                assert!(projected.is_ok(), "{name}, {centre}: {projected:?}");
            }
        }),
        // 1,020 rounds (a retained weight of 2⁻¹⁰²⁰, about 8.9e-308), then
        // ⟨1000·Z₀⟩: the read gives ±1000, as the state's norm stays about
        // 1 however small the weight.
        ("an expectation", |engine, name| {
            let z = PauliSum::new(
                real(0.0),
                vec![PauliTerm::new(real(1000.0), vec![(0, Pauli::Z)]).expect("a term")],
            )
            .expect("an observable");
            let mut state = engine.prepare(&capacity(2, 1)).expect("prepare");
            for _ in 0..1020 {
                halve_the_weight(engine, &mut state, 0).expect("a round");
            }
            let value = engine.expectation(&state, &z).expect("expectation");
            assert!((value.re.abs() - 1000.0).abs() < 1e-6, "{name}: {value}");
        }),
    ];
    for (row, check) in rows {
        for (engine, name) in both_fusions_and_cutoffs() {
            check(&engine, &format!("{row}, {name}"));
        }
    }
}

/// A site turned by 0.9, then a one-site gate (1 + 1e-10)·I on it, the
/// centre, which the unitarity check accepts but which scales the state's
/// norm: P(One) reads sin²(0.45) and ⟨Z⟩ reads cos(0.9), each within 1e-12,
/// as both reads divide by the weight at the centre.
#[test]
fn reads_divide_out_a_gates_drift_on_the_centres_site() {
    let s = 1.0 + 1e-10;
    let grow = [real(s), real(0.0), real(0.0), real(s)];
    for (engine, name) in both_fusions_and_cutoffs() {
        let mut state = engine.prepare(&capacity(2, 1)).expect("prepare");
        engine.apply(&mut state, &[0], &ry(0.9)).expect("apply");
        engine
            .apply(&mut state, &[0], &grow)
            .expect("a gate within the unitarity tolerance");
        let p = engine.probability_of_one(&state, 0).expect("P(One)");
        let expected = 0.45f64.sin().powi(2);
        assert!((p - expected).abs() < 1e-12, "{name}: {p} vs {expected}");
        let value = engine.expectation(&state, &z_string(&[0])).expect("⟨Z⟩");
        let expected = real(0.9f64.cos());
        assert!((value - expected).norm() < 1e-12, "{name}: {value}");
    }
}

/// A 6-site circuit with the centre on site 0, then on each other site a
/// gate diag(1, 1 + d), with d as large as the unitarity check's tolerance
/// τ accepts, left pending off the centre: P(One) of site 0 and an
/// expectation stay within 2τ per drifting gate (times the observable's
/// coefficients) of the exact reference, which divides by the whole norm,
/// as each such gate leaves the environment the reads take for the
/// identity within τ of it.
#[test]
fn a_gates_drift_off_the_centre_shifts_reads_by_at_most_the_tolerances_order() {
    let tolerance = tensornet::checks::UNITARITY_TOLERANCE;
    // M·M† − I = diag(0, 2d + d²), whose entries sum to τ at most.
    let d = 0.99 * ((1.0 + tolerance).sqrt() - 1.0);
    let drift = [real(1.0), real(0.0), real(0.0), real(1.0 + d)];
    let reference = ExactReference::new();
    let observable = mixed_string();
    let coefficients = Complex64::new(0.5, -1.0).norm();
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let (mut state, mut exact_state) = circuit(&engine, 6);
        engine.probability_of_one(&state, 0).expect("P(One)");
        for site in 1..6 {
            engine.apply(&mut state, &[site], &drift).expect("a drift");
            reference
                .apply(&mut exact_state, &[site], &drift)
                .expect("a drift");
        }
        let bound = 2.0 * 5.0 * tolerance;
        let p = engine.probability_of_one(&state, 0).expect("P(One)");
        let q = reference
            .probability_of_one(&exact_state, 0)
            .expect("P(One)");
        assert!((p - q).abs() < bound, "{name}: {p} vs {q}");
        let value = engine.expectation(&state, &observable).expect("⟨O⟩");
        let exact_value = reference
            .expectation(&exact_state, &observable)
            .expect("⟨O⟩");
        assert!(
            (value - exact_value).norm() < bound * coefficients,
            "{name}: {value} vs {exact_value}"
        );
    }
}

/// Rounds of a pair truncated to a bond of 1, each halving the retained
/// weight: the split that would take it below the smallest normal `f64`
/// fails with `WeightUnderflow`, and not before (the last weight read is
/// below 1.5 times the smallest normal); the retained weight and P(One)
/// read after each earlier round are in range.
#[test]
fn a_truncation_fails_once_the_retained_weight_would_underflow() {
    for (engine, name) in both_fusions_and_cutoffs() {
        let mut state = engine.prepare(&capacity(2, 1)).expect("prepare");
        let mut last = 1.0;
        let mut failure = None;
        for _ in 0..1100 {
            match halve_the_weight(&engine, &mut state, 0) {
                Ok(weight) => {
                    assert!(weight >= f64::MIN_POSITIVE, "{name}: {weight:e}");
                    let p = engine.probability_of_one(&state, 0).expect("P(One)");
                    assert!((0.0..=1.0).contains(&p), "{name}: P(One) {p}");
                    last = weight;
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        assert_eq!(failure, Some(CanonicalMpsError::WeightUnderflow), "{name}");
        assert!(last < 1.5 * f64::MIN_POSITIVE, "{name}: failed at {last:e}");
    }
}
