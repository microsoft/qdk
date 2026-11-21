// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The features a part requires against those the part below provides,
//! at one boundary: which fit.

use crate::{BoundaryCompatible, Feature, FeatureMismatch, Features};

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
    for (row, required, provided, fit) in [
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
            "seeded runs, not seedable",
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
        assert_eq!(required.fits(&provided), fit, "{row}");
    }
}

/// A shot run again alone runs with its run's seed: replay required also
/// requires seeded runs, so a provider that replays its shots, but whose
/// runs cannot be seeded, does not serve it.
#[test]
fn replay_requires_seeded_runs() {
    let required = Features::new().with_replay(true);
    let provided = Features::new().with_replay(true).with_seeded_runs(false);
    assert_eq!(
        required.fits(&provided),
        Err(vec![missing(Feature::SeededRuns)])
    );
}

/// Every feature not served is listed at once, in the order readout
/// parity, replay, seeded runs.
#[test]
fn every_feature_not_served_is_listed_in_order() {
    let required = Features::new()
        .with_seeded_runs(true)
        .with_replay(true)
        .with_readout_parity(true);
    let provided = Features::new()
        .with_readout_parity(false)
        .with_replay(false)
        .with_seeded_runs(false);
    assert_eq!(
        required.fits(&provided),
        Err(vec![
            missing(Feature::ReadoutParity),
            missing(Feature::Replay),
            missing(Feature::SeededRuns),
        ])
    );
}

/// A feature's message names it, for a user who reads which feature to
/// drop or which part to change.
#[test]
fn a_feature_not_served_names_the_feature() {
    for (missing, named) in [
        (missing(Feature::ReadoutParity), "readout parit"),
        (missing(Feature::Replay), "replay"),
        (missing(Feature::SeededRuns), "seed"),
    ] {
        let message = missing.to_string();
        assert!(
            message.contains(named),
            "{missing:?} names {named}: {message}"
        );
    }
}
