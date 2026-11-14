// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A part of a chain as the admission's faults name it: [`ChainPart`].

use std::fmt;

/// A part of a chain as the admission's faults name it: its position and
/// its name.
///
/// The position tells apart two parts of one name, such as two encodings
/// of one type; the name is what the part says
/// ([`Execute::name`](crate::Execute::name)).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChainPart {
    /// Its position in the chain, top down: 0 is the driver, then each
    /// adapter, and the executor last, whatever the brackets of the `>>`s.
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
