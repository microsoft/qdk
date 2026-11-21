// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The features a part requires against those the part below provides,
//! at one boundary: which fit.

use super::coin;
use crate::{BoundaryCompatible, Feature, FeatureMismatch, Features, Resources};

/// Each feature `required` uses that `provided` states it does not serve,
/// as [`Capabilities`](crate::Capabilities)' fit lists them.
fn fit(required: Features, provided: &Features) -> Result<(), Vec<FeatureMismatch>> {
    let fit = coin(Resources::new(), required).fits(&coin(Resources::new(), provided.clone()));
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

/// What a part's shots and runs use against what the part below serves:
/// a feature fits unless it is used and the provider states it does not
/// serve it; a provider that does not state a feature serves it.
#[test]
fn a_required_feature_fits_unless_the_provider_states_it_does_not_serve_it() {
    let parity = |yes| Features::new().with_readout_parity(yes);
    let seeded = |yes| Features::new().with_seeded_runs(yes);
    for (row, required, provided, expected) in [
        (
            "readout parities used, not resolved",
            parity(true),
            parity(false),
            Err(vec![missing(Feature::ReadoutParity)]),
        ),
        (
            "readout parities used, resolved",
            parity(true),
            parity(true),
            Ok(()),
        ),
        (
            "readout parities used, not stated below",
            parity(true),
            Features::new(),
            Ok(()),
        ),
        (
            "readout parities not used, not resolved",
            parity(false),
            parity(false),
            Ok(()),
        ),
        (
            "readout parities not stated, not resolved",
            Features::new(),
            parity(false),
            Ok(()),
        ),
        (
            "seeded runs, not provided",
            seeded(true),
            seeded(false),
            Err(vec![missing(Feature::SeededRuns)]),
        ),
        (
            "replay, not replayable",
            Features::new().with_replay(true),
            Features::new().with_replay(false),
            Err(vec![missing(Feature::Replay)]),
        ),
        (
            "no replay, not replayable",
            Features::new().with_replay(false),
            Features::new().with_replay(false),
            Ok(()),
        ),
    ] {
        assert_eq!(fit(required, &provided), expected, "{row}");
    }
}

/// A shot run again alone runs with its run's seed: a part that states
/// replay states seeded runs, and one that states runs that cannot be
/// seeded states no replay, whichever it states last; so replay is not
/// served over runs that cannot be seeded.
#[test]
fn replay_requires_seeded_runs() {
    let replay = || Features::new().with_replay(true);
    for (row, required, provided, expected) in [
        (
            "replay over runs without seeded runs",
            replay(),
            Features::new().with_seeded_runs(false),
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
            Features::new().with_seeded_runs(false).with_replay(true),
            Ok(()),
        ),
    ] {
        assert_eq!(fit(required, &provided), expected, "{row}");
    }
}
