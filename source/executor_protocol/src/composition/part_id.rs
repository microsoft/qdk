// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Which part of a chain a message is about: its [`PartId`]; and the
//! position the roles' hidden `ready_at` is given, [`ChainPosition`].

use std::fmt;

/// Which part of a chain a message is about, in a composition fault, a
/// refusal or a panic's failure: its position in the chain, and its name.
///
/// The position tells apart two parts of one name, such as two encodings
/// of one type; the name is what the part answers, as each role answers
/// it ([`Driver::name`](crate::Driver::name),
/// [`Adapter::name`](crate::Adapter::name),
/// [`Executor::name`](crate::Executor::name)), when the
/// [`on`](crate::DriverJoins::on) that makes a runner reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PartId {
    /// Its position in the chain, top down: 0 is the driver, then each
    /// adapter, and the executor last, whatever the brackets of the joins.
    /// A message writes it as it is, so `d.through(a).on(e)` has parts 0
    /// to 2.
    pub position: usize,
    /// Its name.
    pub name: String,
}

impl PartId {
    /// The part at `position`, named `name`.
    pub(crate) fn new(position: usize, name: &str) -> Self {
        Self {
            position,
            name: name.to_owned(),
        }
    }
}

/// `` `name` (part 2) ``.
impl fmt::Display for PartId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` (part {})", self.name, self.position)
    }
}

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
