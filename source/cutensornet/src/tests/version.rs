// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The versions discovery accepts.

use crate::{AvailabilityError, AvailabilityReport, version::check_versions};
use std::path::PathBuf;

/// What discovery reports on a host with the audited libraries, with a
/// CUDA driver for CUDA 12.9.
fn audited() -> AvailabilityReport {
    AvailabilityReport {
        cutensornet_library: PathBuf::from("/opt/libcutensornet.so.2"),
        cuda_runtime_library: PathBuf::from("/opt/libcudart.so.12"),
        cutensornet_version: 21_300,
        cutensornet_cuda_runtime_version: 12_090,
        cuda_runtime_version: 12_090,
        cuda_driver_version: 12_090,
    }
}

/// The audited libraries are accepted.
#[test]
fn the_audited_versions_are_accepted() {
    check_versions(&audited()).expect("every version is the audited one");
}

/// The CUDA driver's version is reported for diagnostics only: no driver
/// is refused here.
#[test]
fn the_driver_version_is_not_checked() {
    for cuda_driver_version in [11_080, 13_000] {
        check_versions(&AvailabilityReport {
            cuda_driver_version,
            ..audited()
        })
        .expect("the driver version is reported only");
    }
}

/// cuTensorNet 2.13.0 is the audited release; 2.13.1 and 2.12.0 are not.
#[test]
fn only_cutensornet_2_13_0_is_accepted() {
    for found in [21_301, 21_200] {
        let report = AvailabilityReport {
            cutensornet_version: found,
            ..audited()
        };
        assert!(matches!(
            check_versions(&report),
            Err(AvailabilityError::UnsupportedVersion {
                component: "cuTensorNet",
                found: reported,
                supported: 21_300,
            }) if reported == i128::from(u32::try_from(found).expect("small"))
        ));
    }
}

/// The CUDA Runtime 12.9 is the audited release; 12.8 and 13.0 are not.
#[test]
fn only_cuda_runtime_12_9_is_accepted() {
    for found in [12_080, 13_000] {
        let report = AvailabilityReport {
            cuda_runtime_version: found,
            ..audited()
        };
        assert!(matches!(
            check_versions(&report),
            Err(AvailabilityError::UnsupportedVersion {
                component: "CUDA Runtime",
                found: reported,
                supported: 12_090,
            }) if reported == i128::from(found)
        ));
    }
}

/// A cuTensorNet 2.13.0 built for the CUDA Runtime 12.8 is refused, even
/// with the CUDA Runtime 12.9 loaded.
#[test]
fn cutensornet_must_be_built_against_cuda_runtime_12_9() {
    let report = AvailabilityReport {
        cutensornet_cuda_runtime_version: 12_080,
        ..audited()
    };
    assert!(matches!(
        check_versions(&report),
        Err(AvailabilityError::UnsupportedVersion {
            component: "cuTensorNet's CUDA Runtime",
            found: 12_080,
            supported: 12_090,
        })
    ));
}

/// A nonsensical version is reported as the library gave it.
#[test]
fn a_negative_version_is_reported_as_is() {
    let report = AvailabilityReport {
        cuda_runtime_version: -1,
        ..audited()
    };
    assert!(matches!(
        check_versions(&report),
        Err(AvailabilityError::UnsupportedVersion { found: -1, .. })
    ));
}
