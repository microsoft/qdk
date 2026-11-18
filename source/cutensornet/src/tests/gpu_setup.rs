// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`GpuSetup`]: that a user reads what was found, whether the setup is
//! ready, and for each kind of problem, the fix that applies to it.

mod devices;

#[cfg(unix)]
use std::path::PathBuf;

use crate::{AvailabilityError, GpuSetup};
#[cfg(unix)]
use crate::{
    AvailabilityReport, CudaAlgebraError, LibraryFault, LibraryReport, Version,
    gpu_setup::devices::DeviceReport,
};
#[cfg(unix)]
use canonical_mps::CanonicalMpsError;

/// The install every library fault is advised, without a packager's
/// command.
const INSTALL: &str = "install cuTensorNet 2.13 and the CUDA Runtime 12.9";

/// The command a Python packager gives for its install.
const PIP_INSTALL: &str = "pip install \"qdk[tensornetwork]\"";

/// The report of `library`, usable, loaded from `path`, with `versions`.
#[cfg(unix)]
fn usable(library: &'static str, path: &str, versions: [(&'static str, i128); 2]) -> LibraryReport {
    LibraryReport {
        library,
        path: Some(PathBuf::from(path)),
        versions: versions
            .map(|(component, value)| Version { component, value })
            .to_vec(),
        faults: Vec::new(),
        other_copies: Vec::new(),
        loaded_from_override: false,
    }
}

/// The report of the audited libraries, found at `/env/…`, with a driver
/// that supports CUDA 12.8: the CUDA Runtime, then cuTensorNet, in the
/// order discovery opens them.
#[cfg(unix)]
fn audited_report() -> AvailabilityReport {
    AvailabilityReport {
        libraries: vec![
            usable(
                "CUDA Runtime",
                "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
                [("CUDA Runtime", 12_090), ("CUDA driver", 12_080)],
            ),
            usable(
                "cuTensorNet",
                "/env/cuquantum/lib/libcutensornet.so.2",
                [
                    ("cuTensorNet", 21_300),
                    ("cuTensorNet's CUDA Runtime", 12_090),
                ],
            ),
        ],
    }
}

/// The audited report with a driver that supports CUDA `driver`.
#[cfg(unix)]
fn with_driver(driver: i128) -> AvailabilityReport {
    let mut report = audited_report();
    report.libraries[0].versions[1].value = driver;
    report
}

/// The error of a discovery where `library` has `faults` and the other
/// library is usable.
#[cfg(unix)]
fn unusable(library: &str, faults: Vec<LibraryFault>) -> AvailabilityError {
    let mut report = audited_report();
    let at_fault = report
        .libraries
        .iter_mut()
        .find(|report| report.library == library)
        .expect("discovery opens the library");
    at_fault.faults = faults;
    AvailabilityError::Unusable(report)
}

/// An A100 with 80 GiB, 79 free: compute capability 8.0.
#[cfg(unix)]
fn a100() -> DeviceReport {
    DeviceReport {
        ordinal: 0,
        compute_capability: (8, 0),
        free_bytes: 79 << 30,
        total_bytes: 80 << 30,
    }
}

/// The setup of a host where discovery found `report`, the devices read
/// are `devices`, and the trial run succeeded.
#[cfg(unix)]
fn found(report: AvailabilityReport, devices: Vec<DeviceReport>) -> GpuSetup {
    GpuSetup {
        availability: Ok(report),
        devices: Some(Ok(devices)),
        trial_run: Some(Ok(())),
        install_command: None,
    }
}

/// The setup of a host where discovery failed with `error`, checked with no
/// packager's install command.
fn failed(error: AvailabilityError) -> GpuSetup {
    GpuSetup {
        availability: Err(error),
        devices: None,
        trial_run: None,
        install_command: None,
    }
}

/// A trial run that failed: no CUDA device, as the algebra reports it.
#[cfg(unix)]
fn failed_trial_run() -> Option<Result<(), CanonicalMpsError<CudaAlgebraError>>> {
    Some(Err(CanonicalMpsError::Algebra(CudaAlgebraError::NoDevice)))
}

/// The advice of `setup`, which has a problem. The asserts are
/// preconditions every advice test shares, not the rule a test checks: a
/// problem leaves the setup unready, and the report ends with its fix.
fn advice(setup: &GpuSetup) -> String {
    assert!(!setup.is_ready(), "a problem leaves the setup unready");
    let advice = setup.advice().expect("a problem has advice");
    assert!(
        setup.to_string().contains(&format!("to fix: {advice}")),
        "the report ends with the fix: {setup}"
    );
    advice
}

/// A user whose libraries were found and whose device ran the trial run
/// reads each library's path and version, the driver's CUDA, each device's
/// compute capability and memory, and that the trial run succeeded, with
/// nothing to fix.
#[cfg(unix)]
#[test]
fn a_ready_setup_lists_what_was_found_and_nothing_to_fix() {
    let setup = found(audited_report(), vec![a100()]);
    assert!(setup.is_ready());
    assert_eq!(setup.advice(), None);
    let report = setup.to_string();
    for expected in [
        "cuTensorNet: /env/cuquantum/lib/libcutensornet.so.2 (2.13.0, built for CUDA Runtime 12.9)",
        "CUDA Runtime: /env/nvidia/cuda_runtime/lib/libcudart.so.12 (12.9)",
        "CUDA driver: supports CUDA 12.8",
        "CUDA device 0: compute capability 8.0, 79.0 of 80.0 GiB free",
        "trial run on CUDA device 0: ready",
    ] {
        assert!(report.contains(expected), "{expected:?} in {report}");
    }
    assert!(!report.contains("to fix"), "{report}");
}

/// A library found nowhere is advised the install of every library, at its
/// audited version, or the override of that library: each library's own
/// variable.
#[cfg(unix)]
#[test]
fn a_missing_library_is_advised_the_install_or_its_override() {
    for (library, variable) in [
        ("cuTensorNet", "QDK_CUTENSORNET_LIBRARY"),
        ("CUDA Runtime", "QDK_CUDART_LIBRARY"),
    ] {
        let advice = advice(&failed(unusable(
            library,
            vec![LibraryFault::NotFound {
                attempted: vec![PathBuf::from("/usr/lib/missing.so")],
            }],
        )));
        assert!(
            advice.contains(INSTALL) && advice.contains(variable),
            "{advice}"
        );
    }
}

/// A library that does not load, or is not the audited version, is
/// advised the install, which brings the audited libraries and their
/// dependencies.
#[cfg(unix)]
#[test]
fn a_library_that_fails_to_load_or_is_not_audited_is_advised_the_install() {
    for fault in [
        LibraryFault::LoadFailed {
            path: PathBuf::from("/usr/lib/libcutensornet.so.2"),
            message: "libcutensor.so.2: cannot open shared object file".to_string(),
        },
        LibraryFault::UnsupportedVersion {
            component: "cuTensorNet",
            found: 21_400,
            supported: 21_300,
        },
        LibraryFault::MissingRequiredSymbol {
            symbol: "cutensornetCreate",
        },
    ] {
        let advice = advice(&failed(unusable("cuTensorNet", vec![fault])));
        assert!(advice.contains(INSTALL), "{advice}");
    }
}

/// An override that is not an absolute path to a file is advised to be
/// set right, or unset, naming the variable.
#[cfg(unix)]
#[test]
fn an_invalid_override_is_advised_by_its_variable() {
    let advice = advice(&failed(unusable(
        "CUDA Runtime",
        vec![LibraryFault::InvalidOverride {
            variable: "QDK_CUDART_LIBRARY",
            path: PathBuf::from("libcudart.so.12"),
            reason: "path must be absolute",
        }],
    )));
    assert!(advice.contains("set QDK_CUDART_LIBRARY"), "{advice}");
}

/// A user whose cuTensorNet override is mistyped, on a host without the
/// CUDA Runtime, is advised the override first: it decides what is loaded,
/// whatever the order the libraries are opened in.
#[cfg(unix)]
#[test]
fn an_invalid_override_is_advised_before_the_faults_of_libraries_opened_first() {
    let AvailabilityError::Unusable(mut report) = unusable(
        "CUDA Runtime",
        vec![LibraryFault::NotFound {
            attempted: vec![PathBuf::from("libcudart.so.12")],
        }],
    ) else {
        unreachable!("unusable gives a report");
    };
    report.libraries[1].faults = vec![LibraryFault::InvalidOverride {
        variable: "QDK_CUTENSORNET_LIBRARY",
        path: PathBuf::from("libcutensornet.so.2"),
        reason: "path must be absolute",
    }];
    let advice = advice(&failed(AvailabilityError::Unusable(report)));
    assert!(advice.contains("set QDK_CUTENSORNET_LIBRARY"), "{advice}");
}

/// Without an invalid override, the advice is about the first fault of the
/// first library at fault, in the order discovery opens them: the CUDA
/// Runtime, on which cuTensorNet runs.
#[cfg(unix)]
#[test]
fn the_first_library_at_fault_is_advised() {
    let AvailabilityError::Unusable(mut report) = unusable(
        "CUDA Runtime",
        vec![LibraryFault::LoadFailed {
            path: PathBuf::from("/usr/lib/libcudart.so.12"),
            message: "wrong ELF class".to_string(),
        }],
    ) else {
        unreachable!("unusable gives a report");
    };
    report.libraries[1].faults = vec![LibraryFault::NotFound {
        attempted: vec![PathBuf::from("libcutensornet.so.2")],
    }];
    let advice = advice(&failed(AvailabilityError::Unusable(report)));
    assert!(
        advice.starts_with("CUDA Runtime was found but did not load"),
        "{advice}"
    );
}

/// A CUDA Runtime that cannot report its version (status 35: the driver is
/// older than the runtime) points at the driver, its CUDA 12 floor and
/// `nvidia-smi`, not at the install.
#[cfg(unix)]
#[test]
fn a_failed_cuda_version_probe_points_at_the_driver() {
    let advice = advice(&failed(unusable(
        "CUDA Runtime",
        vec![LibraryFault::VersionProbeFailed {
            component: "CUDA driver",
            status: 35,
            message: "CUDA driver version is insufficient".to_string(),
        }],
    )));
    assert!(
        advice.contains("nvidia-smi") && advice.contains("525.60.13") && !advice.contains(INSTALL),
        "{advice}"
    );
}

/// A CUDA Runtime of another release lacks a function and, on this host,
/// also fails a version probe: the advice is about the first fault found,
/// the missing function (install the audited version), not the driver.
#[cfg(unix)]
#[test]
fn the_first_fault_of_a_library_is_advised() {
    let advice = advice(&failed(unusable(
        "CUDA Runtime",
        vec![
            LibraryFault::MissingRequiredSymbol {
                symbol: "cudaMemGetInfo",
            },
            LibraryFault::VersionProbeFailed {
                component: "CUDA driver",
                status: 35,
                message: "CUDA driver version is insufficient".to_string(),
            },
        ],
    )));
    assert!(
        advice.starts_with("only the audited versions are supported") && advice.contains(INSTALL),
        "{advice}"
    );
}

/// A library that gives no advice for a failed version probe, as
/// cuTensorNet, whose version calls cannot fail, is advised the install
/// should one fail anyway.
#[cfg(unix)]
#[test]
fn a_library_without_probe_advice_is_advised_the_install_when_its_probe_fails() {
    let advice = advice(&failed(unusable(
        "cuTensorNet",
        vec![LibraryFault::VersionProbeFailed {
            component: "cuTensorNet",
            status: 1,
            message: "not initialized".to_string(),
        }],
    )));
    assert!(
        advice.starts_with("cuTensorNet could not report its version") && advice.contains(INSTALL),
        "{advice}"
    );
}

/// Libraries that load but a trial run that fails on device 0 point at
/// the driver and the device (`nvidia-smi`), not at the install.
#[cfg(unix)]
#[test]
fn a_failed_trial_run_points_at_nvidia_smi() {
    let advice = advice(&GpuSetup {
        trial_run: failed_trial_run(),
        ..found(audited_report(), vec![a100()])
    });
    assert!(
        advice.contains("nvidia-smi") && !advice.contains(INSTALL),
        "{advice}"
    );
}

/// Off Linux x86-64 the advice says where the GPU engine runs.
#[test]
fn an_unsupported_platform_is_advised_where_the_engine_runs() {
    let advice = advice(&failed(AvailabilityError::UnsupportedPlatform {
        platform: "macos aarch64".to_string(),
    }));
    assert!(advice.contains("Linux x86-64"), "{advice}");
}

/// A failed discovery reports its error, and that no trial run was tried.
#[cfg(unix)]
#[test]
fn a_failed_discovery_reports_its_error_and_no_trial_run() {
    let setup = failed(unusable(
        "cuTensorNet",
        vec![LibraryFault::NotFound {
            attempted: vec![PathBuf::from("libcutensornet.so.2")],
        }],
    ));
    let report = setup.to_string();
    assert!(
        report.contains("libraries: cuTensorNet acceleration is unavailable: cuTensorNet")
            && report.contains("not found")
            && report.contains("trial run: not tried"),
        "{report}"
    );
}

/// Device 0, where the GPU engine runs, below compute capability 7.5 is a
/// problem naming both capabilities; a device 0 of 7.5, the minimum, is
/// not.
#[cfg(unix)]
#[test]
fn a_device_0_below_the_minimum_compute_capability_is_a_problem() {
    let old = DeviceReport {
        compute_capability: (7, 0),
        ..a100()
    };
    let advice = advice(&found(audited_report(), vec![old]));
    assert!(advice.contains("7.0") && advice.contains("7.5"), "{advice}");
    let turing = DeviceReport {
        compute_capability: (7, 5),
        ..a100()
    };
    assert!(found(audited_report(), vec![turing]).is_ready());
}

/// A host whose second device is below compute capability 7.5 is ready:
/// the GPU engine runs on device 0 only.
#[cfg(unix)]
#[test]
fn a_second_device_below_the_minimum_leaves_the_setup_ready() {
    let old = DeviceReport {
        ordinal: 1,
        compute_capability: (7, 0),
        ..a100()
    };
    assert!(found(audited_report(), vec![a100(), old]).is_ready());
}

/// A host with no CUDA device points at `nvidia-smi`.
#[cfg(unix)]
#[test]
fn no_device_points_at_nvidia_smi() {
    let advice = advice(&found(audited_report(), Vec::new()));
    assert!(
        advice.starts_with("no CUDA device was found") && advice.contains("nvidia-smi"),
        "{advice}"
    );
}

/// Devices that could not be read point at `nvidia-smi`.
#[cfg(unix)]
#[test]
fn unreadable_devices_point_at_nvidia_smi() {
    let advice = advice(&GpuSetup {
        devices: Some(Err(CudaAlgebraError::NoDevice)),
        ..found(audited_report(), vec![a100()])
    });
    assert!(
        advice.starts_with("the CUDA devices could not be read") && advice.contains("nvidia-smi"),
        "{advice}"
    );
}

/// A driver that supports no CUDA 12 is a problem naming NVIDIA's floor;
/// a driver for CUDA 12.0, the floor, is not.
#[cfg(unix)]
#[test]
fn a_driver_below_cuda_12_is_a_problem_naming_the_floor() {
    let advice = advice(&found(with_driver(11_080), vec![a100()]));
    assert!(
        advice.contains("11.8") && advice.contains("525.60.13"),
        "{advice}"
    );
    assert!(found(with_driver(12_000), vec![a100()]).is_ready());
}

/// A host whose driver supports CUDA 11.8 only sees no device through it:
/// the advice is about the driver, the cause, not the missing device.
#[cfg(unix)]
#[test]
fn a_driver_below_cuda_12_is_advised_before_the_devices() {
    let advice = advice(&found(with_driver(11_080), Vec::new()));
    assert!(
        advice.starts_with("the NVIDIA driver supports CUDA 11.8 only"),
        "{advice}"
    );
}

/// A host with no device fails the trial run too: the advice is about the
/// devices, which come first, not the trial run.
#[cfg(unix)]
#[test]
fn the_devices_are_advised_before_the_trial_run() {
    let advice = advice(&GpuSetup {
        trial_run: failed_trial_run(),
        ..found(audited_report(), Vec::new())
    });
    assert!(advice.starts_with("no CUDA device was found"), "{advice}");
}

/// A device 0 of compute capability 7.0 on which the trial run also
/// failed: the advice is about the device's compute capability, not the
/// trial run.
#[cfg(unix)]
#[test]
fn device_0s_compute_capability_is_advised_before_the_trial_run() {
    let old = DeviceReport {
        compute_capability: (7, 0),
        ..a100()
    };
    let advice = advice(&GpuSetup {
        trial_run: failed_trial_run(),
        ..found(audited_report(), vec![old])
    });
    assert!(
        advice.starts_with("CUDA device 0, where the GPU engine runs, has compute capability 7.0"),
        "{advice}"
    );
}

/// A host with no CUDA device reads so on the devices' line.
#[cfg(unix)]
#[test]
fn no_device_is_shown_as_none() {
    let report = found(audited_report(), Vec::new()).to_string();
    assert!(report.contains("\nCUDA devices: none\n"), "{report}");
}

/// Devices that could not be read are shown with the CUDA Runtime's error
/// on the devices' line.
#[cfg(unix)]
#[test]
fn unreadable_devices_are_shown_with_their_error() {
    let report = GpuSetup {
        devices: Some(Err(CudaAlgebraError::NoDevice)),
        ..found(audited_report(), vec![a100()])
    }
    .to_string();
    assert!(
        report.contains("\nCUDA devices: no CUDA-capable device is available\n"),
        "{report}"
    );
}

/// A failed trial run is shown on its line with its error and the error's
/// cause: the engine's error says only that its algebra failed, and the
/// algebra's says why.
#[cfg(unix)]
#[test]
fn a_failed_trial_run_is_shown_with_its_error() {
    let report = GpuSetup {
        trial_run: failed_trial_run(),
        ..found(audited_report(), vec![a100()])
    }
    .to_string();
    assert!(
        report.contains(
            "\ntrial run on CUDA device 0: the site algebra failed: \
             no CUDA-capable device is available"
        ),
        "{report}"
    );
}

/// Another copy of a library beside the one loaded is a warning naming both
/// and the variable that chooses, and leaves the setup ready.
#[cfg(unix)]
#[test]
fn another_copy_of_a_library_is_a_warning_that_leaves_the_setup_ready() {
    let mut report = audited_report();
    report.libraries[0].other_copies = vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")];
    report.libraries[1].path = Some(PathBuf::from("/opt/override/libcutensornet.so.2"));
    report.libraries[1].loaded_from_override = true;
    report.libraries[1].other_copies =
        vec![PathBuf::from("/env/cuquantum/lib/libcutensornet.so.2")];
    let setup = found(report, vec![a100()]);
    assert!(setup.is_ready());
    let warnings = setup.warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings[0].contains("/env/cuquantum/lib/libcutensornet.so.2")
            && warnings[0].contains("/opt/override/libcutensornet.so.2")
            && warnings[0].contains("QDK_CUTENSORNET_LIBRARY"),
        "{}",
        warnings[0]
    );
    assert!(
        warnings[1].contains("QDK_CUDART_LIBRARY"),
        "{}",
        warnings[1]
    );
    assert!(
        setup
            .to_string()
            .contains(&format!("warning: {}", warnings[0])),
        "{setup}"
    );
}

/// A user whose cuTensorNet override is loaded beside a wheel's copy is
/// told to remove the other copy or unset the variable; a user whose CUDA
/// Runtime came from the usual paths, beside a system copy, is told to
/// remove one or set the variable to the one to load.
#[cfg(unix)]
#[test]
fn a_warning_says_to_unset_an_override_that_is_loaded_or_else_to_set_it() {
    let mut report = audited_report();
    report.libraries[0].other_copies = vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")];
    report.libraries[1].path = Some(PathBuf::from("/opt/override/libcutensornet.so.2"));
    report.libraries[1].loaded_from_override = true;
    report.libraries[1].other_copies =
        vec![PathBuf::from("/env/cuquantum/lib/libcutensornet.so.2")];
    let warnings = found(report, vec![a100()]).warnings();
    let [cutensornet, cuda_runtime] = warnings.as_slice() else {
        panic!("one warning per other copy: {warnings:?}");
    };
    assert!(
        cutensornet.ends_with(": remove the other, or unset QDK_CUTENSORNET_LIBRARY"),
        "{cutensornet}"
    );
    assert!(
        cuda_runtime.ends_with(": remove one, or set QDK_CUDART_LIBRARY to the one to load"),
        "{cuda_runtime}"
    );
}

/// A user whose device 0 is too old, with another copy of the CUDA Runtime
/// visible, reads what was found first, then the warning, then the fix
/// last.
#[cfg(unix)]
#[test]
fn the_report_lists_what_was_found_then_the_warnings_then_the_fix() {
    let mut report = audited_report();
    report.libraries[0].other_copies = vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")];
    let old = DeviceReport {
        compute_capability: (7, 0),
        ..a100()
    };
    let text = found(report, vec![old]).to_string();
    let position = |line: &str| {
        text.find(line)
            .unwrap_or_else(|| panic!("{line:?} in {text}"))
    };
    let found_last = position("trial run on CUDA device 0: ready");
    let warning = position("\nwarning: another copy of CUDA Runtime");
    let fix = position("\nto fix: CUDA device 0");
    assert!(found_last < warning && warning < fix, "{text}");
}

/// A user who installed through a packager, here Python's, is advised the
/// install with the packager's own command, so they can run it as is.
#[cfg(unix)]
#[test]
fn the_packagers_install_command_is_advised() {
    let advice = advice(&GpuSetup {
        install_command: Some(PIP_INSTALL.to_string()),
        ..failed(unusable(
            "cuTensorNet",
            vec![LibraryFault::NotFound {
                attempted: vec![PathBuf::from("libcutensornet.so.2")],
            }],
        ))
    });
    assert!(
        advice.starts_with(&format!("{INSTALL}, for instance with `{PIP_INSTALL}`")),
        "{advice}"
    );
}

/// A Rust caller that gives no install command is advised what to install,
/// with no command it did not give: the advice names the libraries and
/// their versions only.
#[cfg(unix)]
#[test]
fn without_a_packager_the_advice_names_no_command() {
    let advice = advice(&failed(unusable(
        "cuTensorNet",
        vec![LibraryFault::NotFound {
            attempted: vec![PathBuf::from("libcutensornet.so.2")],
        }],
    )));
    assert!(
        advice.starts_with(&format!("{INSTALL}, or set ")) && !advice.contains('`'),
        "{advice}"
    );
}
