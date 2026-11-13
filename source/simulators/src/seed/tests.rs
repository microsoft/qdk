// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{KeyKind, RngRole, SeedRole, make_key, make_rng, make_seed, share_draws};
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
        let roles = [RngRole::Measurement, RngRole::Noise, RngRole::Classical];
        let seeds = std::iter::once(seed).chain(
            [
                SeedRole::Shot(0),
                SeedRole::Shot(1),
                SeedRole::Evaluation(0),
                SeedRole::Evaluation(1),
            ]
            .map(|role| make_seed(seed, role)),
        );
        // A component that seeds its generator without deriving gets the first stream.
        let rngs = std::iter::once(StdRng::seed_from_u64(seed))
            .chain(seeds.flat_map(|seed| roles.map(|role| make_rng(seed, role))));
        assert!(!share_draws(rngs), "streams from seed {seed} share draws");
    }
}

#[test]
fn nearby_seeds_shots_and_evaluations_get_distinct_seeds() {
    const COUNT: u64 = 256;
    let mut derived = FxHashSet::default();
    for seed in 0..4 {
        for n in 0..COUNT {
            for role in [SeedRole::Shot(n), SeedRole::Evaluation(n)] {
                assert!(
                    derived.insert(make_seed(seed, role)),
                    "{role:?} of seed {seed} repeats another derived seed"
                );
            }
        }
    }
}

#[test]
fn seed_and_generator_keys_with_the_same_label_are_distinct() {
    assert_ne!(
        make_key(0, 1, 0, KeyKind::Seed),
        make_key(0, 1, 0, KeyKind::Generator)
    );
}

#[test]
fn derived_values_are_stable() {
    // Every seeded result depends on these values. If they change on purpose (for example,
    // after a `rand` upgrade), seeded outputs change everywhere; update with `UPDATE_EXPECT=1`.
    let mut actual = String::new();
    for role in [RngRole::Measurement, RngRole::Noise, RngRole::Classical] {
        let generated = make_rng(0, role).next_u64();
        writeln!(actual, "{role:?}: {generated:#018x}").expect("writing should succeed");
    }
    for role in [SeedRole::Shot(1), SeedRole::Evaluation(1)] {
        let derived = make_seed(0, role);
        writeln!(actual, "{role:?}: {derived:#018x}").expect("writing should succeed");
    }
    expect![[r#"
        Measurement: 0xa31d20039a4de169
        Noise: 0x24f4f3b273705a14
        Classical: 0x2d551d11825e4ac0
        Shot(1): 0xc39f6466afce1570
        Evaluation(1): 0x426fb6260e07af4a
    "#]]
    .assert_eq(&actual);
}
