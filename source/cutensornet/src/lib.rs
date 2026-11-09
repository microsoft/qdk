// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Finding and loading NVIDIA cuTensorNet and the CUDA Runtime at run time.
//!
//! This crate is a step towards running the QDK's MPS simulator on an
//! NVIDIA GPU through cuTensorNet, NVIDIA's tensor-network library, which
//! runs on the CUDA Runtime. Most users have neither, so the QDK must build,
//! install and run without them. The crate therefore never links them:
//! [`discover`] finds and opens the two shared libraries installed on the
//! host, when a GPU run is asked for, looks up the functions of a table per
//! library, and checks the libraries' versions against the audited ones. It
//! returns an [`Availability`] that keeps the libraries loaded, or an
//! [`AvailabilityError`] saying what is missing, so a caller can report it
//! or run on the CPU instead. Discovery runs no GPU work. The MPS engine,
//! added in a later change, calls cuTensorNet through these tables.
//!
//! The crate's `README.md` shows the steps of discovery and the error each
//! one can return.

#[allow(
    clippy::mod_module_files,
    reason = "the generated cuTensorNet bindings and the audited CUDA Runtime ones sit side by side under bindings/"
)]
mod bindings;
mod error;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(
        dead_code,
        reason = "off Linux x86-64 the loader is compiled for its tests only"
    )
)]
mod library;
#[cfg(test)]
mod tests;
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
mod version;

pub use error::AvailabilityError;

use std::{fmt, path::PathBuf};

/// What [`discover`] found: the libraries' paths and the versions they report.
///
/// cuTensorNet encodes a version as `10000·major + 100·minor + patch`
/// (`21300` is 2.13.0); CUDA as `1000·major + 10·minor` (`12090` is 12.9).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailabilityReport {
    /// The cuTensorNet library that was loaded.
    pub cutensornet_library: PathBuf,
    /// The CUDA Runtime library that was loaded.
    pub cuda_runtime_library: PathBuf,
    /// cuTensorNet's version (`cutensornetGetVersion`).
    pub cutensornet_version: usize,
    /// The CUDA Runtime version cuTensorNet was built against
    /// (`cutensornetGetCudartVersion`).
    pub cutensornet_cuda_runtime_version: usize,
    /// The loaded CUDA Runtime's version (`cudaRuntimeGetVersion`).
    pub cuda_runtime_version: i32,
    /// The CUDA driver's version (`cudaDriverGetVersion`), for diagnostics
    /// only.
    pub cuda_driver_version: i32,
}

/// The loaded and checked libraries, with their [`AvailabilityReport`].
///
/// Holding it keeps the libraries loaded, and with them every function of
/// their tables. It reserves no GPU and no memory. It is `Send` and `Sync`,
/// so one discovery serves every thread.
pub struct Availability {
    report: AvailabilityReport,
    #[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
    #[expect(
        dead_code,
        reason = "keeps the libraries loaded; the MPS engine, added later, reads the function tables"
    )]
    libraries: std::sync::Arc<library::LoadedLibraries>,
}

impl Availability {
    /// The paths and versions [`discover`] found.
    #[must_use]
    pub fn report(&self) -> &AvailabilityReport {
        &self.report
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

/// Finds, loads and checks the CUDA Runtime and cuTensorNet.
///
/// It selects no device, allocates nothing and runs no GPU work.
/// `QDK_CUTENSORNET_LIBRARY` and `QDK_CUDART_LIBRARY`, when set, name the
/// one absolute path to load for each library; otherwise a fixed list of
/// default paths is tried in order (README, "Where the libraries are
/// looked for").
///
/// # Errors
///
/// Off Linux x86-64; when an override is not an absolute path to a file;
/// when a library is not found, fails to load or lacks a required function;
/// when a version probe fails; or when a version is not the audited one.
/// Each case is an [`AvailabilityError`] variant.
pub fn discover() -> Result<Availability, AvailabilityError> {
    discover_on_this_platform()
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn discover_on_this_platform() -> Result<Availability, AvailabilityError> {
    library::discover(std::env::var_os)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn discover_on_this_platform() -> Result<Availability, AvailabilityError> {
    Err(AvailabilityError::UnsupportedPlatform {
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    })
}

/// The path an override variable names, if it is set.
///
/// An override is checked before anything is loaded: it must be an absolute
/// path to a file, or to a symlink to one, so that a mistyped variable fails
/// with its name instead of loading another library.
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
fn validate_override_path(
    variable: &'static str,
    value: Option<std::ffi::OsString>,
) -> Result<Option<PathBuf>, AvailabilityError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(AvailabilityError::InvalidOverride {
            variable,
            path,
            reason: "path must be absolute",
        });
    }
    let metadata = std::fs::metadata(&path).map_err(|_| AvailabilityError::InvalidOverride {
        variable,
        path: path.clone(),
        reason: "path does not exist",
    })?;
    if !metadata.is_file() {
        return Err(AvailabilityError::InvalidOverride {
            variable,
            path,
            reason: "path must name a file",
        });
    }
    Ok(Some(path))
}
