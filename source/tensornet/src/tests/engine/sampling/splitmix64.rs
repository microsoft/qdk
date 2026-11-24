// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The generator's streams: the published `SplitMix64` values, and the
//! uniforms taken from them.

use crate::sampling::SplitMix64;

/// The seed whose first uniform is 0, and the seed whose first uniform is
/// 1 − 2⁻⁵³, the largest (`a_uniform_reaches_zero_and_its_largest_value`
/// checks both).
pub(super) const SEED_OF_A_ZERO_UNIFORM: u64 = 0x61c8_8646_80b5_83eb;
pub(super) const SEED_OF_THE_LARGEST_UNIFORM: u64 = 0x3162_8af6_7b21_31ab;

/// A test that draws its random circuits from a seed, here or in a crate
/// that uses an engine, gets the same circuits from every release: seed
/// 1234567's first five values are those of Vigna's reference
/// implementation, `splitmix64.c`, as Rosetta Code's `SplitMix64` task lists
/// them.
#[test]
fn a_seed_gives_the_published_splitmix64_values() {
    let mut stream = SplitMix64::new(1_234_567);
    let values: Vec<u64> = (0..5).map(|_| stream.next_u64()).collect();
    assert_eq!(
        values,
        [
            6_457_827_717_110_365_317,
            3_203_168_211_198_807_973,
            9_817_491_932_198_370_423,
            4_593_380_528_125_082_431,
            16_408_922_859_458_223_821,
        ]
    );
}

/// The sampler compares each probability with a uniform: each is the top
/// 53 bits of the stream's next value times 2⁻⁵³, one value per uniform,
/// whether the values before it were taken as values or as uniforms.
#[test]
fn a_uniform_is_the_top_53_bits_of_the_next_value() {
    for seed in [0, 1_234_567, u64::MAX] {
        let mut values = SplitMix64::new(seed);
        let mut mixed = SplitMix64::new(seed);
        for step in 0..64 {
            let value = values.next_u64();
            if step % 3 == 0 {
                assert_eq!(mixed.next_u64(), value, "seed {seed}, step {step}");
            } else {
                #[expect(clippy::cast_precision_loss, reason = "53 bits fit an f64")]
                let expected = (value >> 11) as f64 * 2f64.powi(-53);
                assert_eq!(mixed.next_uniform(), expected, "seed {seed}, step {step}");
            }
        }
    }
}

/// A uniform is in [0, 1), with both ends of its range reached: 0 from one
/// seed, and the largest, 1 − 2⁻⁵³, from another. So a sampler that draws
/// One below P(One) never draws a value of probability zero.
#[test]
fn a_uniform_reaches_zero_and_its_largest_value() {
    assert_eq!(SplitMix64::new(SEED_OF_A_ZERO_UNIFORM).next_uniform(), 0.0);
    assert_eq!(
        SplitMix64::new(SEED_OF_THE_LARGEST_UNIFORM).next_uniform(),
        1.0 - 2f64.powi(-53)
    );
}
