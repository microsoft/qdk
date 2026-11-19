// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the crate needs of the CUDA Runtime: the functions it looks up, the
//! probe of its versions, and the one version it accepts.

use super::function_table::assert_each_field_is_named_for_its_symbol;
use crate::tests::stand_ins::{StandInResolver, cuda_version};
use crate::{
    LibraryFault, Version,
    libraries::{
        NativeLibrary, ProbedVersions,
        cuda_runtime::{CudaFunctions, CudaRuntime, CudaVersionFunctions},
    },
};
use std::ffi::c_char;

/// Each CUDA Runtime table looks each field up by the function it is named
/// for: two functions of one signature cannot be swapped.
#[test]
fn the_cuda_runtime_tables_look_each_field_up_by_its_symbol() {
    for rows in [CudaFunctions::ROWS, CudaVersionFunctions::ROWS] {
        assert_each_field_is_named_for_its_symbol(rows, "cuda");
    }
}

static CUDA_ERROR_MESSAGE: &[u8] = b"simulated CUDA version failure\0";

unsafe extern "C" fn failing_version_probe(_version: *mut i32) -> u32 {
    42
}

unsafe extern "C" fn cuda_error_string(_status: u32) -> *const c_char {
    CUDA_ERROR_MESSAGE.as_ptr().cast()
}

/// The versions read from a CUDA Runtime whose version functions are
/// `resolver`'s.
fn probe(resolver: &StandInResolver) -> ProbedVersions {
    CudaRuntime::probe_versions(resolver)
}

/// A CUDA version call that fails, for instance as no driver is installed,
/// is a fault with CUDA's status and its text, whichever version it reads;
/// without `cudaGetErrorString`, the fault keeps the status and its text
/// names the missing function.
#[test]
fn a_failed_version_probe_keeps_cudas_status_and_message() {
    let with_text =
        || StandInResolver::audited().with("cudaGetErrorString", cuda_error_string as *const ());
    let without_text = || StandInResolver::audited().without("cudaGetErrorString");
    for (resolver, symbol, component, message) in [
        (
            with_text(),
            "cudaRuntimeGetVersion",
            "CUDA Runtime",
            "simulated CUDA version failure",
        ),
        (
            with_text(),
            "cudaDriverGetVersion",
            "CUDA driver",
            "simulated CUDA version failure",
        ),
        (
            without_text(),
            "cudaRuntimeGetVersion",
            "CUDA Runtime",
            "cudaGetErrorString",
        ),
    ] {
        let resolver = resolver.with(symbol, failing_version_probe as *const ());
        let faults = probe(&resolver).faults;
        let [
            LibraryFault::VersionProbeFailed {
                component: found,
                status: 42,
                message: text,
            },
        ] = &faults[..]
        else {
            panic!("{symbol} fails with CUDA's status: {faults:?}");
        };
        assert_eq!(*found, component);
        assert!(text.contains(message), "{symbol}: {text}");
    }
}

/// The audited CUDA Runtime is accepted, and both its version and the
/// driver's are reported.
#[test]
fn the_audited_cuda_runtime_is_accepted() {
    let probed = probe(&StandInResolver::audited());
    assert_eq!(probed.faults, []);
    assert_eq!(
        probed.versions,
        [
            Version {
                component: "CUDA Runtime",
                value: 12_090
            },
            Version {
                component: "CUDA driver",
                value: 12_090
            },
        ]
    );
}

/// The CUDA Runtime 12.9 is the audited release; 12.8 and 13.0 are not.
#[test]
fn only_cuda_runtime_12_9_is_accepted() {
    for (found, version) in [
        (12_080, cuda_version::<12_080> as *const ()),
        (13_000, cuda_version::<13_000> as *const ()),
    ] {
        let resolver = StandInResolver::audited().with("cudaRuntimeGetVersion", version);
        assert_eq!(
            probe(&resolver).faults,
            [LibraryFault::UnsupportedVersion {
                component: "CUDA Runtime",
                found,
                supported: 12_090,
            }]
        );
    }
}

/// The CUDA driver's version is reported for diagnostics only: no driver
/// is refused here.
#[test]
fn the_driver_version_is_not_checked() {
    for version in [
        cuda_version::<11_080> as *const (),
        cuda_version::<13_000> as *const (),
    ] {
        let resolver = StandInResolver::audited().with("cudaDriverGetVersion", version);
        assert_eq!(probe(&resolver).faults, []);
    }
}

/// A nonsensical version is reported as the library gave it.
#[test]
fn a_negative_version_is_reported_as_is() {
    let resolver =
        StandInResolver::audited().with("cudaRuntimeGetVersion", cuda_version::<-1> as *const ());
    assert!(matches!(
        probe(&resolver).faults[..],
        [LibraryFault::UnsupportedVersion { found: -1, .. }]
    ));
}
