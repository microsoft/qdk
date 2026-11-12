// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Samples: no sites give no values, a measured value has probability
//! exactly one, a value of probability zero is never drawn, and a sample
//! draws from the `SplitMix64` stream of its seed.

use super::*;
use tensornet::sampling::{TooManyValues, sample_by_conditionals};

/// A sample of no sites gives no values, at once, however many shots: it
/// draws nothing. The call runs on another thread, so that a sample that
/// loops over the shots fails the test rather than hang it.
#[test]
fn a_sample_of_no_sites_gives_no_values_at_once() {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let engine = CpuMpsEngine::default();
        let state = engine.prepare(&chain(2, 2)).expect("prepare");
        let values = engine.sample(&state, &[], usize::MAX, 0);
        sender.send(values).expect("the test waits");
    });
    let values = receiver
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("a sample of no sites returns at once")
        .expect("no sites");
    assert!(values.is_empty());
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
fn measured_on(engine: &CpuMpsEngine, sites: usize, site: usize, one: bool) -> CanonicalMps {
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

/// A sample of a site and site 0 after a measurement of the site drew One,
/// with the seed whose first uniform is the largest a draw gives: the
/// sample draws One, not the Zero of probability zero, and the next site's
/// draw, given it, is not refused. The site, per fusion setting, is one on
/// which P(One) over the site's whole weight, summed in another order than
/// the two values' weights, rounds below that uniform: site 1 of 5 sites
/// fused, site 3 of 10 unfused, the only such sites of 5 to 10 sites
/// besides site 8 of 10 unfused, all of which
/// `a_measured_value_has_probability_exactly_one` measures.
#[test]
fn a_sample_draws_one_where_zero_has_probability_zero() {
    // 1 − 2⁻⁵³ is the largest `f64` below 1, so a uniform in [0, 1) is not
    // below it only when it is 1 − 2⁻⁵³ exactly.
    assert!(
        !first_draw(1.0 - f64::EPSILON / 2.0, SEED_OF_THE_LARGEST_UNIFORM),
        "the seed's first draw is 1 − 2⁻⁵³ exactly"
    );
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let (sites, site) = if name == "fused" { (5, 1) } else { (10, 3) };
        let state = measured_on(&engine, sites, site, true);
        let values = engine
            .sample(&state, &[site, 0], 1, SEED_OF_THE_LARGEST_UNIFORM)
            .expect("a sample");
        assert!(values[0], "{name}");
    }
}

/// A sample of site 2 after a measurement of site 2 drew Zero, with the
/// seed whose first uniform is 0: the sample draws Zero, not the One of
/// probability zero.
#[test]
fn a_sample_draws_zero_where_one_has_probability_zero() {
    // The smallest positive `f64`, so a uniform is below it only when it is
    // 0 exactly.
    assert!(
        first_draw(f64::from_bits(1), SEED_OF_A_ZERO_UNIFORM),
        "the seed's first draw is 0 exactly"
    );
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let state = measured_on(&engine, 6, 2, false);
        let values = engine
            .sample(&state, &[2], 1, SEED_OF_A_ZERO_UNIFORM)
            .expect("a sample");
        assert!(!values[0], "{name}");
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
    let hadamard = [c(h), c(h), c(h), c(-h)];
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
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
