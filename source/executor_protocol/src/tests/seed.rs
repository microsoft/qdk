// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Each shot's stream is a fixed function of the run's seed and the shot's
//! id; a derived seed's streams are apart from the run's and from other
//! keys'.

use std::collections::BTreeSet;

use crate::{Seed, ShotId};

/// Shot `shot`'s stream in a run seeded with `seed`.
fn stream(seed: u64, shot: u32) -> u64 {
    Seed::new(seed, true).shot(ShotId::new(shot))
}

/// A shot's stream is fixed by the run's seed and the shot's id, the same on
/// every platform and version of Rust, so a simulator that seeds its shots
/// the same way gives the same records alone and as a chain's executor.
// COMPATIBILITY NOTE: these answers are those of the MPS simulation
// method, which seeds its shot j of a run seeded s with
// `SplitMix64::new(mix(mix(s) ^ j))`, and of the earlier protocol's
// `shot_seed`, hence we keep them.
#[test]
fn shot_streams_are_fixed_by_seed_and_id() {
    assert_eq!(stream(0, 0), 12_035_550_249_420_947_055);
    assert_eq!(stream(0, 1), 627_405_149_472_732_430);
    assert_eq!(stream(7, 1), 4_414_019_431_610_648_415);
    assert_eq!(stream(7, 1000), 6_785_964_006_689_842_358);
    assert_eq!(stream(u64::MAX, u32::MAX), 16_813_757_279_435_679_236);
}

/// The shots of a run never share a stream, for each of several seeds.
#[test]
fn shots_have_distinct_streams() {
    for seed in [0, 7, u64::MAX] {
        let mut seen = BTreeSet::new();
        for shot in 0..1 << 16 {
            assert!(
                seen.insert(stream(seed, shot)),
                "seed {seed}, shot {shot}: a stream seen before"
            );
        }
    }
}

/// A noise model that derives its seed never draws the executor's numbers,
/// for any shot, nor another key's: the keys here include the empty one and
/// `"(BpV!"`, whose FNV-1a hash folds to 0.
#[test]
fn derived_streams_are_apart_from_the_runs_and_other_keys() {
    let run = Seed::new(7, true);
    let keys = ["bit_flip", "readout", "", "(BpV!"];
    let mut seen = BTreeSet::new();
    for shot in 0..1 << 12 {
        assert!(seen.insert(run.shot(ShotId::new(shot))));
    }
    for key in keys {
        let derived = run.derive(key);
        for shot in 0..1 << 12 {
            assert!(
                seen.insert(derived.shot(ShotId::new(shot))),
                "key {key:?}, shot {shot}: a stream of the run or of another key"
            );
        }
    }
}

/// A derived seed's streams are fixed by the run's seed and the key, the
/// same on every platform and version of Rust.
// The answers were computed by an independent implementation of FNV-1a 64
// and `SplitMix64`, from their published constants.
#[test]
fn derived_streams_are_fixed_by_seed_and_key() {
    let derived = |seed, key, shot| Seed::new(seed, true).derive(key).shot(ShotId::new(shot));
    assert_eq!(derived(7, "bit_flip", 3), 11_403_248_577_888_900_587);
    assert_eq!(derived(7, "readout", 3), 2_618_841_840_799_957_620);
    assert_eq!(derived(0, "", 0), 9_623_513_680_988_388_014);
    assert_eq!(
        derived(u64::MAX, "(BpV!", u32::MAX),
        1_350_438_703_845_026_390
    );
}
