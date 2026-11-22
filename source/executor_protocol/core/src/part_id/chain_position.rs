// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The position the roles' hidden `ready_at` is given: [`ChainPosition`],
//! which names a part by its [`PartId`] in the runner's list.

use crate::PartId;

/// A part's position in its chain, as [`PartId::position`] counts it, with
/// the chain's parts as the [`on`](crate::DriverJoins::on) that made the
/// runner named them, if known, given to the roles' hidden `ready_at`, so
/// that a joined part names a refusal or a panic of a part it holds by
/// that part's position and the name `on` read. A joined part readied by
/// its role's `ready`, inside a part of the user's, does not know it: its
/// refusals and panics pass up unnamed, for the part above to name.
///
/// It is public, because the roles' hidden hook names it, but no public
/// path reaches it, so that no other crate can call or override the hook.
#[derive(Clone, Copy, Debug)]
pub struct ChainPosition<'a>(Option<(usize, &'a [PartId])>);

impl<'a> ChainPosition<'a> {
    /// A position not known.
    pub(crate) const UNKNOWN: ChainPosition<'static> = ChainPosition(None);

    /// The position `position` in the chain of `parts`, top to bottom.
    pub(crate) fn at(position: usize, parts: &'a [PartId]) -> Self {
        Self(Some((position, parts)))
    }

    /// The position `parts` parts below this one.
    pub(crate) fn below(self, parts: usize) -> Self {
        Self(self.0.map(|(position, ids)| (position + parts, ids)))
    }

    /// The part at this position, if known.
    pub(crate) fn part(self) -> Option<&'a PartId> {
        self.0.map(|(position, parts)| &parts[position])
    }
}
