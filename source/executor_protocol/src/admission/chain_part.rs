// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A part of a chain as the protocol's messages name it: [`ChainPart`].

use std::fmt;

/// A part of a chain as the protocol's messages name it, in an admission
/// fault or a panic's failure: its position and its name.
///
/// The position tells apart two parts of one name, such as two encodings
/// of one type; the name is what the part answers, as each role answers
/// it ([`Drive::name`](crate::Drive::name),
/// [`Adapt::name`](crate::Adapt::name),
/// [`Execute::name`](crate::Execute::name)), when the `>>` that makes a
/// runner reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChainPart {
    /// Its position in the chain, top down: 0 is the driver, then each
    /// adapter, and the executor last, whatever the brackets of the `>>`s.
    /// A message writes it as it is, so `d >> a >> e` has parts 0 to 2.
    pub position: usize,
    /// Its name.
    pub name: String,
}

impl ChainPart {
    /// The part at `position`, named `name`.
    pub(crate) fn new(position: usize, name: &str) -> Self {
        Self {
            position,
            name: name.to_owned(),
        }
    }
}

/// `` `name` (part 2) ``.
impl fmt::Display for ChainPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` (part {})", self.name, self.position)
    }
}
