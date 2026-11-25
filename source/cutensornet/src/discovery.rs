// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Finding the native libraries a GPU run needs on the host, today the CUDA
//! Runtime and cuTensorNet, and checking that the run can use them.
//!
//! A caller preparing a GPU run calls [`discover`], or [`discover_with`]
//! when a package manager installed the libraries where no default path
//! reaches. It gets an [`Availability`], which keeps the libraries loaded
//! with the table of each one's functions. Or it gets an
//! [`AvailabilityError`] saying everything that is wrong, so that it can
//! tell the user or run on the CPU instead. An `Availability` can be
//! relied on: every required function of each table was found, and each
//! library is the audited version.
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

use crate::libraries::PackagedLibraries;
use crate::libraries::setup_guide::SetupGuide;
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
        allow(
            dead_code,
            reason = "off Linux x86-64 nothing in the crate calls the libraries"
        )
    )]
    libraries: std::sync::Arc<LoadedLibraries>,
}

impl Availability {
    /// The paths and versions [`discover`] or [`discover_with`] found.
    #[must_use]
    pub fn report(&self) -> &AvailabilityReport {
        &self.report
    }

    /// The loaded libraries, for the CUDA algebra and the setup check. They
    /// stay private to the crate. A caller outside sees their paths and
    /// versions in [`report`](Self::report), and reaches them only through
    /// the crate's public types.
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
/// CUDA Runtime and cuTensorNet, as [`discover_with`] does with no packaged
/// libraries.
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
    discover_with(&PackagedLibraries::default())
}

/// Finds, loads and checks the native libraries a GPU run needs, trying
/// each library's file in `packaged` as well as its override and default
/// paths, in the order `docs/loading.md` gives ("The steps of discovery").
/// So a user whose package manager installed the libraries where no
/// default path reaches, as NVIDIA's Python wheels do, needs no
/// `LD_LIBRARY_PATH`.
///
/// # Errors
///
/// As [`discover`].
pub fn discover_with(packaged: &PackagedLibraries) -> Result<Availability, AvailabilityError> {
    discover_with_guides(packaged).availability
}

/// What [`discover_with`] finds, with the setup guide of each library
/// opened. Off Linux x86-64, no library is opened, so there is no guide.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) fn discover_with_guides(packaged: &PackagedLibraries) -> Discovered {
    discover_from(std::env::var_os, packaged)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) fn discover_with_guides(_packaged: &PackagedLibraries) -> Discovered {
    let unsupported = AvailabilityError::UnsupportedPlatform {
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    };
    Discovered {
        availability: Err(unsupported),
        guides: Vec::new(),
    }
}

/// What discovery found, with the setup guide of each library it opened.
/// The setup check shows those libraries and advises about them.
pub(crate) struct Discovered {
    /// The libraries, loaded and checked, or why they cannot be used.
    pub(crate) availability: Result<Availability, AvailabilityError>,
    /// The setup guide of each library opened, in the order opened.
    pub(crate) guides: Vec<SetupGuide>,
}

/// Discovery, with the override variables read by `read_variable` and the
/// packaged files `packaged`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) fn discover_from(
    read_variable: impl Fn(&'static str) -> Option<OsString>,
    packaged: &PackagedLibraries,
) -> Discovered {
    availability_of(InstalledLibraries {
        read_variable,
        packaged,
    })
}

/// Discovery, with `checker` opening and checking each library: an
/// [`Availability`] when every library is usable, or else the report of
/// every library, the usable ones included.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn availability_of<C: CheckLibrary>(checker: C) -> Discovered {
    let mut opening = Opening::new(checker);
    let libraries = LoadedLibraries::open(&mut opening);
    let (reports, guides) = opening.into_parts();
    let report = AvailabilityReport { libraries: reports };
    let availability = match libraries {
        Some(libraries) => Ok(Availability {
            report,
            libraries: std::sync::Arc::new(libraries),
        }),
        None => Err(AvailabilityError::Unusable(report)),
    };
    Discovered {
        availability,
        guides,
    }
}

#[cfg(test)]
mod tests;
