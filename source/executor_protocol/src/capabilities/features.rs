// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a part's shots and runs use, or what a part serves:
//! [`Features`], a yes-or-no per [`Feature`].

use std::fmt;

use super::{StatementMismatch, Statements};

/// Yes-or-no features: readout parities, resolved per shot as it runs
/// ([`Bit::Parity`](crate::Bit::Parity)); replay, a shot run again alone
/// giving its readouts back; and seeded runs, whose seed is fixed so that
/// they give the same records each time.
///
/// Required, `true` says the part's shots or runs use the feature, and
/// `false` that they do not; provided, `true` says the part serves it, and
/// `false` that it does not.
///
/// Built from [`new`](Statements::new), which states nothing, with the
/// builders below.
pub type Features = Statements<Feature, bool>;

/// A feature required and not provided: `required` is `true`, and
/// `provided` `false`.
pub type FeatureMismatch = StatementMismatch<Feature, bool>;

/// A yes-or-no feature, in the order mismatches are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Feature {
    /// Readout parities, resolved per shot.
    ReadoutParity,
    /// A shot run again alone, giving its readouts back.
    Replay,
    /// A run whose seed is fixed.
    SeededRuns,
}

impl Features {
    /// These features, with readout parities used, or served, as `yes`
    /// says. A checked batch holding one is refused across a boundary
    /// whose required features do not say `true`
    /// ([`ParityNotRequired`](crate::InstructionFault::ParityNotRequired)).
    #[must_use]
    pub fn with_readout_parity(self, yes: bool) -> Self {
        self.with(Feature::ReadoutParity, yes)
    }

    /// These features, with replay used, or served, as `yes` says. A
    /// replayed shot runs with its run's seed, so `true` also states
    /// seeded runs. A part that draws a run's shots together does not
    /// serve replay, as the crate's `docs/writing-a-part.md` says
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

    /// These features, with seeded runs used, or served, as `yes` says; a
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

    /// These features, provided, with no replay where they state no seeded
    /// runs: a shot is replayed with its run's seed, so a provision that
    /// combines a part's replay with a lack of seeded runs below it serves
    /// neither. It never states seeded runs that are not stated. No check
    /// needs it, since every replay requirement states seeded runs too: it
    /// keeps the provision a part above reads, and the mismatches it meets,
    /// free of a replay that cannot be served.
    pub(crate) fn replay_only_if_seeded(self) -> Self {
        if self.get(&Feature::SeededRuns) == Some(false) {
            self.with(Feature::Replay, false)
        } else {
            self
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
