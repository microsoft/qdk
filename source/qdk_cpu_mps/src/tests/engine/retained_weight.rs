// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The weight a truncated state keeps: projections leave it, reads divide
//! out the norm so that many truncations and a gate's drift do not reach
//! them, and a truncation fails before the weight underflows.

use super::*;

/// A Bell pair truncated to a bond of 1, which keeps half the weight, beside
/// a site turned by 0.9, and a lone site turned by 4e-162, whose P(One)
/// rounds to the smallest subnormal `f64`: a measurement of either site
/// keeps the retained weight the truncations set, 0.5 and 1.
#[test]
fn a_projection_keeps_the_retained_weight() {
    for (engine, name) in both_fusions_and_cutoffs() {
        let mut state = engine.prepare(&chain(3, 1)).expect("prepare");
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
        let mut lone = engine.prepare(&chain(1, 1)).expect("prepare");
        engine.apply(&mut lone, &[0], &ry(4e-162)).expect("apply");
        let p = engine.project(&mut lone, 0, true).expect("project");
        assert!(p > 0.0 && p < f64::MIN_POSITIVE, "{name}: {p:e}");
        let weight = engine.retained_weight(&lone).expect("weight");
        assert!((weight - 1.0).abs() < 1e-12, "{name}: subnormal, {weight}");
    }
}

/// A method that truncates a pair to a bond of 1 for 600 rounds (a
/// retained weight of 2⁻⁶⁰⁰, about 2.4e-181), with each split leaving the
/// centre on the left site, then on the right, then turns a site to a
/// P(One) of 1e-200: the read gives 1e-200, and a projection on it is
/// accepted, however little weight the truncations kept.
#[test]
fn a_rare_value_reads_its_probability_after_many_truncations() {
    for (engine, name) in both_fusions_and_cutoffs() {
        for centre in [0, 1] {
            let mut state = engine.prepare(&chain(2, 1)).expect("prepare");
            for _ in 0..600 {
                halve_the_weight(&engine, &mut state, centre).expect("a round");
            }
            // A gate, not a projection, turns site 0 to Zero: a projection
            // would normalize the state and hide the truncations' losses.
            if engine.probability_of_one(&state, 0).expect("P(One)") > 0.5 {
                let x = [c(0.0), c(1.0), c(1.0), c(0.0)];
                engine.apply(&mut state, &[0], &x).expect("apply");
            }
            engine.apply(&mut state, &[0], &ry(2e-100)).expect("apply");
            let p = engine.probability_of_one(&state, 0).expect("P(One)");
            assert!((p / 1e-200 - 1.0).abs() < 1e-9, "{name}, {centre}: {p:e}");
            let projected = engine.project(&mut state, 0, true);
            assert!(projected.is_ok(), "{name}, {centre}: {projected:?}");
        }
    }
}

/// 1,020 rounds of a pair truncated to a bond of 1 (a retained weight of
/// 2⁻¹⁰²⁰, about 8.9e-308), then ⟨1000·Z₀⟩: the read gives ±1000, as the
/// kept part of each split holds the squared norm of the whole block, not
/// more, so the state's norm stays about 1 however small the weight.
#[test]
fn an_expectation_after_many_truncations_reads_its_value() {
    let z = PauliSum::new(
        c(0.0),
        vec![PauliTerm::new(c(1000.0), vec![(0, Pauli::Z)]).expect("a term")],
    )
    .expect("an observable");
    for (engine, name) in both_fusions_and_cutoffs() {
        let mut state = engine.prepare(&chain(2, 1)).expect("prepare");
        for _ in 0..1020 {
            halve_the_weight(&engine, &mut state, 0).expect("a round");
        }
        let value = engine.expectation(&state, &z).expect("expectation");
        assert!((value.re.abs() - 1000.0).abs() < 1e-6, "{name}: {value}");
    }
}

/// A one-site gate (1 + 1e-10)·I, which the unitarity check accepts but
/// which scales the state's norm: ⟨Z⟩ on the state it leaves reads 1 within
/// 1e-12, as the expectation divides by the squared norm at the centre.
#[test]
fn an_expectation_divides_out_a_gates_drift() {
    let s = 1.0 + 1e-10;
    let grow = [c(s), c(0.0), c(0.0), c(s)];
    for (engine, name) in both_fusions_and_cutoffs() {
        let mut state = engine.prepare(&chain(2, 1)).expect("prepare");
        engine
            .apply(&mut state, &[0], &grow)
            .expect("a gate within the unitarity tolerance");
        let value = engine.expectation(&state, &z_string(&[0])).expect("⟨Z⟩");
        assert!((value - c(1.0)).norm() < 1e-12, "{name}: {value}");
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
        let mut state = engine.prepare(&chain(2, 1)).expect("prepare");
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
        assert_eq!(failure, Some(CpuMpsError::WeightUnderflow), "{name}");
        assert!(last < 1.5 * f64::MIN_POSITIVE, "{name}: failed at {last:e}");
    }
}

/// Twenty rounds of a site turned by 4e-162, so that P(One) rounds to the
/// smallest subnormal `f64`, projected on One and turned back to Zero: each
/// projection is accepted, as it leaves a state of norm 1 whatever the
/// probability's precision (a division by `√p` would shrink the norm each
/// round, until the value's probability rounded to zero).
#[test]
fn projections_on_values_of_subnormal_probability_are_accepted() {
    let x = [c(0.0), c(1.0), c(1.0), c(0.0)];
    for (engine, name) in both_fusions_and_cutoffs() {
        let mut state = engine.prepare(&chain(1, 1)).expect("prepare");
        for round in 0..20 {
            engine.apply(&mut state, &[0], &ry(4e-162)).expect("apply");
            let projected = engine.project(&mut state, 0, true);
            assert!(projected.is_ok(), "{name}, round {round}: {projected:?}");
            engine.apply(&mut state, &[0], &x).expect("apply");
        }
    }
}

/// An invariant of `Site::project`, which no public read tells apart: a
/// projection on an entangled pair, whose kept value spans two bond values,
/// divides the site by the norm of what it kept, not by its largest entry,
/// so the state's norm is 1.
#[test]
fn a_projection_leaves_a_state_of_norm_1() {
    for (engine, name) in both_fusions_and_cutoffs() {
        let mut pair = engine.prepare(&chain(2, 2)).expect("prepare");
        engine.apply(&mut pair, &[0], &ry(1.0)).expect("apply");
        engine.apply(&mut pair, &[0, 1], &cnot()).expect("apply");
        engine.apply(&mut pair, &[0], &ry(0.7)).expect("apply");
        engine.project(&mut pair, 0, true).expect("project");
        let chain = pair.chain.borrow();
        let norm = chain.sites[chain.centre].weight();
        assert!((norm - 1.0).abs() < 1e-12, "{name}: {norm}");
    }
}
