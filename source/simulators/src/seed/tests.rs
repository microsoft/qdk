// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{RngRole, derive_seed, rng_for_role, share_draws};
use expect_test::expect;
use rand::{Rng as _, SeedableRng as _, rngs::StdRng};
use rustc_hash::FxHashSet;
use std::fmt::Write as _;

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
        let roles = [
            RngRole::Measurement,
            RngRole::Noise,
            RngRole::Classical,
            RngRole::Shot(0),
            RngRole::Shot(1),
            RngRole::Evaluation(0),
            RngRole::Evaluation(1),
        ];
        // A component that seeds its generator without deriving gets the first stream.
        let rngs = std::iter::once(StdRng::seed_from_u64(seed))
            .chain(roles.map(|role| rng_for_role(seed, role)));
        assert!(!share_draws(rngs), "streams from seed {seed} share draws");
    }
}

#[test]
fn nearby_seeds_shots_and_evaluations_get_distinct_seeds() {
    const COUNT: u64 = 256;
    let mut derived = FxHashSet::default();
    for seed in 0..4 {
        for n in 0..COUNT {
            for role in [RngRole::Shot(n), RngRole::Evaluation(n)] {
                assert!(
                    derived.insert(derive_seed(seed, role)),
                    "{role:?} of seed {seed} repeats another derived seed"
                );
            }
        }
    }
}

#[test]
fn derived_values_are_stable() {
    // Every seeded result depends on these values. If they change on purpose (for example,
    // after a `rand` upgrade), seeded outputs change everywhere; update with `UPDATE_EXPECT=1`.
    let mut actual = String::new();
    for role in [
        RngRole::Measurement,
        RngRole::Noise,
        RngRole::Classical,
        RngRole::Shot(1),
        RngRole::Evaluation(1),
    ] {
        let generated = rng_for_role(0, role).next_u64();
        let derived = derive_seed(0, role);
        writeln!(actual, "{role:?}: {generated:#018x} {derived:#018x}")
            .expect("writing should succeed");
    }
    expect![[r#"
        Measurement: 0x2d551d11825e4ac0 0x9de338a2ecbcb780
        Noise: 0x55b64798c51dc3a2 0xbefdd9bad9d69d6b
        Classical: 0x9a263a0214ddfd24 0x0065f07eb7a2d77e
        Shot(1): 0xb8849d47d3d9f073 0x426fb6260e07af4a
        Evaluation(1): 0xda4df69121e1152e 0xe2b1e225fce78e7f
    "#]]
    .assert_eq(&actual);
}
