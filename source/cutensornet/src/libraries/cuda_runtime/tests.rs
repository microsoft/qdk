// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the crate needs of the CUDA Runtime: the probe of its versions, and
//! the one version it accepts.

use crate::test_utils::stand_ins::{StandInResolver, cuda_version};
use crate::{
    LibraryFault, Version,
    libraries::{NativeLibrary, ProbedVersions, cuda_runtime::CudaRuntime},
};
use std::ffi::c_char;

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
/// Without `cudaGetErrorString`, the fault keeps the status, and its text
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

/// The CUDA Runtime 12.9 is the audited release, and is accepted. 12.8 and
/// 13.0 are not, nor a nonsensical version, which is reported as the
/// library gave it.
#[test]
fn only_cuda_runtime_12_9_is_accepted() {
    for (found, version) in [
        (12_090, cuda_version::<12_090> as *const ()),
        (12_080, cuda_version::<12_080> as *const ()),
        (13_000, cuda_version::<13_000> as *const ()),
        (-1, cuda_version::<-1> as *const ()),
    ] {
        let resolver = StandInResolver::audited().with("cudaRuntimeGetVersion", version);
        let probed = probe(&resolver);
        let expected = if found == 12_090 {
            vec![]
        } else {
            vec![LibraryFault::UnsupportedVersion {
                component: "CUDA Runtime",
                found,
                supported: 12_090,
            }]
        };
        assert_eq!(probed.faults, expected);
        assert!(
            probed.versions.contains(&Version {
                component: "CUDA Runtime",
                value: found
            }),
            "{found} is reported: {:?}",
            probed.versions
        );
    }
}

/// The CUDA driver's version is reported for diagnostics only, as the
/// library gave it: no driver is refused here.
#[test]
fn the_driver_version_is_not_checked() {
    for (found, version) in [
        (11_080, cuda_version::<11_080> as *const ()),
        (13_000, cuda_version::<13_000> as *const ()),
    ] {
        let resolver = StandInResolver::audited().with("cudaDriverGetVersion", version);
        let probed = probe(&resolver);
        assert_eq!(probed.faults, []);
        assert!(
            probed.versions.contains(&Version {
                component: "CUDA driver",
                value: found
            }),
            "{found} is reported: {:?}",
            probed.versions
        );
    }
}
