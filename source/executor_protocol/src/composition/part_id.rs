// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Which part of a chain a message is about: its [`PartId`].

use std::fmt;

/// Which part of a chain a message is about, in an admission fault or a
/// panic's failure: its position in the chain, and its name.
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
