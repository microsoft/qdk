// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What shots and runs use, or what a part can take:
//! [`Features`], a yes-or-no per [`Feature`].

use std::fmt;

use super::{StatementMismatch, Statements};

/// Yes-or-no features: readout parities, resolved per shot as it runs
/// ([`Bit::Parity`](crate::Bit::Parity)); replay, a shot run again alone
/// giving its readouts back; and seeded runs, whose seed is fixed so that
/// they give the same records each time.
///
/// Required, `true` says the shots or runs use the feature, and `false`
/// that they do not; provided, `false` says the part cannot take shots or
/// runs that use it, and `true` that it can.
///
/// Built from [`unconstrained`](Self::unconstrained), which states
/// nothing, with the builders below.
pub type Features = Statements<Feature, bool>;

/// A feature required and not provided: `required` is `true`, and
/// `provided` `false`.
pub type FeatureMismatch = StatementMismatch<Feature, bool>;

/// A yes-or-no feature, in the order mismatches are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Feature {
    /// Readout parities, resolved per shot.
    ReadoutParity,
    /// A shot run again alone, giving its readouts back.
    Replay,
    /// A run whose seed is fixed.
    SeededRuns,
}

impl Features {
    /// No feature stated: required, none used; provided, none said to be
    /// lacking. What a part leaves unstated, composition cannot refuse a
    /// chain on: a part that uses readout parities, replay or seeded runs
    /// is joined with this one all the same.
    #[must_use]
    pub const fn unconstrained() -> Self {
        Self::new()
    }

    /// These features, with readout parities used, or taken, as `yes`
    /// says. A checked batch holding one is refused across a boundary
    /// whose required features do not say `true`
    /// ([`ParityNotRequired`](crate::InstructionFault::ParityNotRequired)).
    #[must_use]
    pub fn with_readout_parity(self, yes: bool) -> Self {
        self.with(Feature::ReadoutParity, yes)
    }

    /// These features, with replay used, or taken, as `yes` says. A
    /// replayed shot runs with its run's seed, so `true` also states
    /// seeded runs. A part that draws a run's shots together does not
    /// take replay, as the crate's `docs/writing-a-part.md` says
    /// ("Seeds").
    #[must_use]
    pub fn with_replay(self, yes: bool) -> Self {
        let replay = self.with(Feature::Replay, yes);
        if yes {
            replay.with_seeded_runs(true)
        } else {
            replay
        }
    }

    /// These features, with seeded runs used, or taken, as `yes` says; a
    /// quantum computer's measurements cannot be seeded. A shot is replayed
    /// with its run's seed, so `false` also states no replay: no builder
    /// states replay without seeded runs.
    #[must_use]
    pub fn with_seeded_runs(self, yes: bool) -> Self {
        let seeded = self.with(Feature::SeededRuns, yes);
        if yes {
            seeded
        } else {
            seeded.with(Feature::Replay, false)
        }
    }
}

/// `seeded runs required, not provided`.
impl fmt::Display for FeatureMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} required, not provided", self.key)
    }
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Feature::ReadoutParity => "readout parity",
            Feature::Replay => "replay",
            Feature::SeededRuns => "seeded runs",
        })
    }
}
