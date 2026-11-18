// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The weight a truncated state keeps: projections leave it, reads divide
//! out the norm so that many truncations and a gate's drift on the centre's
//! site do not reach them, a drift elsewhere shifts them by at most the
//! unitarity check's tolerance, and a truncation fails before the weight
//! underflows.

use super::*;

/// A Bell pair truncated to a bond of 1, which keeps half the weight, beside
/// a site turned by 0.9, and a lone site turned by 4e-162, whose P(One)
/// rounds to the smallest subnormal `f64`: a measurement of either site
/// keeps the retained weight the truncations set, 0.5 and 1.
#[test]
fn a_projection_keeps_the_retained_weight() {
    for (engine, name) in both_fusions_and_cutoffs() {
        let mut state = engine.prepare(&capacity(3, 1)).expect("prepare");
        engine
            .apply(&mut state, &[0], &ry(std::f64::consts::FRAC_PI_2))
            .expect("apply");
        engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
        engine.apply(&mut state, &[2], &ry(0.9)).expect("apply");
        let before = engine.retained_weight(&state).expect("weight");
        assert!((before - 0.5).abs() < 1e-12, "{name}: truncated, {before}");
        engine.project(&mut state, 2, true).expect("project");
        let after = engine.retained_weight(&state).expect("weight");
        assert!(
            (after - before).abs() < 1e-12,
            "{name}: {after} vs {before}"
        );
        let mut lone = engine.prepare(&capacity(1, 1)).expect("prepare");
        engine.apply(&mut lone, &[0], &ry(4e-162)).expect("apply");
        let p = engine.project(&mut lone, 0, true).expect("project");
        assert!(p > 0.0 && p < f64::MIN_POSITIVE, "{name}: {p:e}");
        let weight = engine.retained_weight(&lone).expect("weight");
        assert!((weight - 1.0).abs() < 1e-12, "{name}: subnormal, {weight}");
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

/// A method that truncates a pair to a bond of 1 many times keeps a
/// weight far below the smallest normal `f64`, yet its reads keep their
/// value, however little weight the truncations kept, as each split
/// normalises the values it keeps. One row per read.
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

/// Twenty rounds of a site turned by 4e-162, so that P(One) rounds to the
/// smallest subnormal `f64`, projected on One and turned back to Zero: each
/// projection is accepted, as it leaves a norm that does not shrink with the
/// probability's precision (a division by `√p` would shrink the norm each
/// round, until the value's probability rounded to zero).
#[test]
fn projections_on_values_of_subnormal_probability_are_accepted() {
    let x = [real(0.0), real(1.0), real(1.0), real(0.0)];
    for (engine, name) in both_fusions_and_cutoffs() {
        let mut state = engine.prepare(&capacity(1, 1)).expect("prepare");
        for round in 0..20 {
            engine.apply(&mut state, &[0], &ry(4e-162)).expect("apply");
            let projected = engine.project(&mut state, 0, true);
            assert!(projected.is_ok(), "{name}, round {round}: {projected:?}");
            engine.apply(&mut state, &[0], &x).expect("apply");
        }
    }
}

/// Three sites in cos(0.55)·|000⟩ + sin(0.55)·|101⟩, the middle one's bonds
/// carrying the ends' correlation, then the middle site turned so that
/// P(One) is 1e-200: a projection on One keeps two nonzero elements, and
/// the site the engine uploads in their place has a squared norm of 1
/// within 1e-12, as `gate_split`'s contract promises every algebra of the
/// states it is handed.
#[test]
fn a_projection_hands_the_algebra_a_site_of_norm_one() {
    let theta = 2.0 * 1e-100_f64.asin();
    for fuse in [true, false] {
        let (algebra, calls) = SpyAlgebra::new();
        let engine = CanonicalMpsEngine::new(algebra).with_fusion(fuse);
        let mut state = engine.prepare(&capacity(3, 2)).expect("prepare");
        engine.apply(&mut state, &[0], &ry(1.1)).expect("apply");
        for pair in [[0, 1], [1, 2], [0, 1]] {
            engine.apply(&mut state, &pair, &cnot()).expect("apply");
        }
        engine.apply(&mut state, &[1], &ry(theta)).expect("apply");
        let p = engine
            .project(&mut state, 1, true)
            .expect("a possible value");
        assert!((p / 1e-200 - 1.0).abs() < 1e-9, "fused {fuse}: {p:e}");
        let uploads = calls.uploads.borrow();
        let site = uploads.last().expect("the projected site's upload");
        let kept = site.iter().filter(|x| x.norm() > 0.0).count();
        assert!(kept > 1, "fused {fuse}: {kept} elements kept");
        let norm: f64 = site.iter().map(Complex64::norm_sqr).sum();
        assert!((norm - 1.0).abs() < 1e-12, "fused {fuse}: {norm}");
    }
}

/// Two sites, each turned so that P(One) is 1e-200, the first projected on
/// One: the projection renormalises what it kept, so the state's norm is 1
/// to rounding, and the second site's P(One) still reads 1e-200 (left
/// unnormalised, the state's squared norm would be 1e-200, and the second
/// value's weight, 1e-400, would underflow to zero).
#[test]
fn a_projection_renormalises_the_state() {
    let theta = 2.0 * 1e-100_f64.asin();
    for (engine, name) in both_fusions_and_cutoffs() {
        let mut state = engine.prepare(&capacity(2, 1)).expect("prepare");
        engine.apply(&mut state, &[0], &ry(theta)).expect("apply");
        engine.apply(&mut state, &[1], &ry(theta)).expect("apply");
        engine
            .project(&mut state, 0, true)
            .expect("a possible value");
        let p = engine.probability_of_one(&state, 1).expect("P(One)");
        assert!((p / 1e-200 - 1.0).abs() < 1e-12, "{name}: {p:e}");
    }
}
