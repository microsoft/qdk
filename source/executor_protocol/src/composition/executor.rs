// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An executor as users compose it ([`Executor`]): it heads no join, since
//! nothing goes below it.

use super::check;
use crate::{Execute, FitFaults};

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
}
