// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The setup check's advice: for each step that failed, what to do, in
//! words for the user.
//!
//! The advice about one library comes from its `SetupGuide`, which its
//! module declares; this file adds the advice that holds for every library
//! (for each kind of fault, whether to set an override or install the
//! libraries, and what to install), that of the warning of another copy,
//! and that of the steps after discovery.

use super::devices::{DeviceReport, MINIMUM_COMPUTE_CAPABILITY};
use super::with_sources;
use crate::engine::cuda_algebra::policy::DEVICE_ORDINAL;
use crate::libraries::setup_guide::{SetupGuide, setup_guides};
use crate::{AvailabilityError, CudaAlgebraError, LibraryFault, LibraryReport};
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
                "{}, or set {} to the absolute path of {library}'s file",
                install(),
                guide.variable
            ),
            None => install(),
        },
        LibraryFault::LoadFailed { .. } => {
            let dependencies = match install_command {
                Some(_) => "which brings their dependencies",
                None => "with their dependencies",
            };
            format!(
                "{library} was found but did not load, often because a library it needs is \
                 missing: {}, {dependencies}",
                install()
            )
        }
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

/// What to do about another copy of `library` beside the one loaded,
/// `guide` being the library's. When the one loaded is at fault, it is
/// what to change: the copy an override names is replaced or the override
/// unset; another, replaced by an override or removed. When it is not, it
/// stays loaded, and the other copy, never checked, is what to remove.
pub(super) fn other_copy_advice(guide: &SetupGuide, library: &LibraryReport) -> String {
    let variable = guide.variable;
    match (library.faults.is_empty(), library.loaded_from_override) {
        (false, true) => format!(
            "set {variable} to an audited copy, or unset it, as the one it names is at fault"
        ),
        (false, false) => format!(
            "set {variable} to an audited copy, or remove the one loaded, as it is at fault"
        ),
        (true, _) => "remove the other if nothing else uses it, as it was not checked".to_string(),
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
    "check that `nvidia-smi` lists a GPU, and that the NVIDIA driver is ",
    crate::libraries::cuda_runtime::cuda_12_driver!()
);

/// The guide of the library named `library`.
fn setup_guide(library: &str) -> Option<&'static SetupGuide> {
    setup_guides().iter().find(|guide| guide.library == library)
}
