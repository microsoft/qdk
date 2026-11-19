// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The setup check's advice: for each step that failed, what to do, in
//! words for the user.
//!
//! The advice about one library comes from its `SetupGuide`, which its
//! module declares as its `LibrarySetupGuide`; this file adds the advice
//! that holds for every library (for each kind of fault, whether to set an
//! override or install the libraries, and what to install), that of the
//! warning of another copy, and that of the steps after discovery. An
//! install is followed by a restart of the process: a library already
//! loaded in the process stays loaded, so a copy installed since is loaded
//! by a new process only.

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
                    .flat_map(|library| library.faults.iter().map(move |fault| (library, fault)))
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

/// What to do about `fault`, of `library`: about its override when the
/// fault is of the copy it names, since an install does not displace it;
/// else by the fault.
fn fault_advice(
    library: &LibraryReport,
    fault: &LibraryFault,
    install_command: Option<&str>,
) -> String {
    let guide = setup_guide(library.library);
    if let Some(guide) = guide.filter(|_| override_at_fault(library, fault)) {
        return change_override_advice(guide.variable, fault);
    }
    let library = library.library;
    let install = || install_advice(install_command);
    match fault {
        LibraryFault::InvalidOverride { variable, .. } => {
            format!("set {variable} to the absolute path of the library file, or unset it")
        }
        LibraryFault::NotFound { .. } => match guide {
            Some(guide) => format!(
                "{}; or set {} to the absolute path of {library}'s file",
                install(),
                guide.variable
            ),
            None => install(),
        },
        LibraryFault::InaccessiblePath { path, .. } => {
            #[expect(
                clippy::unnecessary_debug_formatting,
                reason = "paths are quoted, as in the faults, so a line break adds no line"
            )]
            let directories = format!("let this process search the directories on {path:?}");
            match guide {
                Some(guide) => format!(
                    "{directories}, or set {} to the absolute path of a copy of {library} it \
                     can read",
                    guide.variable
                ),
                None => directories,
            }
        }
        LibraryFault::LoadFailed { .. } => {
            let dependencies = match install_command {
                Some(_) => "which brings their dependencies",
                None => "with their dependencies",
            };
            let install = format!("{}, {dependencies}", install_libraries(install_command));
            format!(
                "{library} was found but did not load, often because a library it needs is \
                 missing: {}",
                then_restart(&install)
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

/// What to install, then the restart that loads it.
fn install_advice(install_command: Option<&str>) -> String {
    then_restart(&install_libraries(install_command))
}

/// `install`, then the restart of the process that loads what it installs
/// (this file's header says why).
fn then_restart(install: &str) -> String {
    format!(
        "{install}, then restart the process (in a notebook, the kernel), because a library \
         already loaded in the process stays loaded"
    )
}

/// What to install: each library at its audited version, in the order the
/// check lists them, then, when the packager gave one, `install_command`
/// as an example.
fn install_libraries(install_command: Option<&str>) -> String {
    let libraries: Vec<&str> = setup_guides().iter().map(|guide| guide.install).collect();
    let libraries = libraries.join(" and ");
    match install_command {
        Some(command) => format!("install {libraries}, for instance with `{command}`"),
        None => format!("install {libraries}"),
    }
}

/// What to do about another copy of `library` beside the one loaded,
/// `guide` being the library's. Only a library loaded lists other copies
/// (`LibraryReport::other_copies`), so the faults met here are a missing
/// function, a version not supported, or a failed version probe. When the
/// one loaded is at fault (a function or its version), it is what to
/// change: the copy an override names is replaced or the override unset;
/// another, replaced by an override or removed. When it is not (it passed
/// its checks, or only a version probe failed), it stays loaded, and the
/// other copy, never checked, is what to remove.
pub(super) fn other_copy_advice(guide: &SetupGuide, library: &LibraryReport) -> String {
    let variable = guide.variable;
    match library.faults.iter().find(|fault| is_of_the_copy(fault)) {
        Some(fault) if override_at_fault(library, fault) => change_override_advice(variable, fault),
        Some(_) => format!(
            "set {variable} to an audited copy, or remove the one loaded if nothing else uses \
             it, as it is at fault"
        ),
        None => "remove the other if nothing else uses it, as it was not checked".to_string(),
    }
}

/// Whether `fault`, of `library`, is of the copy its override names: then
/// the override is what to change, for the advice and the warnings alike.
fn override_at_fault(library: &LibraryReport, fault: &LibraryFault) -> bool {
    library.from_override && is_of_the_copy(fault)
}

/// Whether `fault` is of the copy tried: its load, a function or its
/// version. A failed version probe is not taken as the copy's: its
/// library's probe advice says what to do (for the CUDA Runtime, usually
/// update the driver).
fn is_of_the_copy(fault: &LibraryFault) -> bool {
    matches!(
        fault,
        LibraryFault::LoadFailed { .. }
            | LibraryFault::MissingRequiredSymbol { .. }
            | LibraryFault::UnsupportedVersion { .. }
    )
}

/// What to do when `fault` is of the copy the override `variable` names. A
/// copy that did not load may be sound and miss a library it needs, so it
/// is not said to be at fault; only the main advice meets that fault, as a
/// copy that did not load lists no other copies.
fn change_override_advice(variable: &str, fault: &LibraryFault) -> String {
    let why = match fault {
        LibraryFault::LoadFailed { .. } => "the one it names, or a library it needs, did not load",
        _ => "the one it names is at fault",
    };
    format!("set {variable} to an audited copy, or unset it, as {why}")
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
