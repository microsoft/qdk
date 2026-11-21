// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a driver provides a run: [`RunProvision`].

use super::{Capabilities, CapabilitiesMismatch, Features, Kinds, Resources};

/// What a driver provides the run it drives: the resources and features of
/// a run it serves itself, such as a driver that splits a run into runs of
/// the shots the parts below take. A run states its shots, and whether its
/// seed is fixed, as it starts: the topmost requirement of a chain, which
/// reaches each part below the driver down to the first that serves it,
/// as the crate's `docs/capabilities.md` says.
///
/// Built from [`new`](Self::new), which states nothing, as a driver that
/// serves nothing of a run does, with the builders below.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunProvision(Kinds);

impl RunProvision {
    /// Nothing stated: a run reaches the parts below the driver whole.
    #[must_use]
    pub const fn new() -> Self {
        Self(Kinds::new())
    }

    /// This provision, with `resources` in place of its own, such as the
    /// shots served ([`Resources::with_shots`], with
    /// [`serving`](super::Statements::serving)).
    #[must_use]
    pub fn with_resources(mut self, resources: Resources) -> Self {
        self.0.resources = resources;
        self
    }

    /// This provision, with `features` in place of its own, such as seeded
    /// runs served.
    #[must_use]
    pub fn with_features(mut self, features: Features) -> Self {
        self.0.features = features;
        self
    }

    /// What `self` provides a run over `below`, the provision of the parts
    /// below the driver, by the rule a provision over a provision follows.
    pub(crate) fn over(&self, below: &Capabilities) -> Self {
        Self(self.0.over(&below.kinds))
    }

    /// Whether `self` serves a run of `shots` shots, whose seed is fixed
    /// when `seeded`: what a run states only as it starts.
    // Once per run: kept out of line, so that it does not grow the
    // per-call code of the run it would be inlined into.
    #[inline(never)]
    pub(crate) fn admits_run(
        &self,
        shots: usize,
        seeded: bool,
    ) -> Result<(), CapabilitiesMismatch> {
        // Saturates: no provision can state more than `u32::MAX` shots.
        let shots = u32::try_from(shots).unwrap_or(u32::MAX);
        let run = Kinds {
            resources: Resources::new().with_shots(shots),
            features: Features::new().with_seeded_runs(seeded),
        };
        run.fit(&self.0)
    }
}
