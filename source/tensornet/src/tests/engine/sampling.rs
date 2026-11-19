// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The shared sampler, through its public function, on conditional
//! probabilities a test chooses.

use crate::sampling::splitmix64::SplitMix64;
use crate::sampling::{TooManyValues, sample_by_conditionals};
use std::collections::BTreeSet;

/// The error of the tests' conditional probabilities: the sampler's own,
/// or one the probability call returns.
#[derive(Debug, PartialEq)]
enum Error {
    TooMany(TooManyValues),
    Device,
}

impl From<TooManyValues> for Error {
    fn from(error: TooManyValues) -> Self {
        Self::TooMany(error)
    }
}

/// The seed whose first uniform is 0, and the seed whose first uniform is
/// 1 − 2⁻⁵³, the largest, in the published `SplitMix64` stream
/// (`a_value_of_probability_zero_is_never_drawn` checks both).
const SEED_OF_A_ZERO_UNIFORM: u64 = 0x61c8_8646_80b5_83eb;
const SEED_OF_THE_LARGEST_UNIFORM: u64 = 0x3162_8af6_7b21_31ab;

/// Sites 5 and 3 where site 5 is always One and site 3 always Zero: every
/// shot reads `[true, false]`, in the order the sites are asked for.
#[test]
fn values_are_shot_major_in_the_order_of_the_sites() {
    let values = sample_by_conditionals(&[5, 3], 3, 7, |site, _| {
        Ok::<_, Error>(if site == 5 { 1.0 } else { 0.0 })
    })
    .expect("three shots of two sites fit");
    assert_eq!(values, [true, false, true, false, true, false]);
}

/// With P(One) ½ everywhere, each value is One exactly when the next
/// uniform of the seed's `SplitMix64` stream is below ½, one uniform per
/// value, shot after shot.
#[test]
fn each_value_draws_the_next_uniform_of_the_seed() {
    for seed in [0, 7, u64::MAX] {
        let values = sample_by_conditionals(&[0, 1], 32, seed, |_, _| Ok::<_, Error>(0.5))
            .expect("64 values fit");
        let mut stream = SplitMix64::new(seed);
        let expected: Vec<bool> = (0..64).map(|_| stream.next_uniform() < 0.5).collect();
        assert_eq!(values, expected, "seed {seed}");
    }
}

/// The uniforms are those of the published `SplitMix64` algorithm (Steele,
/// Lea and Flood, 2014), so a seed's values do not change with this
/// crate's version: seed 1234567's first five values, a test vector its
/// implementations share, each taken as its top 53 bits times 2⁻⁵³. A value
/// is Zero when P(One) is its uniform, and One when P(One) is the next
/// multiple of 2⁻⁵³.
#[test]
fn the_uniforms_are_the_published_splitmix64_stream() {
    const PUBLISHED: [u64; 5] = [
        6_457_827_717_110_365_317,
        3_203_168_211_198_807_973,
        9_817_491_932_198_370_423,
        4_593_380_528_125_082_431,
        16_408_922_859_458_223_821,
    ];
    #[expect(clippy::cast_precision_loss, reason = "53 bits fit an f64")]
    let uniform = |value: u64, above: u64| ((value >> 11) + above) as f64 / 2f64.powi(53);
    for (above, expected) in [(0, false), (1, true)] {
        let values = sample_by_conditionals(&[0, 1, 2, 3, 4], 1, 1_234_567, |site, _| {
            Ok::<_, Error>(uniform(PUBLISHED[site], above))
        })
        .expect("five values fit");
        assert_eq!(values, [expected; 5], "P(One) = uniform + {above}·2⁻⁵³");
    }
}

/// Each probability is asked given the shot's earlier values, in the order
/// of the sites: every prefix of every shot drawn is asked for.
#[test]
fn a_probability_is_asked_given_the_shots_earlier_values() {
    let sites = [2, 0, 1];
    let mut asked = BTreeSet::new();
    let values = sample_by_conditionals(&sites, 50, 11, |site, given: &[(usize, bool)]| {
        asked.insert((site, given.to_vec()));
        Ok::<_, Error>(0.5)
    })
    .expect("150 values fit");
    for shot in values.chunks_exact(sites.len()) {
        for t in 0..sites.len() {
            let given: Vec<_> = sites
                .iter()
                .copied()
                .zip(shot.iter().copied())
                .take(t)
                .collect();
            assert!(
                asked.contains(&(sites[t], given.clone())),
                "P(One) of site {} given {given:?} was not asked",
                sites[t]
            );
        }
    }
}

/// Shots that share a prefix share its probability: 200 shots of two
/// sites ask once for the first site and once per value it drew.
#[test]
fn a_probability_is_asked_once_per_distinct_prefix() {
    let mut asked = Vec::new();
    let values = sample_by_conditionals(&[0, 1], 200, 3, |site, given: &[(usize, bool)]| {
        asked.push((site, given.to_vec()));
        Ok::<_, Error>(0.5)
    })
    .expect("400 values fit");
    let prefixes: BTreeSet<_> = asked.iter().cloned().collect();
    assert_eq!(asked.len(), prefixes.len(), "a prefix was asked twice");
    let first_values: BTreeSet<bool> = values.iter().step_by(2).copied().collect();
    assert_eq!(asked.len(), 1 + first_values.len());
}

/// A value of probability exactly zero is never drawn, even at the
/// extreme uniforms: Zero where P(One) is 0 and the uniform is 0, One
/// where P(One) is 1 and the uniform is the largest. The seeds' uniforms
/// are those extremes, checked first as preconditions: P(One) 2⁻⁵³ draws
/// One from the first, and P(One) 1 − 2⁻⁵³ draws Zero from the second.
#[test]
fn a_value_of_probability_zero_is_never_drawn() {
    let largest = 1.0 - f64::EPSILON / 2.0;
    let draw = |seed, p1| sample_by_conditionals(&[0], 1, seed, |_, _| Ok::<_, Error>(p1));
    assert_eq!(
        draw(SEED_OF_A_ZERO_UNIFORM, 2f64.powi(-53)),
        Ok(vec![true]),
        "precondition: the seed gives the uniform 0"
    );
    assert_eq!(
        draw(SEED_OF_THE_LARGEST_UNIFORM, largest),
        Ok(vec![false]),
        "precondition: the seed gives the uniform 1 − 2⁻⁵³"
    );
    for (seed, p1, expected) in [
        (SEED_OF_A_ZERO_UNIFORM, 0.0, false),
        (SEED_OF_THE_LARGEST_UNIFORM, 1.0, true),
    ] {
        assert_eq!(
            draw(seed, p1),
            Ok(vec![expected]),
            "seed {seed:#x}, P(One) {p1}"
        );
    }
}

/// No sites, or no shots, give no values at once, asking nothing, however
/// many shots.
#[test]
fn no_values_ask_nothing() {
    let mut asked = 0;
    let mut ask = |_, _: &[(usize, bool)]| {
        asked += 1;
        Ok::<_, Error>(0.5)
    };
    assert_eq!(
        sample_by_conditionals(&[], usize::MAX, 1, &mut ask),
        Ok(Vec::new())
    );
    assert_eq!(
        sample_by_conditionals(&[0, 1], 0, 1, &mut ask),
        Ok(Vec::new())
    );
    assert_eq!(asked, 0);
}

/// More values than a `usize` counts fail with `TooManyValues`, before
/// any probability is asked: `2^(bits − 1)` shots of two sites, a count
/// that wraps to zero, and would otherwise give no values and no error.
#[test]
fn values_a_usize_cannot_count_fail() {
    let shots = usize::MAX / 2 + 1;
    let result = sample_by_conditionals(&[0, 1], shots, 1, |_, _| {
        panic!("no probability is asked of a sample that cannot be held")
    });
    assert_eq!(
        result,
        Err::<Vec<bool>, _>(Error::TooMany(TooManyValues { shots, sites: 2 }))
    );
}

/// Values a `usize` counts but no allocation holds (more bytes than an
/// `isize`) fail with `TooManyValues`, not a panic, before any probability
/// is asked.
#[test]
fn values_no_allocation_holds_fail() {
    let shots = usize::MAX / 2 + 1;
    let result = sample_by_conditionals(&[4], shots, 1, |_, _| {
        panic!("no probability is asked of a sample that cannot be held")
    });
    assert_eq!(
        result,
        Err::<Vec<bool>, _>(Error::TooMany(TooManyValues { shots, sites: 1 }))
    );
}

/// An error of the probability call is returned as it is, and the draw
/// stops there: a device that fails on its second call is not asked again.
#[test]
fn an_error_of_the_probability_call_stops_the_draw() {
    let mut calls = 0;
    let result = sample_by_conditionals(&[0, 1, 2], 10, 5, |_, _| {
        calls += 1;
        if calls == 2 {
            Err(Error::Device)
        } else {
            Ok(0.5)
        }
    });
    assert_eq!(result, Err(Error::Device));
    assert_eq!(calls, 2);
}
