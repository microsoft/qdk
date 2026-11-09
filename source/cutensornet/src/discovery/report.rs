// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What discovery found: one report per library, in the order they are
//! opened.
//!
//! A caller reads the report to tell the user what is installed, or what to
//! fix. An [`Availability`](crate::Availability) carries it when every
//! library is usable, and
//! [`AvailabilityError::Unusable`](crate::AvailabilityError::Unusable) when
//! one is not. It is a list with one entry per library, a
//! [`LibraryReport`], so binding another library adds an entry and changes
//! no type. The entries are made where each library is opened and checked,
//! so their types are defined with the libraries (`libraries::report`).

use crate::LibraryReport;

/// What [`discover`](crate::discover) found: one [`LibraryReport`] per
/// library, in the order they are opened (today the CUDA Runtime, then
/// cuTensorNet).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailabilityReport {
    /// One report per library.
    pub libraries: Vec<LibraryReport>,
}

impl AvailabilityReport {
    /// The report of the library named `name` (for instance
    /// `"cuTensorNet"`), if discovery checks it.
    #[must_use]
    pub fn library(&self, name: &str) -> Option<&LibraryReport> {
        self.libraries
            .iter()
            .find(|library| library.library == name)
    }
}
