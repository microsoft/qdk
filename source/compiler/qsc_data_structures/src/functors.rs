// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::display::join;
use std::{
    fmt::{self, Display, Formatter},
    iter,
};

/// A functor application.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct FunctorApp {
    /// An invocation is either adjoint or not, with each successive use of `Adjoint` functor switching
    /// between the two, so a bool is sufficient to track.
    pub adjoint: bool,

    /// An invocation can have multiple `Controlled` functors with each one adding another layer of updates
    /// to the argument tuple, so the functor application must be tracked with a count.
    pub controlled: u8,
}

impl Display for FunctorApp {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let controlleds = std::iter::repeat_n("Controlled", self.controlled.into());
        let adjoint = iter::once("Adjoint").filter(|_| self.adjoint);
        join(f, controlleds.chain(adjoint), " ")
    }
}
