// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot's stream: a shot draws from the stream seeded with the seed the
//! executor protocol (planned) will give a chain's last executor in that
//! shot; and `SplitMix64`, its generator, gives the published values, a
//! uniform being the top 53 bits of the next value, from 0 included to 1
//! excluded.

use crate::mps_simulator::MpsResumer;
use crate::mps_simulator::resumer::shot_stream::SplitMix64;
use tensornet::reference::ExactReference;

use super::super::programs::coins;
use super::{ids, results, run};

/// A shot draws from the stream seeded with the seed the executor protocol
/// (planned) will give a chain's last executor, the one that runs the
/// quantum calls, in that shot: so such a chain, its last executor drawing
/// as this MPS simulation method does, will reproduce the records a
/// resumer gives. Sixteen coins, read in order, are the first sixteen
/// uniforms of `SplitMix64` from that seed, each against ½, for the run
/// seeds and shots whose seeds the protocol's own tests fix.
#[test]
fn a_shot_draws_from_the_stream_of_the_protocols_last_executor() {
    // The protocol's known answers for its `shot_seed(seed, shot)`, from an
    // independent implementation of `SplitMix64`.
    let known: [(u64, u32, u64); 5] = [
        (0, 0, 12_035_550_249_420_947_055),
        (0, 1, 627_405_149_472_732_430),
        (7, 1, 4_414_019_431_610_648_415),
        (7, 1000, 6_785_964_006_689_842_358),
        (u64::MAX, u32::MAX, 16_813_757_279_435_679_236),
    ];
    let program = coins(16);
    for (seed, shot, shot_seed) in known {
        let mut stream = SplitMix64::new(shot_seed);
        let bits: Vec<u8> = (0..16)
            .map(|_| u8::from(stream.next_uniform() < 0.5))
            .collect();
        let records = run(
            &mut MpsResumer::new(ExactReference::new(), seed),
            &program,
            &ids(&[shot]),
        )
        .expect("the program runs");
        assert_eq!(records, vec![results(&bits)], "seed {seed}, shot {shot}");
    }
}

/// The stream of `shot` of a run seeded `seed`, as the tests' oracle: with
/// `mix(x)` the first value of `SplitMix64::new(x)`, the seed the executor
/// protocol (planned) gives a chain's last executor in that shot is
/// `mix(mix(seed) ^ shot)`.
pub(super) fn shot_stream(seed: u64, shot: u32) -> SplitMix64 {
    fn mix(x: u64) -> u64 {
        SplitMix64::new(x).next_u64()
    }
    SplitMix64::new(mix(mix(seed) ^ u64::from(shot)))
}

/// The values the published algorithm (Steele, Lea and Flood, 2014) gives
/// for seed 1234567, a test vector its implementations share: a shot's
/// draws, and the seed the executor protocol (planned) gives it, are
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
/// 1 − 2⁻⁵³, and the seed whose first value is zero gives 0: the range a
/// shot's draws rely on, as a shot reads One when its uniform is below
/// P(One).
#[test]
fn a_uniform_reaches_zero_and_stays_below_one() {
    let largest = SplitMix64::new(0x3162_8af6_7b21_31ab).next_uniform();
    assert_eq!(largest.to_bits(), (1.0 - f64::EPSILON / 2.0).to_bits());
    let smallest = SplitMix64::new(0x61c8_8646_80b5_83eb).next_uniform();
    assert_eq!(smallest.to_bits(), 0.0f64.to_bits());
}
