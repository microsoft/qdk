// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Diagnosis`]: that a user reads what was found, whether the method is
//! ready, and for each kind of problem, the fix that applies to it.

use std::path::PathBuf;

use crate::{AvailabilityError, AvailabilityReport, DeviceReport, Diagnosis, EngineError};

const INSTALL: &str = "pip install \"qdk[tensornetwork]\"";

/// The report of the audited pair, found at `/env/…`, with a driver that
/// supports CUDA 12.8.
fn audited_report() -> AvailabilityReport {
    AvailabilityReport {
        cutensornet_library: PathBuf::from("/env/cuquantum/lib/libcutensornet.so.2"),
        cuda_runtime_library: PathBuf::from("/env/nvidia/cuda_runtime/lib/libcudart.so.12"),
        cutensornet_version: 21_300,
        cutensornet_cuda_runtime_version: 12_090,
        cuda_runtime_version: 12_090,
        cuda_driver_version: 12_080,
        other_cutensornet_copies: Vec::new(),
        other_cuda_runtime_copies: Vec::new(),
    }
}

/// An A100 with 80 GiB, 79 free: compute capability 8.0.
fn a100() -> DeviceReport {
    DeviceReport {
        ordinal: 0,
        compute_capability: (8, 0),
        free_bytes: 79 << 30,
        total_bytes: 80 << 30,
    }
}

/// The diagnosis of a host where discovery found `report`, the devices
/// read are `devices`, and the engine ran its gate.
fn found(report: AvailabilityReport, devices: Vec<DeviceReport>) -> Diagnosis {
    Diagnosis {
        availability: Ok(report),
        devices: Some(Ok(devices)),
        engine: Some(Ok(())),
    }
}

/// The diagnosis of a discovery that failed with `error`.
fn failed(error: AvailabilityError) -> Diagnosis {
    Diagnosis {
        availability: Err(error),
        devices: None,
        engine: None,
    }
}

fn advice(diagnosis: &Diagnosis) -> String {
    assert!(!diagnosis.is_ready(), "a problem leaves the method unready");
    let advice = diagnosis.advice().expect("a problem has advice");
    assert!(
        diagnosis.to_string().contains(&format!("to fix: {advice}")),
        "the report ends with the fix: {diagnosis}"
    );
    advice
}

/// A user whose libraries were found and whose device ran the engine reads
/// each library's path and version, the driver's CUDA, each device's
/// compute capability and memory, and that the method is ready, with
/// nothing to fix.
#[test]
fn a_ready_diagnosis_lists_the_libraries_and_their_versions() {
    let diagnosis = found(audited_report(), vec![a100()]);
    assert!(diagnosis.is_ready());
    assert_eq!(diagnosis.advice(), None);
    let report = diagnosis.to_string();
    for expected in [
        "cuTensorNet: /env/cuquantum/lib/libcutensornet.so.2 (2.13.0, built for CUDA Runtime 12.9)",
        "CUDA Runtime: /env/nvidia/cuda_runtime/lib/libcudart.so.12 (12.9)",
        "CUDA driver: supports CUDA 12.8",
        "CUDA device 0: compute capability 8.0, 79.0 of 80.0 GiB free",
        "MPS engine on CUDA device 0: ready",
    ] {
        assert!(report.contains(expected), "{expected:?} in {report}");
    }
    assert!(!report.contains("to fix"), "{report}");
}

/// A library found nowhere is advised the install, or the override of that
/// library: each library's own variable.
#[test]
fn a_missing_library_is_advised_the_install_or_its_override() {
    for (library, variable) in [
        ("cuTensorNet", "QDK_CUTENSORNET_LIBRARY"),
        ("CUDA Runtime", "QDK_CUDART_LIBRARY"),
    ] {
        let advice = advice(&failed(AvailabilityError::LibraryNotFound {
            library,
            attempted: vec![PathBuf::from("/usr/lib/missing.so")],
        }));
        assert!(
            advice.contains(INSTALL) && advice.contains(variable),
            "{advice}"
        );
    }
}

/// A library that does not load, or is not the audited version, is
/// advised the install, which brings the audited pair and its
/// dependencies.
#[test]
fn a_library_that_fails_to_load_or_is_not_audited_is_advised_the_install() {
    for error in [
        AvailabilityError::LoadFailed {
            library: "cuTensorNet",
            path: PathBuf::from("/usr/lib/libcutensornet.so.2"),
            message: "libcutensor.so.2: cannot open shared object file".to_string(),
        },
        AvailabilityError::UnsupportedVersion {
            component: "cuTensorNet",
            found: 21_400,
            supported: 21_300,
        },
        AvailabilityError::MissingRequiredSymbol {
            library: "cuTensorNet",
            path: PathBuf::from("/usr/lib/libcutensornet.so.2"),
            symbol: "cutensornetCreate",
        },
    ] {
        let advice = advice(&failed(error));
        assert!(advice.contains(INSTALL), "{advice}");
    }
}

/// An override that is not an absolute path to a file is advised to be
/// set right, or unset, naming the variable.
#[test]
fn an_invalid_override_is_advised_by_its_variable() {
    let advice = advice(&failed(AvailabilityError::InvalidOverride {
        variable: "QDK_CUDART_LIBRARY",
        path: PathBuf::from("libcudart.so.12"),
        reason: "not an absolute path",
    }));
    assert!(advice.contains("set QDK_CUDART_LIBRARY"), "{advice}");
}

/// A version probe that fails, and an engine that cannot run on the device,
/// point at the driver and the device (`nvidia-smi`), not at the install.
#[test]
fn a_driver_or_device_problem_points_at_nvidia_smi() {
    let probe = failed(AvailabilityError::VersionProbeFailed {
        component: "CUDA driver API",
        status: 35,
        message: "CUDA driver version is insufficient".to_string(),
    });
    let device = Diagnosis {
        engine: Some(Err(EngineError::NoDevice)),
        ..found(audited_report(), vec![a100()])
    };
    for diagnosis in [probe, device] {
        let advice = advice(&diagnosis);
        assert!(
            advice.contains("nvidia-smi") && !advice.contains(INSTALL),
            "{advice}"
        );
    }
}

/// Off Linux x86-64 the advice says where the GPU engine runs.
#[test]
fn an_unsupported_platform_is_advised_where_the_engine_runs() {
    let advice = advice(&failed(AvailabilityError::UnsupportedPlatform {
        platform: "macos aarch64".to_string(),
    }));
    assert!(advice.contains("Linux x86-64"), "{advice}");
}

/// A failed discovery reports its error, and that no engine was tried.
#[test]
fn a_failed_discovery_reports_its_error_and_no_engine() {
    let diagnosis = failed(AvailabilityError::LibraryNotFound {
        library: "cuTensorNet",
        attempted: vec![PathBuf::from("libcutensornet.so.2")],
    });
    let report = diagnosis.to_string();
    assert!(
        report.contains("cuTensorNet was not found") && report.contains("MPS engine: not tried"),
        "{report}"
    );
}

/// Device 0, where the engine runs, below compute capability 7.5 is a
/// problem naming both capabilities; a second device below it is not.
#[test]
fn a_device_0_below_compute_capability_7_5_is_a_problem() {
    let old = DeviceReport {
        compute_capability: (7, 0),
        ..a100()
    };
    let advice = advice(&found(audited_report(), vec![old]));
    assert!(advice.contains("7.0") && advice.contains("7.5"), "{advice}");
    let second = DeviceReport { ordinal: 1, ..old };
    assert!(found(audited_report(), vec![a100(), second]).is_ready());
    let turing = DeviceReport {
        compute_capability: (7, 5),
        ..a100()
    };
    assert!(found(audited_report(), vec![turing]).is_ready());
}

/// No device, or devices that could not be read, point at `nvidia-smi`.
#[test]
fn no_device_or_unreadable_devices_point_at_nvidia_smi() {
    let unreadable = Diagnosis {
        devices: Some(Err(EngineError::NoDevice)),
        ..found(audited_report(), vec![a100()])
    };
    for diagnosis in [found(audited_report(), Vec::new()), unreadable] {
        let advice = advice(&diagnosis);
        assert!(advice.contains("nvidia-smi"), "{advice}");
    }
}

/// A driver that supports no CUDA 12 is a problem naming NVIDIA's floor;
/// a driver for CUDA 12.0, the floor, is not.
#[test]
fn a_driver_below_cuda_12_is_a_problem_naming_the_floor() {
    let old = AvailabilityReport {
        cuda_driver_version: 11_080,
        ..audited_report()
    };
    let advice = advice(&found(old, vec![a100()]));
    assert!(
        advice.contains("11.8") && advice.contains("525.60.13"),
        "{advice}"
    );
    let floor = AvailabilityReport {
        cuda_driver_version: 12_000,
        ..audited_report()
    };
    assert!(found(floor, vec![a100()]).is_ready());
}

/// Another copy of a library beside the one loaded is a warning naming
/// both and the variable that chooses, and leaves the method ready.
#[test]
fn another_copy_of_a_library_is_a_warning_that_leaves_the_method_ready() {
    let report = AvailabilityReport {
        cutensornet_library: PathBuf::from("/opt/override/libcutensornet.so.2"),
        other_cutensornet_copies: vec![PathBuf::from("/env/cuquantum/lib/libcutensornet.so.2")],
        other_cuda_runtime_copies: vec![PathBuf::from("/usr/local/cuda/libcudart.so.12")],
        ..audited_report()
    };
    let diagnosis = found(report, vec![a100()]);
    assert!(diagnosis.is_ready());
    let warnings = diagnosis.warnings();
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
        diagnosis
            .to_string()
            .contains(&format!("warning: {}", warnings[0])),
        "{diagnosis}"
    );
}
