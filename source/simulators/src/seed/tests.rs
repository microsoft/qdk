// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Stream, derive_seed, share_draws, stream_rng};
use rand::{Rng as _, SeedableRng as _, rngs::StdRng};
use rustc_hash::FxHashSet;

#[test]
fn share_draws_detects_offset_copies_of_a_stream() {
    let mut offset = StdRng::seed_from_u64(0);
    for _ in 0..3 {
        offset.next_u64();
    }
    assert!(share_draws([StdRng::seed_from_u64(0), offset]));
}

#[test]
fn streams_derived_from_one_seed_share_no_draws() {
    for seed in [0, 1, 2, 42, u64::MAX] {
        let streams = [
            Stream::Measurement,
            Stream::Noise,
            Stream::Classical,
            Stream::Shot(0),
            Stream::Shot(1),
        ];
        // A component that seeds its generator without deriving gets the first stream.
        let rngs = std::iter::once(StdRng::seed_from_u64(seed))
            .chain(streams.map(|stream| stream_rng(seed, stream)));
        assert!(!share_draws(rngs), "streams from seed {seed} share draws");
    }
}

#[test]
fn runs_with_nearby_seeds_share_no_shots() {
    const SHOTS: u64 = 256;
    let mut shot_seeds = FxHashSet::default();
    for seed in 0..4 {
        for shot in 0..SHOTS {
            assert!(
                shot_seeds.insert(derive_seed(seed, Stream::Shot(shot))),
                "shot {shot} of the run with seed {seed} repeats another shot's seed"
            );
        }
    }
}
