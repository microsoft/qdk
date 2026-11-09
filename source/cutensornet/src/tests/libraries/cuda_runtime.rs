// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the crate needs of the CUDA Runtime: the functions it looks up, the
//! probe of its versions, and the one version it accepts.

use crate::tests::stand_ins::{StandInResolver, cuda_version};
use crate::{
    LibraryFault, Version,
    libraries::loader::FunctionTable,
    libraries::{
        NativeLibrary, ProbedVersions,
        cuda_runtime::{CudaFunctions, CudaRuntime},
    },
};
use std::ffi::c_char;

/// The CUDA Runtime table's rows, as `(field, symbol)`, each required.
const CUDA_RUNTIME_ROWS: &[(&str, &str)] = &[
    ("runtime_get_version", "cudaRuntimeGetVersion"),
    ("driver_get_version", "cudaDriverGetVersion"),
    ("get_device_count", "cudaGetDeviceCount"),
    ("set_device", "cudaSetDevice"),
    ("device_get_attribute", "cudaDeviceGetAttribute"),
    ("get_error_string", "cudaGetErrorString"),
    ("mem_get_info", "cudaMemGetInfo"),
    ("malloc", "cudaMalloc"),
    ("free", "cudaFree"),
    ("memcpy", "cudaMemcpy"),
    ("stream_create_with_flags", "cudaStreamCreateWithFlags"),
    ("stream_synchronize", "cudaStreamSynchronize"),
    ("stream_destroy", "cudaStreamDestroy"),
];

fn sorted<T: Ord>(mut rows: Vec<T>) -> Vec<T> {
    rows.sort_unstable();
    rows
}

/// The CUDA Runtime table looks each field up by its own symbol: two
/// functions of one signature cannot be swapped.
#[test]
fn the_cuda_runtime_table_looks_each_field_up_by_its_symbol() {
    let expected = CUDA_RUNTIME_ROWS
        .iter()
        .map(|&(field, symbol)| (field, symbol, true))
        .collect();
    assert_eq!(sorted(CudaFunctions::ROWS.to_vec()), sorted(expected));
}

/// An installation whose CUDA Runtime lacks required functions is refused,
/// naming each one.
#[test]
fn every_missing_required_cuda_function_is_reported() {
    let resolver = CUDA_RUNTIME_ROWS
        .iter()
        .fold(StandInResolver::audited(), |resolver, &(_, symbol)| {
            resolver.without(symbol)
        });
    let missing = CudaFunctions::resolve(&resolver)
        .err()
        .expect("every required function is missing");
    let expected: Vec<_> = CUDA_RUNTIME_ROWS
        .iter()
        .map(|&(_, symbol)| LibraryFault::MissingRequiredSymbol { symbol })
        .collect();
    assert_eq!(missing, expected);
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
/// is a fault with CUDA's status and its text, whichever version it reads.
#[test]
fn a_failed_version_probe_keeps_cudas_status_and_message() {
    for (symbol, component) in [
        ("cudaRuntimeGetVersion", "CUDA Runtime"),
        ("cudaDriverGetVersion", "CUDA driver"),
    ] {
        let resolver = StandInResolver::audited()
            .with(symbol, failing_version_probe as *const ())
            .with("cudaGetErrorString", cuda_error_string as *const ());
        assert_eq!(
            probe(&resolver).faults,
            [LibraryFault::VersionProbeFailed {
                component,
                status: 42,
                message: "simulated CUDA version failure".to_string(),
            }]
        );
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
