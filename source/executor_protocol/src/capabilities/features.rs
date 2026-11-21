// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a part's shots and runs do, or what a part serves:
//! [`Features`], and each feature required and not provided
//! ([`FeatureMismatch`]).

use std::fmt;

use super::Forwarding;
use crate::BoundaryCompatible;

/// Yes-or-no features: readout parities, resolved per shot as it runs
/// ([`Bit::Parity`](crate::Bit::Parity)); replay, a shot run again alone
/// giving its readouts back; and seeded runs, whose seed is fixed so that
/// they give the same records each time.
///
/// Required, `true` says the part's shots or runs use the feature, and
/// `false` that they do not; provided, `true` says the part serves it, and
/// `false` that it does not. A required feature fits unless the provider
/// states it does not serve it. A feature a part does not state is
/// forwarded, as the crate's `docs/capabilities.md` says.
///
/// Built from [`new`](Self::new), which states nothing, with the builders
/// below.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Features {
    readout_parity: Option<bool>,
    replay: Option<bool>,
    seeded_runs: Option<bool>,
}

impl Features {
    /// No feature stated.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// These features, with readout parities used, or served, as `yes`
    /// says. A checked batch holding one is refused across a boundary
    /// whose required features do not say `true`
    /// ([`ParityNotRequired`](crate::InstructionFault::ParityNotRequired)).
    #[must_use]
    pub fn with_readout_parity(mut self, yes: bool) -> Self {
        self.readout_parity = Some(yes);
        self
    }

    /// These features, with replay used, or served, as `yes` says. A
    /// replayed shot runs with its run's seed, so `true` also states
    /// seeded runs. A part that draws a run's shots together does not
    /// serve replay, as the crate's `docs/writing-a-part.md` says
    /// ("Seeds").
    #[must_use]
    pub fn with_replay(mut self, yes: bool) -> Self {
        self.replay = Some(yes);
        if yes {
            self.seeded_runs = Some(true);
        }
        self
    }

    /// These features, with seeded runs used, or served, as `yes` says; a
    /// quantum computer's measurements cannot be seeded.
    #[must_use]
    pub fn with_seeded_runs(mut self, yes: bool) -> Self {
        self.seeded_runs = Some(yes);
        self
    }

    /// The readout parities stated, if any.
    #[must_use]
    pub fn readout_parity(&self) -> Option<bool> {
        self.readout_parity
    }

    /// The replay stated, if any.
    #[must_use]
    pub fn replay(&self) -> Option<bool> {
        self.replay
    }

    /// The seeded runs stated, if any.
    #[must_use]
    pub fn seeded_runs(&self) -> Option<bool> {
        self.seeded_runs
    }

    /// Each field, paired with its mismatch.
    fn fields(&self) -> [(Option<bool>, FeatureMismatch); 3] {
        [
            (self.readout_parity, FeatureMismatch::ReadoutParity),
            (self.replay, FeatureMismatch::Replay),
            (self.seeded_runs, FeatureMismatch::SeededRuns),
        ]
    }

    /// The features of `fields`, in [`fields`](Self::fields)' order.
    fn from_fields([readout_parity, replay, seeded_runs]: [Option<bool>; 3]) -> Self {
        Self {
            readout_parity,
            replay,
            seeded_runs,
        }
    }

    /// Each field of `self` with `other`'s, combined by `combine`.
    fn zip_with(
        &self,
        other: &Self,
        combine: impl Fn(Option<bool>, Option<bool>) -> Option<bool>,
    ) -> Self {
        let mut theirs = other.fields().into_iter();
        Self::from_fields(self.fields().map(|(mine, _)| {
            let (other, _) = theirs.next().expect("as many fields on each side");
            combine(mine, other)
        }))
    }
}

impl BoundaryCompatible for Features {
    type Fit = ();
    type Mismatch = Vec<FeatureMismatch>;

    /// Every feature `self` requires that `provided` states it does not
    /// serve, in the order readout parity, replay, seeded runs.
    fn fits(&self, provided: &Self) -> Result<(), Vec<FeatureMismatch>> {
        let missing: Vec<_> = (self.fields().into_iter())
            .zip(provided.fields())
            .filter(|((required, _), (provided, _))| {
                *required == Some(true) && *provided == Some(false)
            })
            .map(|((_, mismatch), _)| mismatch)
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}

impl Forwarding for Features {
    fn or(&self, other: &Self) -> Self {
        self.zip_with(other, Option::or)
    }

    fn unanswered(&self, answered: &Self) -> Self {
        self.zip_with(answered, |mine, theirs| mine.filter(|_| theirs.is_none()))
    }

    fn stricter(&self, other: &Self) -> Self {
        self.zip_with(other, |a, b| match (a, b) {
            (Some(a), Some(b)) => Some(a && b),
            (a, b) => a.or(b),
        })
    }

    fn states_nothing(&self) -> bool {
        self.fields().iter().all(|(field, _)| field.is_none())
    }
}

/// A feature required and not provided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FeatureMismatch {
    /// Readout parities are used, and not resolved.
    ReadoutParity,
    /// Shots are run again alone, and do not give their readouts back.
    Replay,
    /// Runs fix their seed, and cannot.
    SeededRuns,
}

impl fmt::Display for FeatureMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FeatureMismatch::ReadoutParity => "uses readout parities, which are not resolved",
            FeatureMismatch::Replay => "runs shots again alone, which are not replayable",
            FeatureMismatch::SeededRuns => "fixes its runs' seed, which cannot be fixed",
        })
    }
}
