// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The words of the setup check: the advice for each step that failed, the
//! install it advises, and how the report shows a device's memory and an
//! error's causes.
//!
//! The words about one library come from its `SetupGuide`, which its
//! module declares; this file adds those that hold for every library (how
//! to set an override, what to install, and when) and those of the steps
//! after discovery.

use super::devices::{DeviceReport, MINIMUM_COMPUTE_CAPABILITY};
use crate::engine::cuda_algebra::policy::DEVICE_ORDINAL;
use crate::libraries::setup_guide::SetupGuide;
use crate::{AvailabilityError, CudaAlgebraError, LibraryFault};
use canonical_mps::CanonicalMpsError;
use std::error::Error;

/// What to do about `error`: when libraries are at fault, about an invalid
/// override if any, since an override decides what is loaded; or else
/// about the first fault of the first library at fault, in the order they
/// are opened. `install_command` is the packager's, given as an example of
/// the install.
pub(super) fn availability_advice(
    error: &AvailabilityError,
    install_command: Option<&str>,
) -> String {
    match error {
        AvailabilityError::UnsupportedPlatform { .. } => {
            "the MPS method's GPU engine runs on Linux x86-64 only".to_string()
        }
        AvailabilityError::Unusable(report) => {
            let faults = || {
                report
                    .libraries
                    .iter()
                    .flat_map(|library| library.faults.iter().map(|fault| (library.library, fault)))
            };
            faults()
                .find(|(_, fault)| matches!(fault, LibraryFault::InvalidOverride { .. }))
                .or_else(|| faults().next())
                .map_or_else(
                    || install_advice(install_command),
                    |(library, fault)| fault_advice(library, fault, install_command),
                )
        }
    }
}

/// What to do about `fault`, of the library named `library`.
fn fault_advice(library: &str, fault: &LibraryFault, install_command: Option<&str>) -> String {
    let guide = setup_guide(library);
    let install = || install_advice(install_command);
    match fault {
        LibraryFault::InvalidOverride { variable, .. } => {
            format!("set {variable} to the absolute path of the library file, or unset it")
        }
        LibraryFault::NotFound { .. } => match guide {
            Some(guide) => format!(
                "{}, or set {} to the absolute path of {library}",
                install(),
                guide.variable
            ),
            None => install(),
        },
        LibraryFault::LoadFailed { .. } => format!(
            "{library} was found but did not load, often because a library it needs is \
             missing: {}, which brings its dependencies",
            install()
        ),
        LibraryFault::MissingRequiredSymbol { .. } | LibraryFault::UnsupportedVersion { .. } => {
            format!("only the audited versions are supported: {}", install())
        }
        LibraryFault::VersionProbeFailed { .. } => {
            match guide.and_then(|guide| guide.version_probe_advice) {
                Some(advice) => advice.to_string(),
                None => format!("{library} could not report its version: {}", install()),
            }
        }
    }
}

/// What to install: each library at its audited version, in the order the
/// check lists them, then, when the packager gave one, `install_command`
/// as an example.
fn install_advice(install_command: Option<&str>) -> String {
    let libraries: Vec<&str> = setup_guides().iter().map(|guide| guide.install).collect();
    let libraries = libraries.join(" and ");
    match install_command {
        Some(command) => format!("install {libraries}, for instance with `{command}`"),
        None => format!("install {libraries}"),
    }
}

/// What to do about the devices step's outcome: `None` when the GPU
/// engine's device was read and cuTensorNet supports it.
pub(super) fn device_advice(devices: &Result<DeviceReport, CudaAlgebraError>) -> Option<String> {
    match devices {
        Err(_) => Some(format!("the CUDA devices could not be read: {DRIVER}")),
        Ok(DeviceReport {
            engine_device: None,
            ..
        }) => Some(no_device_advice()),
        Ok(DeviceReport {
            engine_device: Some(device),
            ..
        }) if !device.is_supported() => {
            let (major, minor) = device.compute_capability;
            let (needed_major, needed_minor) = MINIMUM_COMPUTE_CAPABILITY;
            Some(format!(
                "CUDA device {DEVICE_ORDINAL}, where the GPU engine runs, has compute capability \
                 {major}.{minor}; cuTensorNet needs {needed_major}.{needed_minor} or later"
            ))
        }
        Ok(_) => None,
    }
}

/// What to do about a failed trial run, reached once the libraries and the
/// device passed their checks: the advice of the devices step when the run
/// found no device, else the run's error, with its causes, which those
/// checks did not foresee.
pub(super) fn trial_run_advice(error: &CanonicalMpsError<CudaAlgebraError>) -> String {
    match error {
        CanonicalMpsError::Algebra(CudaAlgebraError::NoDevice) => no_device_advice(),
        error => format!(
            "the trial run on CUDA device {DEVICE_ORDINAL} failed, though the libraries and the \
             device passed their checks: {}",
            with_sources(error)
        ),
    }
}

/// What to do when the process sees no device for the GPU engine.
fn no_device_advice() -> String {
    format!("no CUDA device was found: {DRIVER}")
}

/// What to check on the host when no device can be used: that the driver
/// sees a GPU, and is recent enough for CUDA 12.
const DRIVER: &str = concat!(
    "check that `nvidia-smi` lists a GPU, and that the NVIDIA driver supports CUDA 12 (",
    crate::libraries::cuda_runtime::minimum_driver!(),
    " or later)"
);

/// `error`, then each error it was caused by, separated by `: `, so a
/// wrapping error does not hide its cause: the engine's error says only
/// that its algebra failed.
pub(super) fn with_sources(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

/// `bytes` in GiB, to one decimal.
#[allow(clippy::cast_precision_loss)] // A device's memory, shown to 0.1 GiB.
pub(super) fn gibibytes(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / f64::from(1u32 << 30))
}

/// The guide of each library, in the order the check lists them; none off
/// Linux x86-64, where discovery fails before any library is looked at.
pub(super) fn setup_guides() -> &'static [SetupGuide] {
    #[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
    {
        &crate::libraries::LoadedLibraries::SETUP_GUIDES
    }
    #[cfg(not(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64"))))]
    {
        &[]
    }
}

/// The guide of the library named `library`.
fn setup_guide(library: &str) -> Option<&'static SetupGuide> {
    setup_guides().iter().find(|guide| guide.library == library)
}
