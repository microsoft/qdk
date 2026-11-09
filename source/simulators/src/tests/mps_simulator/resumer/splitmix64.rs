// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::mps_simulator::resumer::splitmix64::SplitMix64;

/// The values the published algorithm (Steele, Lea and Flood, 2014) gives
/// for seed 1234567, a test vector its implementations share: a shot's
/// draws, and the seed the executor protocol (planned) derives for it, are
/// `SplitMix64`'s, whatever this crate's version.
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

/// A uniform is the next value's top 53 bits as a multiple of 2⁻⁵³: the
/// first of seed 1234567 is that of its first published value.
#[test]
fn a_uniform_is_the_top_53_bits_of_the_next_value() {
    let first: u64 = 6_457_827_717_110_365_317;
    #[expect(clippy::cast_precision_loss, reason = "53 bits fit an f64")]
    let expected = (first >> 11) as f64 / 2f64.powi(53);
    assert_eq!(
        SplitMix64::new(1_234_567).next_uniform().to_bits(),
        expected.to_bits()
    );
}

/// The seed whose first value is all ones gives the largest uniform,
/// 1 − 2⁻⁵³, and the seed whose first value is zero gives 0: a shot that
/// reads One below P(One) never reads a value of probability zero.
#[test]
fn a_uniform_reaches_zero_and_stays_below_one() {
    let largest = SplitMix64::new(0x3162_8af6_7b21_31ab).next_uniform();
    assert_eq!(largest.to_bits(), (1.0 - f64::EPSILON / 2.0).to_bits());
    let smallest = SplitMix64::new(0x61c8_8646_80b5_83eb).next_uniform();
    assert_eq!(smallest.to_bits(), 0.0f64.to_bits());
}
