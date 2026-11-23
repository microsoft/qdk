// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The yes-or-no features every chain states, whatever its instruction
//! set: [`Features`], and each one required and not provided
//! ([`FeatureMismatch`]).

use std::fmt;

/// Yes-or-no features: replay, a shot run again alone giving its readouts
/// back; and seeded runs, whose seed is fixed so that they give the same
/// records each time.
///
/// Required, `Some(true)` says the shots or runs use the feature, and
/// `Some(false)` that they do not; provided, `Some(false)` says the part
/// cannot take shots or runs that use it, and `Some(true)` that it can. A
/// part that does not state one (`None`) neither needs nor limits it.
///
/// Built from [`nothing_stated`](Self::nothing_stated), which states
/// nothing, with the builders below, which keep replay and seeded runs
/// consistent, or by setting its fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Features {
    /// A shot run again alone, giving its readouts back.
    pub replay: Option<bool>,
    /// A run whose seed is fixed.
    pub seeded_runs: Option<bool>,
}

impl Features {
    /// No feature stated: required, none used; provided, none said to be
    /// lacking. What a part leaves unstated, composition cannot refuse a
    /// chain on: a part that uses replay or seeded runs is joined with
    /// this one all the same.
    #[must_use]
    pub const fn nothing_stated() -> Self {
        Self {
            replay: None,
            seeded_runs: None,
        }
    }

    /// These features, with replay used, or taken, as `yes` says. A
    /// replayed shot runs with its run's seed, so `true` also states
    /// seeded runs. A part that draws a run's shots together does not
    /// take replay, as the crate's `docs/writing-a-part.md` says
    /// ("Seeds").
    #[must_use]
    pub const fn with_replay(self, yes: bool) -> Self {
        Self {
            replay: Some(yes),
            seeded_runs: if yes { Some(true) } else { self.seeded_runs },
        }
    }

    /// These features, with seeded runs used, or taken, as `yes` says; a
    /// quantum computer's measurements cannot be seeded. A shot is replayed
    /// with its run's seed, so `false` also states replay `false`: no
    /// builder states replay without seeded runs.
    #[must_use]
    pub const fn with_seeded_runs(self, yes: bool) -> Self {
        Self {
            replay: if yes { self.replay } else { Some(false) },
            seeded_runs: Some(yes),
        }
    }

    /// Each feature `self` requires and `provided` states it lacks, in the
    /// fields' order.
    pub(crate) fn mismatches(self, provided: Self) -> Vec<FeatureMismatch> {
        let lacks = |required: Option<bool>, provided: Option<bool>| {
            required == Some(true) && provided == Some(false)
        };
        let replay = lacks(self.replay, provided.replay).then_some(FeatureMismatch::Replay);
        let seeded =
            (lacks(self.seeded_runs, provided.seeded_runs)).then_some(FeatureMismatch::SeededRuns);
        replay.into_iter().chain(seeded).collect()
    }
}

/// A feature required and not provided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FeatureMismatch {
    /// Replay required, and not provided.
    Replay,
    /// Seeded runs required, and not provided.
    SeededRuns,
}

/// `seeded runs required, not provided`.
impl fmt::Display for FeatureMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self {
            FeatureMismatch::Replay => "replay",
            FeatureMismatch::SeededRuns => "seeded runs",
        };
        write!(f, "{what} required, not provided")
    }
}
