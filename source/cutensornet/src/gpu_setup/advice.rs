// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The setup check's advice: for each step that failed, what to do, in
//! words for the user.
//!
//! The advice about one library comes from its `SetupGuide`; this file adds
//! the advice that holds for every library (for each kind of fault, whether
//! to set an override or install the libraries, and what to install), that
//! of the warning of another copy, and that of the steps after discovery,
//! which apply the floors the guides declare. An install is advised with a
//! restart (`then_restart` says why).

use super::device_report::DeviceReport;
use super::{TRIAL_RUN_PROBABILITY, is_right_trial_run, with_sources};
use crate::engine::cuda_algebra::policy::DEVICE_ORDINAL;
use crate::libraries::setup_guide::SetupGuide;
use crate::{AvailabilityError, CudaAlgebraError, LibraryFault, LibraryReport, LibrarySource};
use canonical_mps::CanonicalMpsError;

/// What to do about `error`: about its first fault in the order of the
/// libraries step (`gpu_setup`'s header), `guides` being the guide of each
/// library of its report. `install_command` is the packager's, given as an
/// example of the install.
pub(super) fn availability_advice(
    error: &AvailabilityError,
    guides: &[SetupGuide],
    install_command: Option<&str>,
) -> String {
    match error {
        AvailabilityError::UnsupportedPlatform { .. } => {
            "the MPS method's GPU engine runs on Linux x86-64 only".to_string()
        }
        AvailabilityError::Unusable(report) => {
            let faults = || {
                guides
                    .iter()
                    .zip(&report.libraries)
                    .flat_map(|(guide, library)| {
                        library
                            .faults
                            .iter()
                            .map(move |fault| (guide, library, fault))
                    })
            };
            faults()
                .find(|(_, _, fault)| matches!(fault, LibraryFault::InvalidOverride { .. }))
                .or_else(|| faults().next())
                .map_or_else(
                    || install_advice(guides, install_command),
                    |(guide, library, fault)| {
                        fault_advice(guide, library, fault, guides, install_command)
                    },
                )
        }
    }
}

/// What to do about `fault`, of `library`, whose guide is `guide`: change
/// the override when the fault is of the copy it names, since an install
/// does not displace it; else the fix of the fault's kind.
fn fault_advice(
    guide: &SetupGuide,
    library: &LibraryReport,
    fault: &LibraryFault,
    guides: &[SetupGuide],
    install_command: Option<&str>,
) -> String {
    if override_at_fault(library, fault) {
        return change_override_advice(guide.variable, fault);
    }
    let name = guide.library;
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
            #[expect(
                clippy::unnecessary_debug_formatting,
                reason = "paths are quoted, as in the faults, so a line break adds no line"
            )]
            let directories = format!("let this process search the directories on {path:?}");
            format!(
                "{directories}, or set {} to the absolute path of a copy of {name} it can read",
                guide.variable
            )
        }
        LibraryFault::LoadFailed { .. } => {
            let dependencies = match install_command {
                Some(_) => "which brings their dependencies",
                None => "with their dependencies",
            };
            let install = format!(
                "{}, {dependencies}",
                install_libraries(guides, install_command)
            );
            format!(
                "{name} was found but did not load, often because a library it needs is \
                 missing: {}",
                then_restart(&install)
            )
        }
        LibraryFault::MissingRequiredSymbol { .. } | LibraryFault::UnsupportedVersion { .. } => {
            format!("only the audited versions are supported: {}", install())
        }
        LibraryFault::VersionProbeFailed { .. } => (guide.version_probe_advice)()
            .unwrap_or_else(|| format!("{name} could not report its version: {}", install())),
    }
}

/// What to install, then the restart that loads it.
fn install_advice(guides: &[SetupGuide], install_command: Option<&str>) -> String {
    then_restart(&install_libraries(guides, install_command))
}

/// `install`, then the restart of the process that loads what it
/// installs: the system loader returns a library already loaded in the
/// process, so a copy installed since is loaded by a new process only.
fn then_restart(install: &str) -> String {
    format!(
        "{install}, then restart the process (in a notebook, the kernel), because a library \
         already loaded in the process stays loaded"
    )
}

/// What to install: each library of `guides` at its audited version, in
/// order, then, when the packager gave one, `install_command` as an
/// example.
fn install_libraries(guides: &[SetupGuide], install_command: Option<&str>) -> String {
    let libraries: Vec<&str> = guides.iter().map(|guide| guide.install).collect();
    let libraries = libraries.join(" and ");
    match install_command {
        Some(command) => format!("install {libraries}, for instance with `{command}`"),
        None => format!("install {libraries}"),
    }
}

/// What to do about another copy of `library` beside the one loaded,
/// `guide` being the library's: when the one loaded is at fault, change it
/// (or the override that names it); else remove the other, which was never
/// checked.
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
/// the override is what to change.
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
/// A copy that did not load may be sound and miss a library it needs, so
/// it is not said to be at fault.
fn change_override_advice(variable: &str, fault: &LibraryFault) -> String {
    let why = match fault {
        LibraryFault::LoadFailed { .. } => "the one it names, or a library it needs, did not load",
        _ => "the one it names is at fault",
    };
    format!("set {variable} to an audited copy, or unset it, as {why}")
}

/// What to do about the devices step's outcome: `None` when the GPU
/// engine's device was read and is of the compute capability each library
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
        }) => {
            let (floor, library) = guides
                .iter()
                .filter_map(|guide| Some((guide.minimum_compute_capability?, guide.library)))
                .max_by_key(|(floor, _)| *floor)?;
            (device.compute_capability < floor).then(|| {
                let (major, minor) = device.compute_capability;
                let (needed_major, needed_minor) = floor;
                format!(
                    "CUDA device {DEVICE_ORDINAL}, where the GPU engine runs, has compute \
                     capability {major}.{minor}; {library} needs {needed_major}.{needed_minor} \
                     or later"
                )
            })
        }
    }
}

/// What to do about the trial run's outcome, reached once the libraries
/// and the device passed their checks: `None` when it read the right
/// probability. A run that found no device gets the devices step's
/// advice; a run that failed, its error with its causes, which those
/// checks did not foresee; a wrong probability, a reinstall of the
/// libraries of `guides`, then a check of the device.
pub(super) fn trial_run_advice(
    outcome: &Result<f64, CanonicalMpsError<CudaAlgebraError>>,
    guides: &[SetupGuide],
    install_command: Option<&str>,
) -> Option<String> {
    match outcome {
        Ok(probability) if is_right_trial_run(*probability) => None,
        Ok(probability) => Some(format!(
            "the trial run on CUDA device {DEVICE_ORDINAL} read the probability {probability} \
             of One, where {TRIAL_RUN_PROBABILITY} is exact, so the libraries or the device \
             compute wrong results: {}; if the result stays wrong, check the device with \
             `nvidia-smi`",
            install_advice(guides, install_command)
        )),
        Err(CanonicalMpsError::Algebra(CudaAlgebraError::NoDevice)) => {
            Some(no_device_advice(guides))
        }
        Err(error) => Some(format!(
            "the trial run on CUDA device {DEVICE_ORDINAL} failed, though the libraries and the \
             device passed their checks: {}",
            with_sources(error)
        )),
    }
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
