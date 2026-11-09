// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Finding the native libraries a GPU run needs on the host, today the CUDA
//! Runtime and cuTensorNet, and checking that the run can use them.
//!
//! A caller preparing a GPU run calls [`discover`]. It gets an
//! [`Availability`], which keeps the libraries loaded with the table of each
//! one's functions, or an [`AvailabilityError`] saying everything that is
//! wrong, so that it can tell the user or run on the CPU instead. An
//! `Availability` can be relied on: every required function of each table
//! was found, and each library is the audited version. Discovery selects no
//! device, allocates nothing and runs no GPU work.
//!
//! An `Availability`, and the error when a library is at fault
//! ([`AvailabilityError::Unusable`]), carry an [`AvailabilityReport`], with
//! one [`LibraryReport`](crate::LibraryReport) per library: where it was
//! loaded from, the versions it reports, and its faults. The report is in
//! `report`, and each library's entry, made where the library is opened, in
//! `libraries::report`; the error, and the message it shows, in `error`.
//!
//! Discovery names no library: `libraries::LoadedLibraries` lists the
//! libraries a GPU run needs, in the order they are opened, and each
//! library's module says where it is looked for, which functions it must
//! export and which versions are accepted. Each is opened and checked the
//! same way (`libraries::open`), even when one before it is at fault, so the
//! report lists every fault found; a failed discovery closes the libraries
//! it opened.

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
/// their tables. It reserves no GPU and no memory. It is `Send` and `Sync`,
/// so one discovery serves every thread.
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

    /// The loaded libraries, for the engines of this crate. They stay
    /// private to it: a caller outside sees their paths and versions in
    /// [`report`](Self::report), and calls them only through an engine,
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
/// It selects no device, allocates nothing and runs no GPU work. Each
/// library has an override variable, today `QDK_CUDART_LIBRARY` and
/// `QDK_CUTENSORNET_LIBRARY`, which, when set, names the one absolute path
/// to load; otherwise the library's default paths are tried in order
/// (README, "Where the libraries are looked for"). Each library is checked
/// whatever the faults of the others.
///
/// # Errors
///
/// [`AvailabilityError::UnsupportedPlatform`] off Linux x86-64. Otherwise
/// [`AvailabilityError::Unusable`] when a library is at fault: its override
/// is not an absolute path to a file, it is not found or fails to load, it
/// lacks required functions, a version probe fails, or a version is not the
/// audited one. Its report lists every fault of every library.
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
pub(crate) fn discover_from(
    read_variable: impl Fn(&'static str) -> Option<OsString>,
) -> Result<Availability, AvailabilityError> {
    availability_of(InstalledLibraries(read_variable))
}

/// Discovery, with `checker` opening and checking each library: an
/// [`Availability`] when every library is usable, or else the report of
/// every library, the usable ones included.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) fn availability_of<C: CheckLibrary>(
    checker: C,
) -> Result<Availability, AvailabilityError> {
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
