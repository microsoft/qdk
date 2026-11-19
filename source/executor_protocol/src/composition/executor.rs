// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An executor as users compose it ([`Executor`]): it heads no join, since
//! nothing goes below it.

use super::check;
use crate::{Execute, FitFaults, InstructionSet, Offer};

/// An executor, ready to compose with `>>`.
pub struct Executor<E: Execute> {
    pub(super) part: E,
    pub(super) fit: Vec<FitFaults>,
}

impl<E: Execute> Executor<E> {
    /// The executor `part`, ready to compose.
    pub fn new(part: E) -> Self {
        Self {
            part,
            fit: Vec::new(),
        }
    }

    /// As [`Driver::check`](crate::Driver::check).
    pub fn check(&self) -> Result<(), &[FitFaults]> {
        check(&self.fit)
    }

    /// The instructions it accepts: its top part's, which the part above it
    /// is fitted to. The process that holds this executor reads it for a
    /// remote executor, one that stands for this executor in another
    /// process, to accept what this executor does.
    pub fn accepts(&self) -> &InstructionSet {
        self.part.accepts()
    }

    /// Its offer, as it reaches the part above it: its bottom part's,
    /// adapted by each adapter it holds, the lowest first. The process that
    /// holds this executor reads it, as it reads
    /// [`accepts`](Self::accepts), for a remote executor to offer what this
    /// executor does.
    pub fn offers(&self) -> Offer {
        self.part.offers()
    }
}
