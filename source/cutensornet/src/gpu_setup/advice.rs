// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The setup check's advice: for each step that failed, what to do, in
//! words for the user.
//!
//! The advice about one library comes from its `SetupGuide`. This file adds
//! the advice that holds for every library: for each kind of fault, whether
//! to set an override or install the libraries, and what to install. It
//! also holds the advice on another copy, and that of the steps after
//! discovery, which apply the floors the guides declare. An install is
//! advised with a restart (`then_restart` says why).

use super::device_report::{DeviceReport, EngineDevice};
use super::{is_right_trial_run, with_sources, wrong_probability};
use crate::engine::cuda_algebra::settings::DEVICE_ORDINAL;
use crate::libraries::setup_guide::{ComputeCapability, SetupGuide};
use crate::{
    AvailabilityError, AvailabilityReport, CudaAlgebraError, LibraryFault, LibraryReport,
    LibrarySource,
};
use canonical_mps::CanonicalMpsError;
use std::path::Path;

/// What to do about the first fault of `error`, in the order of the
/// libraries step ([`GpuSetup`](super::GpuSetup)'s steps). `guides` holds
/// the guide of each library of its report. `install_command` is the
/// packager's, given as an example of the install.
pub(super) fn availability_advice(
    error: &AvailabilityError,
    guides: &[SetupGuide],
    install_command: Option<&str>,
) -> String {
    match error {
        AvailabilityError::UnsupportedPlatform { .. } => {
            "the MPS method's GPU engine runs on Linux x86-64 only".to_string()
        }
        AvailabilityError::Unusable(report) => first_fault_to_advise(report).map_or_else(
            || install_advice(guides, install_command),
            |(library, fault)| fault_advice(library, fault, guides, install_command),
        ),
    }
}

/// The fault of `report` advised first: an invalid override, since an
/// override decides what is loaded; otherwise the first fault, in the order
/// the libraries were opened.
fn first_fault_to_advise(report: &AvailabilityReport) -> Option<(&LibraryReport, &LibraryFault)> {
    report
        .faults()
        .find(|(_, fault)| matches!(fault, LibraryFault::InvalidOverride { .. }))
        .or_else(|| report.faults().next())
}

/// What to do about `fault` of `library`, whose guide is in `guides`. When
/// the fault is of the copy the override names, change the override, since
/// an install does not displace it. Otherwise, the fix of the fault's kind.
fn fault_advice(
    library: &LibraryReport,
    fault: &LibraryFault,
    guides: &[SetupGuide],
    install_command: Option<&str>,
) -> String {
    let guide = SetupGuide::for_library(guides, library);
    if override_at_fault(library, fault) {
        return change_override_advice(guide.variable, fault);
    }
    let name = library.name;
    let install = || install_advice(guides, install_command);
    match fault {
        LibraryFault::InvalidOverride { variable, .. } => {
            format!("set {variable} to the absolute path of the library file, or unset it")
        }
        LibraryFault::NotFound { .. } => format!(
            "{}; or set {} to the absolute path of {name}'s file",
            install(),
            guide.variable
        ),
        LibraryFault::InaccessiblePath { path, .. } => {
            inaccessible_path_advice(name, path, guide.variable)
        }
        LibraryFault::LoadFailed { .. } => load_failed_advice(name, guides, install_command),
        LibraryFault::MissingRequiredSymbol { .. } | LibraryFault::UnsupportedVersion { .. } => {
            format!("only the audited versions are supported: {}", install())
        }
        LibraryFault::VersionProbeFailed { .. } => (guide.version_probe_advice)()
            .unwrap_or_else(|| format!("{name} could not report its version: {}", install())),
    }
}

/// What to do when the process cannot access `path`, tried for the library
/// `name`, whose override is `variable`.
fn inaccessible_path_advice(name: &str, path: &Path, variable: &str) -> String {
    let reach = format!(
        "let this process reach `{}` (the permissions of its directories, or a link that loops)",
        path.display()
    );
    format!("{reach}, or set {variable} to the absolute path of a copy of {name} it can read")
}

/// What to do when the library `name` was found but did not load: install
/// the libraries of `guides` with their dependencies.
fn load_failed_advice(name: &str, guides: &[SetupGuide], install_command: Option<&str>) -> String {
    let dependencies = match install_command {
        Some(_) => "which brings their dependencies",
        None => "with their dependencies",
    };
    let install = format!(
        "{}, {dependencies}",
        install_libraries(guides, install_command)
    );
    format!(
        "{name} was found but did not load, often because a library it needs is missing: {}",
        then_restart(&install)
    )
}

/// What to install, then the restart that loads it.
fn install_advice(guides: &[SetupGuide], install_command: Option<&str>) -> String {
    then_restart(&install_libraries(guides, install_command))
}

/// `install`, then a restart of the process. The system loader returns a
/// library already loaded in the process, so only a new process loads a
/// copy installed since.
fn then_restart(install: &str) -> String {
    format!(
        "{install}, then restart the process (in a notebook, the kernel), because a library \
         already loaded in the process stays loaded"
    )
}

/// What to install: each library of `guides` at its audited version, in
/// order. Then `install_command` as an example, when the packager gave one.
fn install_libraries(guides: &[SetupGuide], install_command: Option<&str>) -> String {
    let libraries: Vec<&str> = guides.iter().map(|guide| guide.install).collect();
    let libraries = libraries.join(" and ");
    match install_command {
        Some(command) => format!("install {libraries}, for instance with `{command}`"),
        None => format!("install {libraries}"),
    }
}

/// What to do about another copy of `library` beside the one loaded,
/// `guide` being the library's. When the one loaded is at fault, change it
/// (or the override that names it). Otherwise remove the other, which was
/// never checked.
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

/// Whether `fault` of `library` is of the copy its override names. Then the
/// override is what to change.
fn override_at_fault(library: &LibraryReport, fault: &LibraryFault) -> bool {
    library.source == Some(LibrarySource::Override) && is_of_the_copy(fault)
}

/// Whether `fault` is of the copy tried. A failed version probe is not: its
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

/// What to do when `fault` is of the copy the override `variable` names.
/// A copy that did not load is not said to be at fault: the loader also
/// refuses a sound copy when a library it needs is missing.
fn change_override_advice(variable: &str, fault: &LibraryFault) -> String {
    let why = match fault {
        LibraryFault::LoadFailed { .. } => "the one it names, or a library it needs, did not load",
        _ => "the one it names is at fault",
    };
    format!("set {variable} to an audited copy, or unset it, as {why}")
}

/// What to do about the devices step's outcome. `None` when the GPU
/// engine's device was read and has the compute capability each library
/// of `guides` needs.
pub(super) fn device_advice(
    devices: &Result<DeviceReport, CudaAlgebraError>,
    guides: &[SetupGuide],
) -> Option<String> {
    match devices {
        Err(_) => Some(format!(
            "the CUDA devices could not be read: {}",
            driver_advice(guides)
        )),
        Ok(DeviceReport {
            engine_device: None,
            ..
        }) => Some(no_device_advice(guides)),
        Ok(DeviceReport {
            engine_device: Some(device),
            ..
        }) => compute_capability_advice(device, guides),
    }
}

/// What to do when `device`, the GPU engine's, is older than the most
/// demanding compute capability floor of `guides`. `None` when it meets
/// every floor.
fn compute_capability_advice(device: &EngineDevice, guides: &[SetupGuide]) -> Option<String> {
    let (floor, library) = most_demanding_compute_capability(guides)?;
    (device.compute_capability < floor).then(|| {
        format!(
            "CUDA device {DEVICE_ORDINAL}, where the GPU engine runs, has compute capability \
             {}; {library} needs {floor} or later",
            device.compute_capability
        )
    })
}

/// The highest compute capability floor of `guides`, with the library that
/// sets it, which the advice names. `None` when no library sets one.
fn most_demanding_compute_capability(
    guides: &[SetupGuide],
) -> Option<(ComputeCapability, &'static str)> {
    guides
        .iter()
        .filter_map(|guide| Some((guide.minimum_compute_capability?, guide.library)))
        .max_by_key(|(floor, _)| *floor)
}

/// What to do about the trial run's outcome, reached once the libraries
/// and the device passed their checks. `None` when it read the right
/// probability. Otherwise:
///
/// - a run that found no device gets the devices step's advice;
/// - a run that failed gets its error with its causes, which those checks
///   did not foresee;
/// - a wrong probability gets a reinstall of the libraries of `guides`,
///   then a check of the device.
pub(super) fn trial_run_advice(
    outcome: &Result<f64, CanonicalMpsError<CudaAlgebraError>>,
    guides: &[SetupGuide],
    install_command: Option<&str>,
) -> Option<String> {
    match outcome {
        Ok(probability) if is_right_trial_run(*probability) => None,
        Ok(probability) => Some(wrong_result_advice(*probability, guides, install_command)),
        Err(CanonicalMpsError::Algebra(CudaAlgebraError::NoDevice)) => {
            Some(no_device_advice(guides))
        }
        Err(error) => Some(failed_trial_run_advice(error)),
    }
}

/// What to do when the trial run read `probability`, a wrong one: the
/// libraries or the device compute wrong results, so reinstall the
/// libraries of `guides`, then check the device.
fn wrong_result_advice(
    probability: f64,
    guides: &[SetupGuide],
    install_command: Option<&str>,
) -> String {
    format!(
        "the trial run on CUDA device {DEVICE_ORDINAL} {}, so the libraries or the device \
         compute wrong results: {}; if the result stays wrong, check the device with \
         `nvidia-smi`",
        wrong_probability(probability),
        install_advice(guides, install_command)
    )
}

/// What to do when the trial run failed with `error`, which the checks of
/// the libraries and the device did not foresee: its error, with its
/// causes, is all the advice there is.
fn failed_trial_run_advice(error: &CanonicalMpsError<CudaAlgebraError>) -> String {
    format!(
        "the trial run on CUDA device {DEVICE_ORDINAL} failed, though the libraries and the \
         device passed their checks: {}",
        with_sources(error)
    )
}

/// What to do when the process sees no device for the GPU engine.
fn no_device_advice(guides: &[SetupGuide]) -> String {
    format!("no CUDA device was found: {}", driver_advice(guides))
}

/// What to check on the host when no device can be used: that the driver
/// sees a GPU, and is as recent as the most demanding library of `guides`
/// needs.
fn driver_advice(guides: &[SetupGuide]) -> String {
    let lists_a_gpu = "check that `nvidia-smi` lists a GPU";
    match guides.iter().filter_map(|guide| guide.minimum_driver).max() {
        Some(floor) => format!("{lists_a_gpu}, and that the NVIDIA driver is {floor}"),
        None => lists_a_gpu.to_string(),
    }
}
