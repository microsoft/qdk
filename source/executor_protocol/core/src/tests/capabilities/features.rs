// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What `Features` adds to the one rule of every kind: replay requires
//! seeded runs.

use crate::{BoundaryCompatible, Capabilities, Feature, FeatureMismatch, Features, Resources};

/// Each feature `required` uses that `provided` states it does not provide,
/// as [`Capabilities`](crate::Capabilities)' fit lists them.
fn fit(required: Features, provided: &Features) -> Result<(), Vec<FeatureMismatch>> {
    let fit = Capabilities::new(Resources::unconstrained(), required).fits(&Capabilities::new(
        Resources::unconstrained(),
        provided.clone(),
    ));
    fit.map(drop).map_err(|mismatch| mismatch.features)
}

/// The mismatch of `feature`, required and not provided.
fn missing(feature: Feature) -> FeatureMismatch {
    FeatureMismatch {
        key: feature,
        required: true,
        provided: false,
    }
}

/// A shot run again alone runs with its run's seed: a part that states
/// replay states seeded runs, and one that states seeded runs `false`
/// states replay `false`, whichever it states last; so replay is not
/// provided over runs that cannot be seeded. A replay builder not stating
/// seeded runs fails the first row; a seeded-runs builder not clearing
/// replay, the second; builders letting an earlier statement win, the
/// last.
#[test]
fn replay_requires_seeded_runs() {
    let replay = || Features::unconstrained().with_replay(true);
    for (row, required, provided, expected) in [
        (
            "replay over runs without seeded runs",
            replay(),
            Features::unconstrained().with_seeded_runs(false),
            Err(vec![missing(Feature::Replay), missing(Feature::SeededRuns)]),
        ),
        (
            "replay, then no seeded runs, stated by a provider",
            replay(),
            replay().with_seeded_runs(false),
            Err(vec![missing(Feature::Replay), missing(Feature::SeededRuns)]),
        ),
        (
            "no seeded runs, then replay, stated by a provider",
            replay(),
            Features::unconstrained()
                .with_seeded_runs(false)
                .with_replay(true),
            Ok(()),
        ),
    ] {
        assert_eq!(fit(required, &provided), expected, "{row}");
    }
}
