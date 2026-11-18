// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The setup check's advice: for each step that failed, what to do, in
//! words for the user.
//!
//! The advice about one library comes from its `SetupGuide`, which its
//! module declares; this file adds the advice that holds for every library
//! (how to set an override, what to install, and when) and that of the
//! steps after discovery.

use super::devices::{DeviceReport, MINIMUM_COMPUTE_CAPABILITY};
use super::with_sources;
use crate::engine::cuda_algebra::policy::DEVICE_ORDINAL;
use crate::libraries::setup_guide::{SetupGuide, setup_guides};
use crate::{AvailabilityError, CudaAlgebraError, LibraryFault};
use canonical_mps::CanonicalMpsError;

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

/// The guide of the library named `library`.
fn setup_guide(library: &str) -> Option<&'static SetupGuide> {
    setup_guides().iter().find(|guide| guide.library == library)
}
