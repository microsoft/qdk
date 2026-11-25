// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Finding the native libraries a GPU run needs on the host, today the CUDA
//! Runtime and cuTensorNet, and checking that the run can use them.
//!
//! A caller preparing a GPU run calls [`discover`]. It gets an
//! [`Availability`], which keeps the libraries loaded with the table of each
//! one's functions. Or it gets an [`AvailabilityError`] saying everything
//! that is wrong, so that it can tell the user or run on the CPU instead.
//! An `Availability` can be relied on: every required function of each
//! table was found, and each library is the audited version.
//!
//! An `Availability`, and the error when a library is at fault
//! ([`AvailabilityError::Unusable`]), carry an [`AvailabilityReport`].
//!
//! Discovery names no library: `libraries::LoadedLibraries` lists them.
//! Each library is opened and checked even when one before it is at fault,
//! so the report lists every fault found. A failed discovery closes the
//! libraries it opened.

pub(crate) mod error;
pub(crate) mod report;

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use crate::libraries::{CheckLibrary, InstalledLibraries, LoadedLibraries, Opening};
use crate::{AvailabilityError, AvailabilityReport};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use std::ffi::OsString;
use std::fmt;

/// The loaded and checked libraries, with their [`AvailabilityReport`].
///
/// Holding it keeps the libraries loaded, and with them every function of
/// their tables. It is `Send` and `Sync`, so one discovery serves every
/// thread.
pub struct Availability {
    report: AvailabilityReport,
    #[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
    #[cfg_attr(
        not(all(target_os = "linux", target_arch = "x86_64")),
        allow(dead_code, reason = "off Linux x86-64 no engine reads the libraries")
    )]
    libraries: std::sync::Arc<LoadedLibraries>,
}

impl Availability {
    /// The paths and versions [`discover`] found.
    #[must_use]
    pub fn report(&self) -> &AvailabilityReport {
        &self.report
    }

    /// The loaded libraries, for the CUDA algebra. They stay private to
    /// the crate. A caller outside sees their paths and versions in
    /// [`report`](Self::report), and calls them only through the engine,
    /// which checks each call against its contract.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    pub(crate) fn libraries(&self) -> &std::sync::Arc<LoadedLibraries> {
        &self.libraries
    }
}

impl fmt::Debug for Availability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Availability")
            .field("report", &self.report)
            .finish_non_exhaustive()
    }
}

/// Finds, loads and checks the native libraries a GPU run needs, today the
/// CUDA Runtime and cuTensorNet.
///
/// It selects no device, allocates no GPU memory and runs no GPU work.
/// Each library has an override variable, today `QDK_CUDART_LIBRARY` and
/// `QDK_CUTENSORNET_LIBRARY`. When set, it names the one absolute path to
/// load. Otherwise the library's default paths are tried in order (README,
/// "Where the libraries are looked for"). Each library is checked whatever
/// the faults of the others.
///
/// # Errors
///
/// [`AvailabilityError::UnsupportedPlatform`] off Linux x86-64. Otherwise
/// [`AvailabilityError::Unusable`] when a library is at fault;
/// [`LibraryFault`](crate::LibraryFault) lists the faults. Its report lists
/// every fault of every library.
pub fn discover() -> Result<Availability, AvailabilityError> {
    discover_on_this_platform()
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn discover_on_this_platform() -> Result<Availability, AvailabilityError> {
    discover_from(std::env::var_os)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn discover_on_this_platform() -> Result<Availability, AvailabilityError> {
    Err(AvailabilityError::UnsupportedPlatform {
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    })
}

/// Discovery, with the override variables read by `read_variable`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn discover_from(
    read_variable: impl Fn(&'static str) -> Option<OsString>,
) -> Result<Availability, AvailabilityError> {
    availability_of(InstalledLibraries(read_variable))
}

/// Discovery, with `checker` opening and checking each library: an
/// [`Availability`] when every library is usable, or else the report of
/// every library, the usable ones included.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn availability_of<C: CheckLibrary>(checker: C) -> Result<Availability, AvailabilityError> {
    let mut opening = Opening::new(checker);
    let libraries = LoadedLibraries::open(&mut opening);
    let report = AvailabilityReport {
        libraries: opening.into_reports(),
    };
    match libraries {
        Some(libraries) => Ok(Availability {
            report,
            libraries: std::sync::Arc::new(libraries),
        }),
        None => Err(AvailabilityError::Unusable(report)),
    }
}

#[cfg(test)]
mod tests;
