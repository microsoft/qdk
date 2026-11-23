// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a run requires, whatever the chain's instruction sets:
//! [`RunRequirement`].

use crate::{Features, Resources};

/// What a run requires of a chain, as the runner states it: its shots
/// ([`Resources::shots`]), or `u32::MAX` for a run of more, and seeded
/// runs when its seed is fixed
/// ([`Features::seeded_runs`]). It names no instruction set, since a run
/// states nothing of a set's capabilities: the driver at the top gets it
/// ([`DriverLimits::requires`](crate::DriverLimits::requires)) and states what its
/// shots use in the set it sends.
///
/// One run requirement *requires no more than* another as
/// [`Capabilities`](crate::Capabilities) defines it for its resources and
/// features.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunRequirement {
    /// The amounts the run requires, such as its shots.
    pub resources: Resources,
    /// The features the run uses, such as seeded runs.
    pub features: Features,
}

impl RunRequirement {
    /// Nothing required: what [`on`](crate::DriverJoins::on) checks a
    /// chain with, before any run.
    #[must_use]
    pub const fn unconstrained() -> Self {
        Self {
            resources: Resources::unconstrained(),
            features: Features::unconstrained(),
        }
    }
}
