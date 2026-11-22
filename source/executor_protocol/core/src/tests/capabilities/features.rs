// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How features fit: one used and stated lacking is the one mismatch; and
//! replay requires seeded runs.

use crate::tests::fake_parts::capabilities;
use crate::{BoundaryCompatible, FeatureMismatch, Features, Resources};

/// Each feature `required` uses that `provided` states it lacks, as
/// [`Capabilities`](crate::Capabilities)' fit lists them.
fn fit(required: Features, provided: Features) -> Vec<FeatureMismatch> {
    let fit = capabilities(Resources::unconstrained(), required)
        .fits(&capabilities(Resources::unconstrained(), provided));
    fit.map_or_else(|mismatch| mismatch.features, |()| Vec::new())
}

/// A feature required fits unless it is used (`true`) and the provider
/// states it lacking (`false`): a feature a part does not state, it
/// neither uses nor lacks, on either side. Each one lacking is listed, in
/// the fields' order. A fit refusing a feature not used fails "`false`
/// over `true`"; one reading a feature the provider does not state as
/// lacking fails "a feature the provider does not state"; one listing only
/// the first fails "every feature lacking".
#[test]
fn a_used_feature_fits_unless_the_provider_states_it_lacking() {
    let none = Features::unconstrained();
    let replay = |yes| Features {
        replay: Some(yes),
        ..none
    };
    for (row, required, provided, expected) in [
        (
            "a feature the requirer does not state",
            none,
            replay(false),
            vec![],
        ),
        (
            "a feature the provider does not state",
            replay(true),
            none,
            vec![],
        ),
        ("`false` over `true`", replay(false), replay(true), vec![]),
        ("`true` over `true`", replay(true), replay(true), vec![]),
        (
            "`true` over `false`",
            replay(true),
            replay(false),
            vec![FeatureMismatch::Replay],
        ),
        (
            "every feature lacking",
            none.with_replay(true),
            none.with_seeded_runs(false),
            vec![FeatureMismatch::Replay, FeatureMismatch::SeededRuns],
        ),
    ] {
        assert_eq!(fit(required, provided), expected, "{row}");
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
    let both = vec![FeatureMismatch::Replay, FeatureMismatch::SeededRuns];
    for (row, required, provided, expected) in [
        (
            "replay over runs without seeded runs",
            replay(),
            Features::unconstrained().with_seeded_runs(false),
            both.clone(),
        ),
        (
            "replay, then no seeded runs, stated by a provider",
            replay(),
            replay().with_seeded_runs(false),
            both,
        ),
        (
            "no seeded runs, then replay, stated by a provider",
            replay(),
            Features::unconstrained()
                .with_seeded_runs(false)
                .with_replay(true),
            vec![],
        ),
    ] {
        assert_eq!(fit(required, provided), expected, "{row}");
    }
}
