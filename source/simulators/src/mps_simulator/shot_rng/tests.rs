// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot's RNG: a shot draws from `SplitMix64` seeded with
//! `mix(mix(seed) ^ shot)`, fixed by the run's seed and its id alone. The
//! generator itself is `tensornet`'s, tested there.

use crate::mps_simulator::MpsResumer;
use tensornet::reference::ExactReference;
use tensornet::sampling::SplitMix64;

use crate::mps_simulator::test_utils::programs::coins;
use crate::mps_simulator::test_utils::{ids, results, run};

/// A shot draws from `SplitMix64` seeded with `mix(mix(seed) ^ shot)`,
/// with `mix(x)` the first value of `SplitMix64::new(x)`: its RNG is
/// fixed by the run's seed and its id alone. Sixteen coins, read in order,
/// are the first sixteen uniforms of `SplitMix64` from that seed, each
/// against ½, for seeds and shots from zero to the largest.
#[test]
fn a_shots_rng_is_seeded_from_the_run_seed_and_its_id() {
    // Known answers of `mix(mix(seed) ^ shot)`, from an independent
    // implementation of `SplitMix64`.
    let known: [(u64, u32, u64); 5] = [
        (0, 0, 12_035_550_249_420_947_055),
        (0, 1, 627_405_149_472_732_430),
        (7, 1, 4_414_019_431_610_648_415),
        (7, 1000, 6_785_964_006_689_842_358),
        (u64::MAX, u32::MAX, 16_813_757_279_435_679_236),
    ];
    let program = coins(16);
    for (seed, shot, shot_seed) in known {
        let mut rng = SplitMix64::new(shot_seed);
        let bits: Vec<u8> = (0..16)
            .map(|_| u8::from(rng.next_uniform() < 0.5))
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
