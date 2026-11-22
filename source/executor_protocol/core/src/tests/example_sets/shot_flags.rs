// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ShotFlags`]: the shot conditions of every example set.

use std::ops::{BitOr, BitOrAssign};

/// What a part flagged on a shot, as bits: none by default, joined by `|`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ShotFlags(u8);

impl ShotFlags {
    pub(crate) const NONE: Self = Self(0);
    pub(crate) const REJECTED: Self = Self(1);
    pub(crate) const INCONSISTENT: Self = Self(1 << 1);
    pub(crate) const UNRESOLVED: Self = Self(1 << 2);
}

impl BitOr for ShotFlags {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitOrAssign for ShotFlags {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}
