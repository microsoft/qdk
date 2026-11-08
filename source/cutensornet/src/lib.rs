// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running the QDK's matrix product state (MPS) simulation on an NVIDIA GPU,
//! through cuTensorNet, NVIDIA's tensor-network library, without making
//! NVIDIA software part of the QDK's build.
//!
//! A simulation method written against `tensornet::MpsEngine` (gates as
//! matrices on sites; probabilities, projections, forks, samples and
//! expectations back) runs on any engine. This crate gives it one on the
//! GPU, in two halves:
//!
//! - **Finding and loading.** cuTensorNet runs on the CUDA Runtime, and most
//!   users have neither, so the QDK must build, install and run without
//!   them: the crate never links them. [`discover`] finds and opens the two
//!   shared libraries installed on the host, when a GPU run is asked for,
//!   looks up the functions of a table per library, and checks the
//!   libraries' versions against the audited ones. It returns an
//!   [`Availability`] that keeps the libraries loaded, or an
//!   [`AvailabilityError`] saying what is missing, so a caller can report it
//!   or run on the CPU instead. Discovery runs no GPU work.
//! - **The MPS engine.** The availability's `mps_engine` (Linux x86-64)
//!   opens a [`CuTensorNetEngine`], the `tensornet::MpsEngine` on the loaded
//!   libraries: it holds each state as a cuTensorNet state, computed only
//!   when a read needs it, and refuses a call that breaks the contract as
//!   every engine does ([`EngineError`]).
//!
//! A user setting the GPU up reads [`diagnose`]'s [`Diagnosis`]: what was
//! found, whether an engine runs on the device, and what to fix. A Python
//! environment's NVIDIA wheels are found by the caller and passed to
//! [`discover_with`] ([`PackagedLibraries`]).
//!
//! The crate's `README.md` shows the steps of discovery and the error each
//! one can return, and how the engine calls cuTensorNet.

#[allow(
    clippy::mod_module_files,
    reason = "the generated cuTensorNet bindings and the audited CUDA Runtime ones sit side by side under bindings/"
)]
mod bindings;
#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(
        dead_code,
        reason = "off Linux x86-64 no CUDA Runtime reads the devices; the rules stay compiled so their host tests run"
    )
)]
mod devices;
mod diagnosis;
#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(
        dead_code,
        reason = "off Linux x86-64 nothing calls the native API; the engine stays compiled so its host tests run"
    )
)]
mod engine;
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

pub use devices::{DeviceReport, MINIMUM_COMPUTE_CAPABILITY};
pub use diagnosis::{Diagnosis, diagnose};
pub use engine::{CuTensorNetEngine, CuTensorNetState, EngineError};
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
    /// Other copies of cuTensorNet the setup makes visible, not loaded: a
    /// wheel's beside an override or a system install, say. Two copies may
    /// be of different versions, and a dependency of the one loaded may be
    /// found beside the other, so the setup check warns of them.
    pub other_cutensornet_copies: Vec<PathBuf>,
    /// Other copies of the CUDA Runtime the setup makes visible, not loaded.
    pub other_cuda_runtime_copies: Vec<PathBuf>,
}

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
    libraries: std::sync::Arc<library::LoadedLibraries>,
}

impl Availability {
    /// The paths and versions [`discover`] found.
    #[must_use]
    pub fn report(&self) -> &AvailabilityReport {
        &self.report
    }

    /// Every CUDA device, with its compute capability and memory, in
    /// ordinal order: what the setup check reports.
    ///
    /// # Errors
    ///
    /// When the CUDA Runtime fails to count or read the devices (with no
    /// device or no driver, say).
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    pub fn devices(&self) -> Result<Vec<DeviceReport>, EngineError> {
        devices::device_reports(self.libraries.as_ref())
    }

    /// An MPS engine on CUDA device 0. A split discards at most 1e-14 of
    /// its cut's squared norm beyond the capacity's bonds, with GESVD and
    /// the simple-update gauge; each state restarts from its computed MPS
    /// every 10 computes, on a cuTensorNet handle of its own, which keeps
    /// the scratch a compute asks for bounded over a long shot. It keeps
    /// the loaded libraries alive for as long as it lives.
    ///
    /// # Errors
    ///
    /// Fails if no CUDA device is available, or if creating the device's
    /// stream or cuTensorNet handle fails.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    pub fn mps_engine(&self) -> Result<CuTensorNetEngine, EngineError> {
        CuTensorNetEngine::with_calls(self.libraries.clone())
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

/// Where Python packages installed the two libraries, for [`discover_with`]:
/// NVIDIA's wheels (`cutensornet-cu12`, `nvidia-cuda-runtime-cu12`) put them
/// inside a Python environment, where only the QDK's Python package, which
/// reads the environment's installed distributions, can find them. A path
/// is absolute; `None` when the package is not installed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackagedLibraries {
    /// `libcutensornet.so.2` as a package installed it.
    pub cutensornet: Option<PathBuf>,
    /// `libcudart.so.12` as a package installed it.
    pub cuda_runtime: Option<PathBuf>,
}

/// Finds, loads and checks the CUDA Runtime and cuTensorNet, as
/// [`discover_with`] does with no packaged libraries.
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
    discover_with(&PackagedLibraries::default())
}

/// Finds, loads and checks the CUDA Runtime and cuTensorNet, trying for each
/// library its override, alone, when set; or else its path in `packaged`,
/// then the default paths (README, "Where the libraries are looked for").
/// So a user who installed NVIDIA's wheels in a Python environment needs no
/// `LD_LIBRARY_PATH`, and a set override still decides.
///
/// # Errors
///
/// As [`discover`].
pub fn discover_with(packaged: &PackagedLibraries) -> Result<Availability, AvailabilityError> {
    discover_on_this_platform(packaged)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn discover_on_this_platform(
    packaged: &PackagedLibraries,
) -> Result<Availability, AvailabilityError> {
    library::discover(std::env::var_os, packaged)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn discover_on_this_platform(
    _packaged: &PackagedLibraries,
) -> Result<Availability, AvailabilityError> {
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
