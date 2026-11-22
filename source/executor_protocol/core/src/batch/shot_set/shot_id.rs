// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot's name ([`ShotId`]): the number its caller gave it.

use std::fmt;

/// A shot's name: the number its caller gave it.
///
/// A shot's records depend on its id, not on its place in a set: each
/// replayable part's stream for the shot comes from the id
/// ([`Seed::shot`](crate::Seed::shot)), so a shot can be rerun alone. Parts
/// key what they hold for each shot by it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ShotId(u32);

impl ShotId {
    /// The shot numbered `id`.
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// The shot's number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// The shot's number, `7`, as messages write it after "shot"; a width, a
/// fill and an alignment format it as they format the number, so that a
/// table of shots lines up.
impl fmt::Display for ShotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}
