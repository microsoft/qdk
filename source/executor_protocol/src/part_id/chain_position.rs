// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The position the roles' hidden `ready_at` is given: [`ChainPosition`],
//! which names a part by the [`PartId`] it builds.

use crate::PartId;

/// A part's position in its chain, as [`PartId::position`] counts it, if
/// known, given to the roles' hidden `ready_at`, so that a joined part
/// names a refusal of a part it holds by that part's position. A joined
/// part readied by its role's `ready`, inside a part of the user's, does
/// not know it: its refusals pass up unnamed, for the part above to name.
///
/// It is public, because the roles' hidden hook names it, but no public
/// path reaches it, so that no other crate can call or override the hook.
#[derive(Clone, Copy, Debug)]
pub struct ChainPosition(Option<usize>);

impl ChainPosition {
    /// A position not known.
    pub(crate) const UNKNOWN: Self = Self(None);

    /// The position `position`.
    pub(crate) fn at(position: usize) -> Self {
        Self(Some(position))
    }

    /// The position `parts` parts below this one.
    pub(crate) fn below(self, parts: usize) -> Self {
        Self(self.0.map(|position| position + parts))
    }

    /// The part at this position, named `name`, if the position is known.
    pub(crate) fn part(self, name: &str) -> Option<PartId> {
        self.0.map(|position| PartId::new(position, name))
    }
}
