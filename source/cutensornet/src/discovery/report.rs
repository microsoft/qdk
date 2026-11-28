// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What discovery found: one report per library, in the order they are
//! opened.
//!
//! A caller reads the report to tell the user what is installed, or what to
//! fix. An [`Availability`](crate::Availability) carries it when every
//! library is usable, and
//! [`AvailabilityError::Unusable`](crate::AvailabilityError::Unusable) when
//! one is not. Each entry is a [`LibraryReport`].

use crate::{LibraryFault, LibraryReport};

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
        self.libraries.iter().find(|library| library.name == name)
    }

    /// Every fault discovery found, each with the report of its library.
    /// Libraries come in the order they are opened, and each library's
    /// faults in the order found. A caller telling the user what to fix
    /// reads them here.
    pub fn faults(&self) -> impl Iterator<Item = (&LibraryReport, &LibraryFault)> {
        self.libraries
            .iter()
            .flat_map(|library| library.faults.iter().map(move |fault| (library, fault)))
    }
}

#[cfg(test)]
mod tests;
