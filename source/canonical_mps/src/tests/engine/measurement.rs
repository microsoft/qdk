// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A measurement's operations on one site: reads keep the state, a
//! projection makes its value certain, returns its probability without
//! recompressing, keeps the retained weight, accepts a value of subnormal
//! probability, hands the algebra a site of norm one and renormalises the
//! state; a conditional probability is the probability after its
//! conditions, and forks once; and samples, drawn from the conditional
//! probabilities: no sites give no values, a measured value has
//! probability exactly one, a value of probability zero is never drawn,
//! and a sample draws from the `SplitMix64` stream of its seed.

use super::*;
use tensornet::sampling::{TooManyValues, sample_by_conditionals};

/// Reads move the orthogonality centre, never the state they read; nor does
/// a sample, which draws from forks.
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
        assert_eq!(after.bond_dim(0), Some(2), "{name}");
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
/// within 1e-12, as the contract promises every algebra of the states the
/// engine hands it.
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

/// A sample of no sites gives no values, at once, however many shots: it
/// draws nothing. The call runs on another thread, so that a sample that
/// loops over the shots fails the test rather than hang it.
#[test]
fn a_sample_of_no_sites_gives_no_values_at_once() {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let engine = Engine::new(CpuAlgebra);
        let state = engine.prepare(&capacity(2, 2)).expect("prepare");
        let values = engine.sample(&state, &[], usize::MAX, 0);
        sender.send(values).expect("the test waits");
    });
    let values = receiver
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("a sample of no sites returns at once")
        .expect("no sites");
    assert_eq!(values, Vec::<bool>::new());
}

/// The seed whose first uniform is `1 − 2⁻⁵³`, the largest a draw gives.
const SEED_OF_THE_LARGEST_UNIFORM: u64 = 0x3162_8af6_7b21_31ab;

/// The seed whose first uniform is 0, the smallest a draw gives.
const SEED_OF_A_ZERO_UNIFORM: u64 = 0x61c8_8646_80b5_83eb;

/// The value `tensornet`'s shared sampler draws in one shot of one site
/// of probability `p1` of One, with `seed`: One exactly when the seed's
/// first uniform is below `p1`.
fn first_draw(p1: f64, seed: u64) -> bool {
    let values = sample_by_conditionals(&[0], 1, seed, |_, _| Ok::<_, TooManyValues>(p1))
        .expect("one value");
    values[0]
}

/// A mid-circuit measurement of `site` of `circuit(engine, sites)` that
/// drew `one`, on an entangled state whose bonds on both sides of the site
/// exceed 1: the other value has probability exactly zero, with the centre
/// left on the site.
fn measured_on(engine: &Engine, sites: usize, site: usize, one: bool) -> State {
    let (mut state, _) = circuit(engine, sites);
    let shape = engine.shape(&state).expect("shape");
    assert!(
        shape.bond_dim(site - 1) > Some(1) && shape.bond_dim(site) > Some(1),
        "bonds above 1 around site {site}: {shape:?}"
    );
    engine
        .project(&mut state, site, one)
        .expect("a possible value");
    state
}

/// Mid-circuit measurements of each site of entangled states on 5 to 10
/// sites, on either value: after each, the measured value's probability
/// is exactly one, since a value's probability is its weight over the sum
/// of the two values' weights, and the other's weight is then exactly zero.
/// Over the site's whole weight, summed in another order, it could read
/// `1 − 2⁻⁵³`, and a sample could draw the value of probability zero.
#[test]
fn a_measured_value_has_probability_exactly_one() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        for sites in 5..=10 {
            let (state, _) = circuit(&engine, sites);
            for site in 0..sites {
                for one in [false, true] {
                    let mut fork = engine.fork(&state).expect("fork");
                    engine
                        .project(&mut fork, site, one)
                        .expect("a possible value");
                    let p1 = engine.probability_of_one(&fork, site).expect("P(One)");
                    let expected = if one { 1.0 } else { 0.0 };
                    assert_eq!(
                        p1.to_bits(),
                        f64::to_bits(expected),
                        "{name}, {sites} sites, site {site}, {one}: {p1}"
                    );
                }
            }
        }
    }
}

/// A sample never draws a value of probability zero, even with the seed
/// whose first uniform is the edge of the draw's range. One row per value.
#[test]
fn a_sample_never_draws_a_value_of_probability_zero() {
    type Row = fn(&Engine, &str);
    let rows: [(&str, Row); 2] = [
        // A sample of a site and site 0 after a measurement of the site
        // drew One, with the seed whose first uniform is the largest a draw
        // gives: the sample draws One, not the Zero of probability zero,
        // and the next site's draw, given it, is not refused. The site must
        // be one whose P(One) over its whole weight rounds below that
        // uniform: site 1 of 5, fused, rounds so (no unfused site of the
        // circuits of `a_measured_value_has_probability_exactly_one` does,
        // and the rule is the same for both). A precondition asks to find
        // such a site again when the splits' rounding changes.
        ("One where Zero has probability zero", |engine, name| {
            // COMPATIBILITY NOTE: this behavior was present in the earlier
            // CPU MPS engine's sampling (`qdk_cpu_mps`, not merged), hence
            // we keep its rule. The site differs from its test's, because
            // this crate's splits round differently from that engine's SVD.
            // 1 − 2⁻⁵³ is the largest `f64` below 1, so a uniform in [0, 1) is
            // not below it only when it is 1 − 2⁻⁵³ exactly.
            assert!(
                !first_draw(1.0 - f64::EPSILON / 2.0, SEED_OF_THE_LARGEST_UNIFORM),
                "the seed's first draw is 1 − 2⁻⁵³ exactly"
            );
            if !name.starts_with("fused") {
                return;
            }
            let (sites, site) = (5, 1);
            let state = measured_on(engine, sites, site, true);
            let whole = engine
                .probability_of_one_over_the_whole_weight(&state, site)
                .expect("a read");
            assert!(
                whole < 1.0,
                "{name}: precondition, P(One) of site {site} over its whole \
                 weight rounds below 1 ({whole}); the splits' rounding changed, \
                 so find such a site again as the row's comment says"
            );
            let values = engine
                .sample(&state, &[site, 0], 1, SEED_OF_THE_LARGEST_UNIFORM)
                .expect("a sample");
            assert!(values[0], "{name}");
        }),
        // A sample of site 2 after a measurement of site 2 drew Zero, with
        // the seed whose first uniform is 0: the sample draws Zero, not the
        // One of probability zero.
        ("Zero where One has probability zero", |engine, name| {
            // The smallest positive `f64`, so a uniform is below it only
            // when it is 0 exactly.
            assert!(
                first_draw(f64::from_bits(1), SEED_OF_A_ZERO_UNIFORM),
                "the seed's first draw is 0 exactly"
            );
            let state = measured_on(engine, 6, 2, false);
            let values = engine
                .sample(&state, &[2], 1, SEED_OF_A_ZERO_UNIFORM)
                .expect("a sample");
            assert!(!values[0], "{name}");
        }),
    ];
    for (row, check) in rows {
        for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
            check(&engine, &format!("{name}: {row}"));
        }
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
        for seed in [0, 1, 42, SEED_OF_THE_LARGEST_UNIFORM] {
            let expected =
                sample_by_conditionals(&[0], 64, seed, |_, _| Ok::<_, TooManyValues>(0.5))
                    .expect("64 values");
            let values = engine.sample(&state, &[0], 64, seed).expect("a sample");
            assert_eq!(values, expected, "{name}, seed {seed}");
        }
    }
}
