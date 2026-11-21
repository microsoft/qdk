// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The features a part requires against those the part below provides,
//! at one boundary: which fit.

use crate::prelude::*;
use crate::tests::fake_parts::{Bits, Log, Program, Stating};
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

/// What a part's shots and runs use against what the part below provides:
/// a feature fits unless it is used and the provider states it does not
/// provide it; a provider that does not state a feature does not limit it,
/// and a feature stated not used requires nothing, even of a provider that
/// provides it. A fit refusing any feature both sides state differently
/// fails the rows of a feature not used over one provided.
#[test]
fn a_required_feature_fits_unless_the_provider_states_it_does_not_provide_it() {
    let parity = |yes| Features::unconstrained().with_readout_parity(yes);
    let seeded = |yes| Features::unconstrained().with_seeded_runs(yes);
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
            Features::unconstrained(),
            Ok(()),
        ),
        (
            "readout parities not used, not resolved",
            parity(false),
            parity(false),
            Ok(()),
        ),
        (
            "readout parities not used, resolved",
            parity(false),
            parity(true),
            Ok(()),
        ),
        (
            "readout parities not stated, not resolved",
            Features::unconstrained(),
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
            Features::unconstrained().with_replay(true),
            Features::unconstrained().with_replay(false),
            Err(vec![missing(Feature::Replay)]),
        ),
        (
            "no replay, not replayable",
            Features::unconstrained().with_replay(false),
            Features::unconstrained().with_replay(false),
            Ok(()),
        ),
        (
            "no replay, replayable",
            Features::unconstrained().with_replay(false),
            Features::unconstrained().with_replay(true),
            Ok(()),
        ),
    ] {
        assert_eq!(fit(required, &provided), expected, "{row}");
    }
}

/// A shot run again alone runs with its run's seed: a part that states
/// replay states seeded runs, and one that states runs that cannot be
/// seeded states no replay, whichever it states last; so replay is not
/// provided over runs that cannot be seeded, nor through a part that
/// replays its shots over them, which still requires the seeded runs.
#[test]
fn replay_requires_seeded_runs() {
    let log = Log::default();
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
    let through_a_replayer = Program::new(&[("mz", 0)])
        .with_features(replay())
        .through(Stating::replaying(&log))
        .on(Bits::new(&log).with_features(Features::unconstrained().with_seeded_runs(false)));
    let Err(faults) = through_a_replayer else {
        panic!("replay through a replayer over runs without seeded runs is refused");
    };
    let [fault] = faults.faults() else {
        panic!("one boundary does not fit: {faults:?}");
    };
    assert_eq!(
        fault.mismatch.capabilities.features,
        [missing(Feature::SeededRuns)],
        "{faults:?}"
    );
}
