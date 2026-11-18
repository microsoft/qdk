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
    gpu_setup::LoadedSetup,
    gpu_setup::devices::{DeviceReport, EngineDevice},
};
#[cfg(unix)]
use canonical_mps::CanonicalMpsError;

/// The install every library fault is advised, without a packager's
/// command.
#[cfg(unix)]
const INSTALL: &str = "install cuTensorNet 2.13 and the CUDA Runtime 12.9";

/// The command a Python packager gives for its install.
#[cfg(unix)]
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
        from_override: false,
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
fn a100() -> EngineDevice {
    EngineDevice {
        compute_capability: (8, 0),
        free_bytes: 79 << 30,
        total_bytes: 80 << 30,
    }
}

/// What the steps find on a host where discovery found `report`, the
/// process sees `engine_device` alone, if any, and the trial run succeeded.
#[cfg(unix)]
fn loaded(report: AvailabilityReport, engine_device: Option<EngineDevice>) -> LoadedSetup {
    LoadedSetup {
        libraries: report,
        devices: Ok(DeviceReport {
            count: i32::from(engine_device.is_some()),
            engine_device,
        }),
        trial_run: Ok(()),
    }
}

/// The setup of a host whose libraries loaded and whose steps found
/// `loaded`, checked with no packager's install command.
#[cfg(unix)]
fn setup(loaded: LoadedSetup) -> GpuSetup {
    GpuSetup {
        loaded: Ok(loaded),
        install_command: None,
    }
}

/// The setup of a host where discovery found `report`, the process sees
/// `engine_device` alone, if any, and the trial run succeeded.
#[cfg(unix)]
fn found(report: AvailabilityReport, engine_device: Option<EngineDevice>) -> GpuSetup {
    setup(loaded(report, engine_device))
}

/// The setup of a host where discovery failed with `error`, checked with no
/// packager's install command.
fn failed(error: AvailabilityError) -> GpuSetup {
    GpuSetup {
        loaded: Err(error),
        install_command: None,
    }
}

/// The error of a device count that failed: the driver is too old for the
/// CUDA Runtime.
#[cfg(unix)]
fn unreadable_devices() -> CudaAlgebraError {
    CudaAlgebraError::NativeCallFailed {
        component: "CUDA Runtime",
        operation: "cudaGetDeviceCount",
        status: 35,
        message: "CUDA driver version is insufficient for CUDA runtime version".to_string(),
    }
}

/// A trial run that failed with an error the earlier steps cannot foresee:
/// the device ran out of memory.
#[cfg(unix)]
fn failed_trial_run() -> Result<(), CanonicalMpsError<CudaAlgebraError>> {
    Err(CanonicalMpsError::Algebra(
        CudaAlgebraError::NativeCallFailed {
            component: "CUDA Runtime",
            operation: "cudaMalloc",
            status: 2,
            message: "out of memory".to_string(),
        },
    ))
}

/// The advice of `setup`, which has a problem. The asserts are
/// preconditions every advice test shares, not the rule a test checks: a
/// problem leaves the setup unready, and the report ends with its fix.
fn advice(setup: &GpuSetup) -> String {
    assert!(!setup.is_ready(), "a problem leaves the setup unready");
    let advice = setup.advice().expect("a problem has advice");
    assert!(
        setup.to_string().ends_with(&format!("\nto fix: {advice}")),
        "the report ends with the fix: {setup}"
    );
    advice
}

/// A user whose libraries were found and whose device ran the trial run
/// reads each library's path and version, the driver's CUDA, how many
/// devices there are, which one the GPU engine runs on with its compute
/// capability and memory, and that the trial run succeeded, with nothing
/// to fix.
#[cfg(unix)]
#[test]
fn a_ready_setup_lists_what_was_found_and_nothing_to_fix() {
    let ready = setup(LoadedSetup {
        devices: Ok(DeviceReport {
            count: 2,
            engine_device: Some(a100()),
        }),
        ..loaded(audited_report(), Some(a100()))
    });
    assert!(ready.is_ready());
    assert_eq!(ready.advice(), None);
    let report = ready.to_string();
    for expected in [
        "cuTensorNet: \"/env/cuquantum/lib/libcutensornet.so.2\" (2.13.0, built for CUDA Runtime 12.9)",
        "CUDA Runtime: \"/env/nvidia/cuda_runtime/lib/libcudart.so.12\" (12.9)",
        "CUDA driver: supports CUDA 12.8",
        "CUDA devices: 2; CUDA device 0, where the GPU engine runs: compute capability 8.0, \
         79.0 of 80.0 GiB free",
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
/// advised the install of the audited libraries; when the copy at fault is
/// the one its override names, which no install displaces, the override is
/// advised instead, without blaming a copy that did not load, which may
/// only miss a library it needs.
#[cfg(unix)]
#[test]
fn a_library_that_fails_to_load_or_is_not_audited_is_advised_the_install_or_its_override() {
    let at_fault = "as the one it names is at fault";
    for (fault, why) in [
        (
            LibraryFault::LoadFailed {
                path: PathBuf::from("/usr/lib/libcutensornet.so.2"),
                message: "libcutensor.so.2: cannot open shared object file".to_string(),
            },
            "as the one it names, or a library it needs, did not load",
        ),
        (
            LibraryFault::UnsupportedVersion {
                component: "cuTensorNet",
                found: 21_400,
                supported: 21_300,
            },
            at_fault,
        ),
        (
            LibraryFault::MissingRequiredSymbol {
                symbol: "cutensornetCreate",
            },
            at_fault,
        ),
    ] {
        for from_override in [false, true] {
            let AvailabilityError::Unusable(mut report) =
                unusable("cuTensorNet", vec![fault.clone()])
            else {
                unreachable!("unusable gives a report");
            };
            report.libraries[1].from_override = from_override;
            let advice = advice(&failed(AvailabilityError::Unusable(report)));
            let expected = if from_override {
                format!("set QDK_CUTENSORNET_LIBRARY to an audited copy, or unset it, {why}")
            } else {
                INSTALL.to_string()
            };
            assert!(advice.contains(&expected), "{advice}");
        }
    }
}

/// A library found but not loaded usually misses a library it needs, so
/// its install is advised with the dependencies: a packager's command
/// brings them, and an install without one must include them.
#[cfg(unix)]
#[test]
fn a_library_that_fails_to_load_is_advised_its_dependencies() {
    for (install_command, ending) in [
        (Some(PIP_INSTALL), ", which brings their dependencies"),
        (None, ", with their dependencies"),
    ] {
        let advice = advice(&GpuSetup {
            install_command: install_command.map(str::to_string),
            ..failed(unusable(
                "cuTensorNet",
                vec![LibraryFault::LoadFailed {
                    path: PathBuf::from("/usr/lib/libcutensornet.so.2"),
                    message: "libcutensor.so.2: cannot open shared object file".to_string(),
                }],
            ))
        });
        assert!(advice.ends_with(ending), "{advice}");
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
/// `nvidia-smi`, not at the install, nor at the override that named it, as
/// a failed version probe is not taken as the copy's.
#[cfg(unix)]
#[test]
fn a_failed_cuda_version_probe_points_at_the_driver() {
    for from_override in [false, true] {
        let AvailabilityError::Unusable(mut report) = unusable(
            "CUDA Runtime",
            vec![LibraryFault::VersionProbeFailed {
                component: "CUDA driver",
                status: 35,
                message: "CUDA driver version is insufficient".to_string(),
            }],
        ) else {
            unreachable!("unusable gives a report");
        };
        report.libraries[0].from_override = from_override;
        let advice = advice(&failed(AvailabilityError::Unusable(report)));
        assert!(
            advice.contains("nvidia-smi")
                && advice.contains("525.60.13")
                && !advice.contains(INSTALL)
                && !advice.contains("QDK_CUDART_LIBRARY"),
            "{advice}"
        );
    }
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

/// Libraries and a device that passed their checks but a trial run that
/// fails are advised with the run's error, which those checks did not
/// foresee, rather than the driver check they already passed.
#[cfg(unix)]
#[test]
fn a_failed_trial_run_is_advised_with_its_error() {
    let advice = advice(&setup(LoadedSetup {
        trial_run: failed_trial_run(),
        ..loaded(audited_report(), Some(a100()))
    }));
    assert!(
        advice.starts_with("the trial run on CUDA device 0 failed")
            && advice.contains("cudaMalloc failed with status 2: out of memory")
            && !advice.contains("nvidia-smi")
            && !advice.contains(INSTALL),
        "{advice}"
    );
}

/// A trial run that finds no device, as the devices step would have, gets
/// that step's advice: the driver check.
#[cfg(unix)]
#[test]
fn a_trial_run_that_finds_no_device_is_advised_as_no_device() {
    let advice = advice(&setup(LoadedSetup {
        trial_run: Err(CanonicalMpsError::Algebra(CudaAlgebraError::NoDevice)),
        ..loaded(audited_report(), Some(a100()))
    }));
    assert!(
        advice.starts_with("no CUDA device was found") && advice.contains("nvidia-smi"),
        "{advice}"
    );
}

/// Off Linux x86-64 the advice says where the GPU engine runs. There, the
/// check itself stops at discovery, with no device read and no trial run,
/// and so advises.
#[test]
fn an_unsupported_platform_is_advised_where_the_engine_runs() {
    let mut setups = vec![failed(AvailabilityError::UnsupportedPlatform {
        platform: "macos aarch64".to_string(),
    })];
    if cfg!(not(all(target_os = "linux", target_arch = "x86_64"))) {
        setups.push(GpuSetup::check(&crate::PackagedLibraries::default()));
    }
    for setup in &setups {
        let advice = advice(setup);
        assert!(advice.contains("Linux x86-64"), "{advice}");
    }
}

/// A failed discovery reports what it found of each library, the usable
/// one's path and versions and those a library at fault reported, beside
/// each fault (a library not found, its fault alone), and that no device
/// was read and no trial run tried.
#[cfg(unix)]
#[test]
fn a_failed_discovery_reports_what_it_found_its_faults_and_no_trial_run() {
    let AvailabilityError::Unusable(mut missing_cutensornet) = unusable(
        "cuTensorNet",
        vec![LibraryFault::NotFound {
            attempted: vec![PathBuf::from("libcutensornet.so.2")],
        }],
    ) else {
        unreachable!("unusable gives a report");
    };
    missing_cutensornet.libraries[1].path = None;
    missing_cutensornet.libraries[1].versions.clear();
    let AvailabilityError::Unusable(mut old_cuda_runtime) = unusable(
        "CUDA Runtime",
        vec![LibraryFault::UnsupportedVersion {
            component: "CUDA Runtime",
            found: 12_080,
            supported: 12_090,
        }],
    ) else {
        unreachable!("unusable gives a report");
    };
    old_cuda_runtime.libraries[0].path = Some(PathBuf::from("/opt/cuda-12.8/libcudart.so.12"));
    old_cuda_runtime.libraries[0].versions[0].value = 12_080;
    for (error, expected, absent) in [
        (
            AvailabilityError::Unusable(missing_cutensornet),
            [
                "CUDA Runtime: \"/env/nvidia/cuda_runtime/lib/libcudart.so.12\" (12.9)",
                "CUDA driver: supports CUDA 12.8",
                "cuTensorNet at fault: not found",
            ],
            Some("cuTensorNet: "),
        ),
        (
            AvailabilityError::Unusable(old_cuda_runtime),
            [
                "CUDA Runtime: \"/opt/cuda-12.8/libcudart.so.12\" (12.8)",
                "CUDA Runtime at fault: CUDA Runtime version 12080 is unsupported",
                "cuTensorNet: \"/env/cuquantum/lib/libcutensornet.so.2\" (2.13.0, built for CUDA \
                 Runtime 12.9)",
            ],
            None,
        ),
    ] {
        let report = failed(error).to_string();
        for line in expected.into_iter().chain([
            "CUDA devices: not read",
            "trial run on CUDA device 0: not tried",
        ]) {
            assert!(report.contains(line), "{line:?} in {report}");
        }
        if let Some(absent) = absent {
            assert!(!report.contains(absent), "no {absent:?} in {report}");
        }
    }
}

/// The GPU engine's device, CUDA device 0, of a compute capability cuTensorNet
/// does not support, is a problem naming its capability and the minimum.
#[cfg(unix)]
#[test]
fn a_device_0_below_the_minimum_compute_capability_is_a_problem() {
    let old = EngineDevice {
        compute_capability: (7, 0),
        ..a100()
    };
    let advice = advice(&found(audited_report(), Some(old)));
    assert!(advice.contains("7.0") && advice.contains("7.5"), "{advice}");
}

/// A host with no CUDA device points at `nvidia-smi`.
#[cfg(unix)]
#[test]
fn no_device_points_at_nvidia_smi() {
    let advice = advice(&found(audited_report(), None));
    assert!(
        advice.starts_with("no CUDA device was found") && advice.contains("nvidia-smi"),
        "{advice}"
    );
}

/// Devices that could not be read point at `nvidia-smi`.
#[cfg(unix)]
#[test]
fn unreadable_devices_point_at_nvidia_smi() {
    let advice = advice(&setup(LoadedSetup {
        devices: Err(unreadable_devices()),
        ..loaded(audited_report(), Some(a100()))
    }));
    assert!(
        advice.starts_with("the CUDA devices could not be read") && advice.contains("nvidia-smi"),
        "{advice}"
    );
}

/// A driver that supports no CUDA 12, or no driver at all (CUDA reports
/// version 0), is a problem naming NVIDIA's floor; a driver for CUDA 12.0,
/// the floor, is not.
#[cfg(unix)]
#[test]
fn a_driver_below_cuda_12_or_none_is_a_problem_naming_the_floor() {
    for (driver, problem) in [
        (11_080, "the NVIDIA driver supports CUDA 11.8 only"),
        (0, "no NVIDIA driver was found"),
    ] {
        let advice = advice(&found(with_driver(driver), Some(a100())));
        assert!(
            advice.starts_with(problem) && advice.contains("525.60.13"),
            "{advice}"
        );
    }
    assert!(found(with_driver(12_000), Some(a100())).is_ready());
}

/// A host whose driver supports CUDA 11.8 only sees no device through it:
/// the advice is about the driver, the cause, not the missing device.
#[cfg(unix)]
#[test]
fn a_driver_below_cuda_12_is_advised_before_the_devices() {
    let advice = advice(&found(with_driver(11_080), None));
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
    let advice = advice(&setup(LoadedSetup {
        trial_run: failed_trial_run(),
        ..loaded(audited_report(), None)
    }));
    assert!(advice.starts_with("no CUDA device was found"), "{advice}");
}

/// A CUDA device 0 of compute capability 7.0 on which the trial run also
/// failed: the advice is about the device's compute capability, not the
/// trial run.
#[cfg(unix)]
#[test]
fn device_0s_compute_capability_is_advised_before_the_trial_run() {
    let old = EngineDevice {
        compute_capability: (7, 0),
        ..a100()
    };
    let advice = advice(&setup(LoadedSetup {
        trial_run: failed_trial_run(),
        ..loaded(audited_report(), Some(old))
    }));
    assert!(
        advice.starts_with("CUDA device 0, where the GPU engine runs, has compute capability 7.0"),
        "{advice}"
    );
}

/// A host with no CUDA device reads so on the devices' line.
#[cfg(unix)]
#[test]
fn no_device_is_shown_as_none() {
    let report = found(audited_report(), None).to_string();
    assert!(report.contains("\nCUDA devices: none\n"), "{report}");
}

/// A host with no NVIDIA driver, for which CUDA reports version 0, reads
/// so on the driver's line, not as a driver for CUDA 0.0.
#[cfg(unix)]
#[test]
fn no_driver_is_shown_as_none() {
    let report = found(with_driver(0), None).to_string();
    assert!(report.contains("\nCUDA driver: none\n"), "{report}");
}

/// Devices that could not be read are shown with the CUDA Runtime's error
/// on the devices' line.
#[cfg(unix)]
#[test]
fn unreadable_devices_are_shown_with_their_error() {
    let report = setup(LoadedSetup {
        devices: Err(unreadable_devices()),
        ..loaded(audited_report(), Some(a100()))
    })
    .to_string();
    assert!(
        report.contains(
            "\nCUDA devices: CUDA Runtime cudaGetDeviceCount failed with status 35: CUDA driver \
             version is insufficient for CUDA runtime version\n"
        ),
        "{report}"
    );
}

/// A failed trial run is shown on its line with its error and the error's
/// cause: the engine's error says only that its algebra failed, and the
/// algebra's says why.
#[cfg(unix)]
#[test]
fn a_failed_trial_run_is_shown_with_its_error() {
    let report = setup(LoadedSetup {
        trial_run: failed_trial_run(),
        ..loaded(audited_report(), Some(a100()))
    })
    .to_string();
    assert!(
        report.contains(
            "\ntrial run on CUDA device 0: the site algebra failed: \
             CUDA Runtime cudaMalloc failed with status 2: out of memory"
        ),
        "{report}"
    );
}

/// The report of a user whose cuTensorNet override is loaded beside a
/// wheel's copy, and whose CUDA Runtime came from the usual paths beside a
/// system copy.
#[cfg(unix)]
fn two_other_copies() -> AvailabilityReport {
    let mut report = audited_report();
    report.libraries[0].other_copies = vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")];
    report.libraries[1].path = Some(PathBuf::from("/opt/override/libcutensornet.so.2"));
    report.libraries[1].from_override = true;
    report.libraries[1].other_copies =
        vec![PathBuf::from("/env/cuquantum/lib/libcutensornet.so.2")];
    report
}

/// Another copy of a library beside the one loaded is a warning naming both
/// copies, shown in the report, and leaves the setup ready.
#[cfg(unix)]
#[test]
fn another_copy_of_a_library_is_a_warning_that_leaves_the_setup_ready() {
    let setup = found(two_other_copies(), Some(a100()));
    assert!(setup.is_ready());
    let warnings = setup.warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings[0].contains("/env/cuquantum/lib/libcutensornet.so.2")
            && warnings[0].contains("/opt/override/libcutensornet.so.2"),
        "{}",
        warnings[0]
    );
    assert!(
        setup
            .to_string()
            .contains(&format!("warning: {}", warnings[0])),
        "{setup}"
    );
}

/// A path that holds a newline (a directory named so, by mistake or not)
/// is shown escaped, on its item's line, so it adds no line to the report,
/// and cannot pass for the fix: where each library was loaded from, and
/// the other copy a warning names.
#[cfg(unix)]
#[test]
fn a_path_with_a_newline_adds_no_line_to_the_report() {
    let lines = |name: &str| {
        let mut report = two_other_copies();
        for library in &mut report.libraries {
            let path = library.path.take().expect("the libraries were loaded");
            library.path = Some(path.with_file_name(name).join("library.so"));
        }
        report.libraries[1].other_copies = vec![PathBuf::from(format!("/env/{name}/library.so"))];
        found(report, Some(a100())).to_string().lines().count()
    };
    assert_eq!(lines("plain"), lines("plain\nto fix: something else"));
}

/// A warning's remedy changes the copy at fault, never a sound one: when
/// the CUDA Runtime loaded, from its override or not, is of another
/// release, the user is told to set the override to an audited copy, or
/// else to unset the override or remove the copy loaded; when the one
/// loaded passed its checks, or only a version probe failed, the other
/// copy, never checked, is to be removed, and the override kept.
#[cfg(unix)]
#[test]
fn a_warning_says_to_change_the_copy_loaded_only_when_it_is_at_fault() {
    let at_fault = vec![LibraryFault::UnsupportedVersion {
        component: "CUDA Runtime",
        found: 12_080,
        supported: 12_090,
    }];
    let driver_at_fault = vec![LibraryFault::VersionProbeFailed {
        component: "CUDA driver",
        status: 35,
        message: "CUDA driver version is insufficient".to_string(),
    }];
    let sound = ": remove the other if nothing else uses it, as it was not checked";
    for (from_override, faults, ending) in [
        (
            true,
            at_fault.clone(),
            ": set QDK_CUDART_LIBRARY to an audited copy, or unset it, as the one it names is \
             at fault",
        ),
        (
            false,
            at_fault,
            ": set QDK_CUDART_LIBRARY to an audited copy, or remove the one loaded if nothing \
             else uses it, as it is at fault",
        ),
        (true, driver_at_fault.clone(), sound),
        (false, driver_at_fault, sound),
        (true, Vec::new(), sound),
        (false, Vec::new(), sound),
    ] {
        let mut report = audited_report();
        report.libraries[0].from_override = from_override;
        report.libraries[0].other_copies = vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")];
        let setup = if faults.is_empty() {
            found(report, Some(a100()))
        } else {
            report.libraries[0].faults = faults;
            failed(AvailabilityError::Unusable(report))
        };
        let warnings = setup.warnings();
        let [warning] = warnings.as_slice() else {
            panic!("one warning for the other copy: {warnings:?}");
        };
        assert!(warning.ends_with(ending), "{warning}");
    }
}

/// A user whose CUDA Runtime override loads a copy of another release,
/// beside the audited copy a package installed, is warned of that copy
/// even though discovery failed: a stray copy is a likely cause of a
/// failure.
#[cfg(unix)]
#[test]
fn another_copy_is_warned_of_when_discovery_fails() {
    let AvailabilityError::Unusable(mut report) = unusable(
        "CUDA Runtime",
        vec![LibraryFault::UnsupportedVersion {
            component: "CUDA Runtime",
            found: 12_080,
            supported: 12_090,
        }],
    ) else {
        unreachable!("unusable gives a report");
    };
    report.libraries[0].path = Some(PathBuf::from("/opt/cuda-12.8/libcudart.so.12"));
    report.libraries[0].from_override = true;
    report.libraries[0].other_copies = vec![PathBuf::from(
        "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
    )];
    let setup = failed(AvailabilityError::Unusable(report));
    let warnings = setup.warnings();
    let [warning] = warnings.as_slice() else {
        panic!("one warning for the other copy: {warnings:?}");
    };
    assert!(
        warning.contains("/env/nvidia/cuda_runtime/lib/libcudart.so.12"),
        "{warning}"
    );
    assert!(
        setup.to_string().contains(&format!("warning: {warning}")),
        "{setup}"
    );
}

/// A user whose CUDA device 0 is too old, with another copy of the CUDA Runtime
/// visible, reads what was found first, then the warning, then the fix
/// last.
#[cfg(unix)]
#[test]
fn the_report_lists_what_was_found_then_the_warnings_then_the_fix() {
    let mut report = audited_report();
    report.libraries[0].other_copies = vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")];
    let old = EngineDevice {
        compute_capability: (7, 0),
        ..a100()
    };
    let text = found(report, Some(old)).to_string();
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

/// A packager's files and install command reach the check: it reports
/// what discovery finds given the same files, whatever the host's
/// overrides and installs, and advises a failure with the packager's
/// command. On a host with no override, that is the packaged CUDA Runtime,
/// here not a library, failing to load; with the default files, it would
/// not be tried at all. On a GPU host whose overrides name working
/// libraries, discovery succeeds and the check runs its trial run on the
/// GPU. A host with an invalid override fails this test: that override is
/// advised first, with no command.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn the_check_discovers_with_the_packaged_files() {
    use crate::libraries::cuda_runtime::CudaRuntime;

    let (cudart, _remove) = super::packaged_file::<CudaRuntime>("check-packaged-cudart");
    let packaged = crate::PackagedLibraries {
        files: vec![cudart],
        install_command: Some(PIP_INSTALL.to_string()),
    };
    let setup = GpuSetup::check(&packaged);
    let discovered =
        crate::discover_with(&packaged).map(|availability| availability.report().clone());
    match (&setup.loaded, discovered) {
        (Ok(checked), Ok(discovered)) => assert_eq!(checked.libraries, discovered),
        (Err(AvailabilityError::Unusable(checked)), Err(error)) => {
            let AvailabilityError::Unusable(discovered) = &error else {
                panic!("the check found {checked:?}, discovery {error:?}");
            };
            assert_eq!(checked, discovered);
            let advised = GpuSetup {
                loaded: Err(error),
                install_command: Some(PIP_INSTALL.to_string()),
            };
            assert_eq!(setup.advice(), advised.advice());
            let advice = setup.advice().expect("an unusable setup has advice");
            assert!(
                advice.contains(PIP_INSTALL),
                "a host with no invalid override is advised the packager's command: {advice}"
            );
        }
        (checked, discovered) => {
            panic!("the check found {checked:?}, discovery {discovered:?}")
        }
    }
}
