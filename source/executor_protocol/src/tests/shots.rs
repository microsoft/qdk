// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use crate::{
    BuildError, Capabilities, ComponentId, Need, Needs, Rejected, ShotContext, ShotId, ShotSet,
    Unmet, shot_seed,
};

#[test]
fn a_shot_set_names_at_least_one_shot_each_once() {
    assert_eq!(ShotSet::new(Vec::new()), Err(Rejected::NoShots));
    assert_eq!(
        ShotSet::new(vec![5, 2, 5, 2]),
        Err(Rejected::RepeatedShot(2))
    );
    let (high, low) = (3, 2);
    assert_eq!(ShotSet::range(high..=low), Err(Rejected::NoShots));
    let given = ShotSet::new(vec![4, 1, 3]).expect("a set");
    assert_eq!(given.as_slice(), [4, 1, 3], "the caller's order");
    assert_eq!(ShotSet::try_from(&[4, 1, 3][..]), Ok(given.clone()));
    let all = ShotSet::range(1..=4).expect("a set");
    let even = all.subset(|j| j % 2 == 0).expect("some");
    assert_eq!(even.as_slice(), [2, 4]);
    assert_eq!(all.subset(|j| j > 4), None);
    assert_eq!(ShotSet::one(7).as_slice(), [7]);
    assert_eq!((&all).into_iter().count(), 4);
}

/// Two shots of a session never share a seed, and the seeds are fixed: a
/// recorded run keeps its seeds when a dependency changes.
#[test]
fn shot_seeds_are_distinct_and_fixed() {
    for seed in [0, 7, u64::MAX] {
        let seeds = (0..1 << 16)
            .map(|j| shot_seed(seed, j))
            .collect::<BTreeSet<_>>();
        assert_eq!(seeds.len(), 1 << 16, "seed {seed}: a shot shares its seed");
        assert!(!seeds.contains(&seed), "seed {seed}");
    }
    // The prototype's known answers (its test CT7), from an independent
    // implementation of SplitMix64.
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

/// The terminal's seed in a shot is the shot's seed; every other component
/// of the shot, and every other shot, has its own; component ids are fixed.
#[test]
fn each_component_of_a_shot_has_its_own_seed() {
    let ids = [
        ComponentId::TERMINAL,
        ComponentId::named("a"),
        ComponentId::named("b"),
        ComponentId::named("readout"),
    ];
    let mut seen = BTreeSet::new();
    for seed in [0, 7] {
        for j in 0..64 {
            let ctx = ShotContext::new(seed, j);
            assert_eq!((ctx.seed(), ctx.shot()), (seed, j));
            assert_eq!(ctx.rng_seed(ComponentId::TERMINAL), shot_seed(seed, j));
            for id in ids {
                assert!(seen.insert((seed, ctx.rng_seed(id))), "{id} of shot {j}");
            }
        }
    }
    // The prototype's known answers (its test P4-1).
    assert_eq!(ComponentId::named("readout").get(), 0xb192_6246);
    assert_eq!(ComponentId::named("").get(), 0x4fd0_bfc1);
    assert_eq!(
        ShotContext::new(7, 3).rng_seed(ComponentId::named("readout")),
        0xf798_a1e2_8cb5_77d2
    );
    assert_eq!(ComponentId::TERMINAL.get(), 0);
}

/// A component that draws is refused when built over one that already draws
/// with its id, since the two would draw the same numbers.
#[test]
fn a_component_id_is_drawn_from_once_per_stack() {
    let noise = ComponentId::named("noise");
    let offered = Capabilities::default().drawing(noise).expect("a free id");
    assert_eq!(
        offered.rngs.as_slice(),
        [ComponentId::TERMINAL, noise],
        "the terminal's first, then from the bottom up"
    );
    assert_eq!(
        offered.clone().drawing(noise),
        Err(BuildError::RngInUse(noise))
    );
    assert!(offered.rngs.contains(noise));
    assert_eq!(
        Capabilities::default().drawing(ComponentId::TERMINAL),
        Err(BuildError::RngInUse(ComponentId::TERMINAL))
    );
    assert_eq!(
        BuildError::RngInUse(ComponentId::TERMINAL).to_string(),
        "the terminal's random numbers are already drawn from below: two components would share them"
    );
}

/// Each need beyond what a component offers is refused, naming the
/// component; the default needs fit every component.
#[test]
fn needs_beyond_the_capabilities_are_unmet() {
    let shots = ShotSet::range(1..=3).expect("a set");
    let small = Capabilities::default()
        .with_qubits(Some(2))
        .with_feedback(false)
        .with_resets(false)
        .with_replayable(false)
        .with_states(Some(2));
    // Three shots, two states: the only default need it cannot meet.
    assert_eq!(
        small.admit("small", &shots, &Needs::default()),
        Err(Rejected::Unmet(Unmet {
            component: "small".into(),
            need: Need::States,
            needed: 3,
            offered: 2,
        }))
    );
    let unmet = |needs: Needs| match small.admit("small", &ShotSet::one(1), &needs) {
        Err(Rejected::Unmet(unmet)) => Some((unmet.need, unmet.needed, unmet.offered)),
        _ => None,
    };
    assert_eq!(
        unmet(Needs::default().with_qubits(Some(3))),
        Some((Need::Qubits, 3, 2))
    );
    assert_eq!(unmet(Needs::default().with_qubits(Some(2))), None);
    assert_eq!(
        unmet(Needs::default().with_feedback(true)),
        Some((Need::Feedback, 1, 0))
    );
    assert_eq!(
        unmet(Needs::default().with_resets(true)),
        Some((Need::Resets, 1, 0))
    );
    assert_eq!(
        unmet(Needs::default().with_replayable(true)),
        Some((Need::Replayable, 1, 0))
    );
    assert_eq!(
        unmet(Needs::default().with_probabilities(true)),
        Some((Need::Probabilities, 1, 0))
    );
    let any = Capabilities::default();
    assert_eq!(any.admit("any", &shots, &Needs::default()), Ok(()));
    assert_eq!(
        any.with_probabilities(true).admit(
            "any",
            &shots,
            &Needs::default()
                .with_qubits(Some(1000))
                .with_feedback(true)
                .with_resets(true)
                .with_replayable(true)
                .with_probabilities(true)
        ),
        Ok(())
    );
}
