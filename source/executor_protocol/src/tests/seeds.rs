// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use crate::{BuildError, Capabilities, RngId, RngIds, ShotContext, ShotId, shot_seed};

/// Two shots of a run never share a seed.
#[test]
fn a_runs_shots_never_share_a_seed() {
    for seed in [0, 7, u64::MAX] {
        let seeds = (0..1 << 16)
            .map(|j| shot_seed(seed, j))
            .collect::<BTreeSet<_>>();
        assert_eq!(seeds.len(), 1 << 16, "seed {seed}: a shot shares its seed");
    }
}

/// A shot's seed is fixed: a recorded run keeps its seeds when a dependency
/// changes. Known answers, from an independent implementation of `SplitMix64`.
#[test]
fn shot_seeds_are_fixed() {
    let known = [
        (0, 0, 12_035_550_249_420_947_055),
        (0, 1, 627_405_149_472_732_430),
        (7, 1, 4_414_019_431_610_648_415),
        (7, 1000, 6_785_964_006_689_842_358),
        (u64::MAX, ShotId::MAX, 16_813_757_279_435_679_236),
    ];
    let got = known
        .iter()
        .map(|&(seed, j, _)| (seed, j, shot_seed(seed, j)))
        .collect::<Vec<_>>();
    assert_eq!(got, known);
}

/// The last executor draws from the shot's seed: its seed in a shot's
/// context, under either name (`seed`, the one a state of an executor is
/// made from, and its component seed, `rng_seed(LAST_EXECUTOR)`), is the
/// shot's seed.
#[test]
fn the_last_executors_seed_is_the_shots_seed() {
    for (seed, j) in [(0, 0), (7, 3), (u64::MAX, ShotId::MAX)] {
        let ctx = ShotContext::last_executor(seed, j);
        assert_eq!(
            (ctx.seed(), ctx.rng_seed(RngId::LAST_EXECUTOR)),
            (shot_seed(seed, j), shot_seed(seed, j)),
            "seed {seed}, shot {j}"
        );
    }
}

/// Every component of every shot of a run has a seed of its own.
#[test]
fn each_component_of_a_shot_has_its_own_seed() {
    let ids = [
        RngId::LAST_EXECUTOR,
        RngId::named("a"),
        RngId::named("b"),
        RngId::named("readout"),
    ];
    let mut seen = BTreeSet::new();
    for seed in [0, 7] {
        for j in 0..64 {
            let ctx = ShotContext::last_executor(seed, j);
            for id in ids {
                assert!(seen.insert((seed, ctx.rng_seed(id))), "{id} of shot {j}");
            }
        }
    }
}

/// A component's id, and so its seed in a shot, is fixed: it is a hash of
/// its name. Known answers, from an independent implementation of FNV-1a and
/// `SplitMix64`.
#[test]
fn rng_ids_and_their_seeds_are_fixed() {
    let ctx = ShotContext::last_executor(7, 3);
    assert_eq!(
        [
            ctx.rng_seed(RngId::named("readout")),
            ctx.rng_seed(RngId::named("")),
        ],
        [0xf798_a1e2_8cb5_77d2, 0xe760_2ebc_7c07_2703]
    );
}

/// A component that draws adds its id to those drawn with downstream of it,
/// which hold the last executor's alone in a chain of one executor.
#[test]
fn a_component_that_draws_adds_its_id_to_those_downstream() {
    let noise = RngId::named("noise");
    let offered = Capabilities::default()
        .drawing(noise)
        .expect("nothing draws with `noise` yet");
    assert_eq!(
        offered.rng_ids,
        RngIds::last_executor()
            .with(noise)
            .expect("the last executor does not draw with `noise`")
    );
}

/// A component that draws is refused when built against one that already draws
/// with its id, the last executor's included, since the two would draw the
/// same numbers.
#[test]
fn an_rng_id_is_drawn_from_once_per_chain() {
    let noise = RngId::named("noise");
    let offered = Capabilities::default()
        .drawing(noise)
        .expect("nothing draws with `noise` yet");
    assert_eq!(offered.drawing(noise), Err(BuildError::RngInUse(noise)));
    assert_eq!(
        Capabilities::default().drawing(RngId::LAST_EXECUTOR),
        Err(BuildError::RngInUse(RngId::LAST_EXECUTOR))
    );
}

/// An id is written as the random numbers it names, so that a refusal
/// citing it says whose they are: the last executor's, or a component's by its
/// hash.
#[test]
fn an_id_is_written_as_the_random_numbers_it_names() {
    assert_eq!(
        RngId::LAST_EXECUTOR.to_string(),
        "the last executor's random numbers"
    );
    assert_eq!(
        RngId::named("readout").to_string(),
        "the random numbers with id 0xb1926246"
    );
}
