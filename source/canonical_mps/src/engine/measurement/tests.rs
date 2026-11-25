// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A measurement's operations on one site.
//!
//! - Reads keep the state, and divide out its norm: a gate's drift on the
//!   centre's site does not reach them, and a drift elsewhere shifts them
//!   by at most the unitarity check's tolerance
//!   (`tensornet::checks::UNITARITY_TOLERANCE`).
//! - A projection makes its value certain and returns its probability
//!   without recompressing. It keeps the retained weight, accepts a value of
//!   subnormal probability, and hands the algebra a site of norm one.
//! - A value's probability is its own weight over the sum of both values'
//!   weights, so a value of weight zero reads exactly zero, the other
//!   exactly one, and a value of tiny weight its own ratio.
//! - A conditional probability is the probability after its conditions,
//!   and forks once.
//! - Samples are drawn from the conditional probabilities by `tensornet`'s
//!   shared sampler, whose own rules are tested there. A sample draws from
//!   the `SplitMix64` stream of its seed.

use super::value_probabilities;
use crate::engine::conformance::{capacity, cnot, mixed_string, real, ry};
use crate::engine::host_site::HostSite;
use crate::engine::tests::spy_algebra::SpyAlgebra;
use crate::engine::tests::{
    both_fusions, both_fusions_and_cutoffs, circuit, spied_entangled_chain, z_string,
};
use crate::{CanonicalMpsEngine, CanonicalMpsState, DEFAULT_CUTOFF};
use num_complex::Complex64;
use std::borrow::Cow;
use tensornet::MpsEngine;
use tensornet::reference::ExactReference;
use tensornet::sampling::{TooManyValues, sample_by_conditionals};

/// Reads move the orthogonality centre, never the state they read; nor does
/// a sample, which draws from forks. On a six-site circuit, reads of sites
/// at either end and in the middle, each moving the centre, leave an
/// expectation as it was.
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
            engine.sample(&state, &[site, 1], 8, 7).expect("a sample");
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
/// τ accepts, left pending off the centre. P(One) of site 0 and an
/// expectation stay within 2τ per drifting gate (times the observable's
/// coefficients) of the exact reference, which divides by the whole norm.
/// Each such gate leaves the environment the reads take for the identity
/// within τ of it.
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

/// A turn of site 0 by 0.9, which waits pending on the site, the centre,
/// then a measurement of it that draws One: the projection applies the
/// turn first, so it returns the turn's P(One), sin²(0.45), and only once,
/// so the site reads One with certainty after it.
#[test]
fn a_projection_applies_the_sites_pending_gate_once() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&capacity(3, 2)).expect("prepare");
        engine.apply(&mut state, &[0], &ry(0.9)).expect("apply");
        let p = engine.project(&mut state, 0, true).expect("project");
        let expected = 0.45f64.sin().powi(2);
        assert!((p - expected).abs() < 1e-12, "{name}: {p} vs {expected}");
        let after = engine.probability_of_one(&state, 0).expect("P(One)");
        assert!((after - 1.0).abs() < 1e-12, "{name}: {after}");
    }
}

/// A simulation method that records each measurement's probability: the
/// projection on either value returns that value's probability before it.
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

/// A sampler's conditional probability of site 5 given values of sites 0
/// and 3 is the probability of site 5 on a fork projected on those values
/// in order. Site 5 may be among the conditions too: given One it answers
/// 1, and given Zero 0.
#[test]
fn a_conditional_probability_is_the_probability_after_its_conditions() {
    // The conditions, and the value they make certain, if any.
    type Row = (&'static [(usize, bool)], Option<f64>);
    let rows: [Row; 3] = [
        (&[(0, true), (3, false)], None),
        (&[(5, true)], Some(1.0)),
        (&[(0, true), (5, false)], Some(0.0)),
    ];
    for (given, certain) in rows {
        for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
            let (state, _) = circuit(&engine, 6);
            let p = engine
                .conditional_probability_of_one(&state, 5, given)
                .expect("possible conditions");
            let mut fork = engine.fork(&state).expect("fork");
            for &(site, one) in given {
                engine
                    .project(&mut fork, site, one)
                    .expect("a possible value");
            }
            let expected = engine.probability_of_one(&fork, 5).expect("P(One)");
            assert!(
                (p - expected).abs() < 1e-12,
                "{name}, given {given:?}: {p} vs {expected}"
            );
            if let Some(certain) = certain {
                assert!((p - certain).abs() < 1e-12, "{name}, given {given:?}: {p}");
            }
        }
    }
}

/// A measurement of one qubit of a GHZ state leaves a product state, whose
/// Schmidt ranks are all 1; the engine does not recompress, so `shape`
/// still reports the bonds of 2 it held before.
#[test]
fn a_projection_does_not_recompress_the_bonds() {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let hadamard = [real(h), real(h), real(h), real(-h)];
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut ghz = engine.prepare(&capacity(4, 4)).expect("prepare");
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
        assert_eq!(after.bond_dimension(0), Some(2), "{name}");
    }
}

/// Grouped shots read a site's probability given the values drawn on
/// others: a conditional probability forks at most once, as many
/// duplicates of the chain's sites as one fork makes, whatever its number
/// of conditions; without conditions it reads the state itself and
/// duplicates nothing.
#[test]
fn a_conditional_probability_forks_at_most_once() {
    let (engine, state, calls) = spied_entangled_chain(6);
    engine.fork(&state).expect("fork");
    let one_fork = calls.duplicates.get();
    assert!(one_fork > 0, "precondition: a fork duplicates the sites");
    for (given, forks) in [
        (&[][..], 0),
        (&[(1, true)], 1),
        (&[(1, true), (3, false)], 1),
    ] {
        let before = calls.duplicates.get();
        engine
            .conditional_probability_of_one(&state, 5, given)
            .expect("possible conditions");
        assert_eq!(
            calls.duplicates.get() - before,
            forks * one_fork,
            "given {given:?}"
        );
    }
}

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

/// A projection hands the algebra a site of squared norm 1 within 1e-12.
/// The contract promises every algebra this of the states the engine hands
/// it; and with the centre on that site, the state's norm is 1 too, so the
/// next reads keep their precision. Each row: a scene, the site projected
/// on One, the range of its probability, and the fewest elements the
/// projection keeps.
///
/// - Three sites in cos(0.55)·|000⟩ + sin(0.55)·|101⟩, the middle one's
///   bonds carrying the ends' correlation, then the middle site turned so
///   that P(One) is 1e-200. The projection keeps two nonzero elements, so
///   their norm sets the scale, not the largest alone.
/// - A lone site turned by 4e-162, so that P(One) rounds to the smallest
///   subnormal `f64`. Its one kept element, about 2e-162, has a square
///   that rounds to that subnormal: a norm not scaled by the largest
///   modulus would divide it by about 2.22e-162, a squared norm of 0.81.
#[test]
fn a_projection_hands_the_algebra_a_site_of_norm_one() {
    let rows = [
        ProjectedScene {
            name: "three sites, P(One) 1e-200",
            prepare: |engine| {
                let theta = 2.0 * 1e-100_f64.asin();
                let mut state = engine.prepare(&capacity(3, 2)).expect("prepare");
                engine.apply(&mut state, &[0], &ry(1.1)).expect("apply");
                for pair in [[0, 1], [1, 2], [0, 1]] {
                    engine.apply(&mut state, &pair, &cnot()).expect("apply");
                }
                engine.apply(&mut state, &[1], &ry(theta)).expect("apply");
                state
            },
            site: 1,
            probability: 1e-200 * (1.0 - 1e-9)..=1e-200 * (1.0 + 1e-9),
            fewest_kept: 2,
        },
        ProjectedScene {
            name: "a lone site, P(One) subnormal",
            prepare: |engine| {
                let mut state = engine.prepare(&capacity(1, 1)).expect("prepare");
                engine.apply(&mut state, &[0], &ry(4e-162)).expect("apply");
                state
            },
            site: 0,
            probability: f64::from_bits(1)..=f64::MIN_POSITIVE,
            fewest_kept: 1,
        },
    ];
    for row in &rows {
        for fuse in [true, false] {
            let (algebra, calls) = SpyAlgebra::new();
            let engine = CanonicalMpsEngine::new(algebra).with_fusion(fuse);
            let mut state = (row.prepare)(&engine);
            let p = engine
                .project(&mut state, row.site, true)
                .expect("a possible value");
            let name = format!("{}, fused {fuse}", row.name);
            assert!(row.probability.contains(&p), "{name}: {p:e}");
            let uploads = calls.uploads.borrow();
            let site = uploads.last().expect("the projected site's upload");
            let kept = site.iter().filter(|x| x.norm() > 0.0).count();
            assert!(kept >= row.fewest_kept, "{name}: {kept} elements kept");
            let norm: f64 = site.iter().map(Complex64::norm_sqr).sum();
            assert!((norm - 1.0).abs() < 1e-12, "{name}: {norm}");
        }
    }
}

/// A scene of [`a_projection_hands_the_algebra_a_site_of_norm_one`].
struct ProjectedScene {
    /// The scene's name, in failure messages.
    name: &'static str,
    /// Prepares the state on the spied engine.
    prepare: fn(&CanonicalMpsEngine<SpyAlgebra>) -> CanonicalMpsState<SpyAlgebra>,
    /// The site projected on One.
    site: usize,
    /// The range One's probability falls in, a precondition.
    probability: std::ops::RangeInclusive<f64>,
    /// The fewest nonzero elements the projection keeps, a precondition.
    fewest_kept: usize,
}

/// A site whose weights the test writes out, `left` and `right` 2, so that
/// `T[a, s, b]` is element `a + 2s + 4b`: each value's probability is its
/// own weight over the sum of both values' weights, exactly. Each row: the
/// elements, and P(Zero) and P(One).
///
/// - One holds the weight: `T[0, 1, 0]` is 1, `T[0, 1, 1]` and
///   `T[1, 1, 1]` are `x`, `3·2⁻²⁸`. P(Zero) reads exactly 0 and P(One)
///   exactly 1. `x²` is `9·2⁻⁵⁶`, `0.5625 ε` (`ε` being `f64::EPSILON`,
///   `2⁻⁵²`). Summed by runs of its elements, One's weight is
///   `1 + 1.125 ε`, which rounds to `1 + ε`, over a sum of `(1 + ε) + 0`.
///   Summed over the whole site in one pass, each `x²` rounds the weight up
///   a step, to `1 + 2 ε`: over it, One would read `(1 + ε) / (1 + 2 ε)`,
///   below 1, and a sample whose uniform is `1 − 2⁻⁵³` would draw Zero, of
///   weight zero.
/// - Zero holds the weight: the same elements at `s = 0`. P(Zero) reads
///   exactly 1 and P(One) exactly 0.
/// - Zero has weight `y²`, `y` being `2⁻³⁰` at `T[0, 0, 0]`, and One weight
///   1 at `T[0, 1, 0]`: P(Zero) reads `2⁻⁶⁰` and P(One) 1, the sum
///   `1 + 2⁻⁶⁰` rounding to 1. One minus P(One) would read 0, and a
///   condition on Zero would be refused.
/// - The same, values swapped: P(Zero) reads 1 and P(One) `2⁻⁶⁰`.
#[test]
fn each_value_reads_its_own_weight_over_the_sum_of_both() {
    let x = 3.0 * 2f64.powi(-28);
    let y = 2f64.powi(-30);
    let rows = [
        ([0.0, 0.0, 1.0, 0.0, 0.0, 0.0, x, x], [0.0, 1.0]),
        ([1.0, 0.0, 0.0, 0.0, x, x, 0.0, 0.0], [1.0, 0.0]),
        ([y, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0], [y * y, 1.0]),
        ([1.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, 0.0], [1.0, y * y]),
    ];
    for (elements, expected) in rows {
        let site = HostSite {
            left: 2,
            right: 2,
            elements: Cow::Owned(elements.map(real).to_vec()),
        };
        let probabilities = value_probabilities(&site);
        assert_eq!(
            probabilities.map(f64::to_bits),
            expected.map(f64::to_bits),
            "{elements:?}: {probabilities:?}"
        );
    }
}

/// A sample of sites 3, 0 and 1 of 5, from the centre on site 0, leaves
/// the centre on site 3, the first site it samples, its first read: three
/// moves, and a read of site 3 after it takes none. A sample that reads
/// nothing (refused, or of no shots) leaves the centre where it was.
#[test]
fn a_sample_leaves_the_centre_on_the_first_site_it_samples() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let state = engine.prepare(&capacity(5, 4)).expect("prepare");
        engine.sample(&state, &[5], 8, 0).expect_err("refused");
        engine.sample(&state, &[3], 0, 0).expect("no shots");
        assert_eq!(state.statistics().centre_moves(), 0, "{name}");
        engine.sample(&state, &[3, 0, 1], 8, 0).expect("a sample");
        assert_eq!(state.statistics().centre_moves(), 3, "{name}: sample");
        engine.probability_of_one(&state, 3).expect("P(One)");
        assert_eq!(state.statistics().centre_moves(), 3, "{name}: on 3");
    }
}

/// A gate on sites 1 and 3 left pending, then samples that draw no value
/// (of no sites, of no shots, or of more values than memory holds): none
/// applies the gate, so none moves the centre, splits or applies an
/// operator. A sample of one shot then applies it.
#[test]
fn a_sample_that_draws_no_value_applies_no_pending_gate() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&capacity(5, 4)).expect("prepare");
        engine.apply(&mut state, &[1, 3], &cnot()).expect("apply");
        let before = state.statistics();
        engine.sample(&state, &[], 8, 0).expect("no sites");
        engine.sample(&state, &[3], 0, 0).expect("no shots");
        engine
            .sample(&state, &[0, 1], usize::MAX, 0)
            .expect_err("more values than memory holds");
        assert_eq!(state.statistics(), before, "{name}: no value drawn");
        engine.sample(&state, &[3], 1, 0).expect("one shot");
        assert_eq!(
            state.statistics().mpo_applications(),
            1,
            "{name}: a value drawn"
        );
    }
}

/// 64 shots of qubit 0 of two after a Hadamard on it, P(One) = ½, with a
/// few seeds:
/// each shot draws One exactly when the next uniform of the `SplitMix64`
/// stream seeded with the seed is below ½, as `tensornet`'s shared sampler
/// draws, so a seed gives its own sequence of values, the same on every
/// engine.
#[test]
fn a_sample_draws_from_the_splitmix64_stream_of_its_seed() {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let hadamard = [real(h), real(h), real(h), real(-h)];
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
        engine.apply(&mut state, &[0], &hadamard).expect("apply");
        let p1 = engine.probability_of_one(&state, 0).expect("P(One)");
        assert_eq!(p1.to_bits(), 0.5f64.to_bits(), "{name}: P(One) is ½");
        for seed in [0, 1, 42, u64::MAX] {
            let expected =
                sample_by_conditionals(&[0], 64, seed, |_, _| Ok::<_, TooManyValues>(0.5))
                    .expect("64 values");
            let values = engine.sample(&state, &[0], 64, seed).expect("a sample");
            assert_eq!(values, expected, "{name}, seed {seed}");
        }
    }
}
