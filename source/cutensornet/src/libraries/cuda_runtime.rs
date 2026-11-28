// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime, NVIDIA's API to the GPU, on which cuTensorNet runs.
//!
//! This module holds what discovery and the setup check's library steps
//! assume of the CUDA Runtime. Its `NativeLibrary` says where it is looked
//! for, the functions looked up in it (`CudaFunctions`), and its versions.
//! Its versions are `CUDA Runtime`, which must be 12.9, and `CUDA driver`,
//! reported only. Its `LibrarySetupGuide` says what the setup check shows
//! of it and advises. CUDA encodes a version as `1000·major + 10·minor`
//! (`12090` is 12.9).
//!
//! One file per family of the table's functions declares their
//! function-pointer types (for instance `memory`: free memory, allocation,
//! release and copies). Each type is named as its function, as the table's
//! fields are.
//!
//! `cudart_12`, in `generated/`, declares the CUDA Runtime 12's API: the
//! functions, types and constants of its header. A function pointer is only
//! as right as the C signature it was declared with. `cudart_12` is
//! bindgen's output of `scripts/generate_bindings.py`, from what
//! `bindings.toml` in this module's folder pins. It also declares the CUDA
//! stream type that cuTensorNet's bindings import. It declares the header's
//! version too: the crate does not compile unless it is the accepted one.

#![allow(
    non_camel_case_types,
    reason = "each function-pointer type is named as its C function"
)]

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod device;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod error_strings;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod memory;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod stream;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod versions;

/// The output of `scripts/generate_bindings.py`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
mod generated {
    #[allow(
        clippy::all,
        clippy::pedantic,
        dead_code,
        rustdoc::all,
        non_camel_case_types,
        non_snake_case,
        non_upper_case_globals,
        unused_imports,
        reason = "bindgen's output, not reviewed, keeps NVIDIA's C identifiers and documentation"
    )]
    pub(crate) mod cudart_12;
}
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) use generated::cudart_12;

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use super::{
    NativeLibrary, ProbedVersions,
    setup_guide::{DriverFloor, LibrarySetupGuide, cuda_version, or_unread, path_or_unread},
};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use crate::{
    LibraryFault, LibraryReport,
    libraries::function_table::{FunctionTable, SymbolResolver},
};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use std::ffi::CStr;

/// The CUDA Runtime library (libcudart): where its file is looked for, the
/// version it must have, and the tables of the functions looked up in it.
///
/// [`discover`](crate::discover) opens it first, through
/// [`LoadedLibraries::open`](super::LoadedLibraries::open), as cuTensorNet
/// runs on it. Discovery's steps are in docs/loading.md, "The steps of
/// discovery".
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) struct CudaRuntime;

/// The driver CUDA 12 needs: NVIDIA's floor for CUDA 12, 525.60.13 on
/// Linux. The QDK adds none.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
const DRIVER_FLOOR: DriverFloor = DriverFloor {
    cuda: 12_000,
    release: "525.60.13",
};

/// The driver version CUDA reports, with success, when no NVIDIA driver is
/// installed (`cudaDriverGetVersion`'s documentation).
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
const NO_DRIVER: i128 = 0;

/// The CUDA Runtime 12.9, the release the bindings were audited against.
/// cuTensorNet must also have been built against it.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(super) const AUDITED_VERSION: i32 = 12_090;

// The bindings must come from the audited release's header, which declares
// its version. A new header changes the bindings, which must be audited
// before this version is changed to accept it.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
const _: () = assert!(
    cudart_12::CUDART_VERSION.cast_signed() == AUDITED_VERSION,
    "the bindings come from another CUDA Runtime release than the audited one"
);

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl NativeLibrary for CudaRuntime {
    const NAME: &'static str = "CUDA Runtime";
    const VARIABLE: &'static str = "QDK_CUDART_LIBRARY";
    const FILE_NAME: &'static str = "libcudart.so.12";
    const DEFAULTS: &'static [&'static str] = &[
        // Where NVIDIA's apt package installs it. The path spells out
        // `FILE_NAME`, as a constant cannot be built from another in Rust.
        "/usr/local/cuda-12.9/targets/x86_64-linux/lib/libcudart.so.12",
        Self::FILE_NAME,
    ];
    type Functions = CudaFunctions;

    /// Reads the CUDA Runtime's version, which must be 12.9, and the
    /// driver's, which is not checked. Each is read when its function is
    /// found, whether or not the other functions are.
    fn probe_versions<R: SymbolResolver>(resolver: &R) -> ProbedVersions {
        let mut probed = ProbedVersions::default();
        let functions = CudaVersionFunctions::resolve(resolver)
            .expect("a table of optional rows is resolved whatever is missing");
        if let Some(runtime_get_version) = functions.cudaRuntimeGetVersion {
            let runtime = probe_version(
                "CUDA Runtime",
                runtime_get_version,
                functions.cudaGetErrorString,
            );
            match runtime {
                Ok(found) => probed.audited("CUDA Runtime", found.into(), AUDITED_VERSION.into()),
                Err(fault) => probed.faults.push(fault),
            }
        }
        if let Some(driver_get_version) = functions.cudaDriverGetVersion {
            let driver = probe_version(
                "CUDA driver",
                driver_get_version,
                functions.cudaGetErrorString,
            );
            match driver {
                Ok(found) => probed.reported("CUDA driver", found.into()),
                Err(fault) => probed.faults.push(fault),
            }
        }
        probed
    }
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl LibrarySetupGuide for CudaRuntime {
    const INSTALL: &'static str = "the CUDA Runtime 12.9";
    const MINIMUM_DRIVER: Option<DriverFloor> = Some(DRIVER_FLOOR);

    /// CUDA's version probe fails mostly when the driver is too old.
    fn version_probe_advice() -> Option<String> {
        Some(format!(
            "CUDA could not report its version, usually because the NVIDIA driver is too old: \
             check `nvidia-smi`, and install {DRIVER_FLOOR}"
        ))
    }

    /// Its path and version, and the CUDA the driver supports, or that
    /// there is no driver.
    fn describe(report: &LibraryReport) -> Vec<String> {
        let driver = match report.version("CUDA driver") {
            Some(NO_DRIVER) => "none".to_string(),
            driver => format!("supports CUDA {}", or_unread(driver.map(cuda_version))),
        };
        vec![
            format!(
                "CUDA Runtime: {} ({})",
                path_or_unread(report),
                or_unread(report.version("CUDA Runtime").map(cuda_version)),
            ),
            format!("CUDA driver: {driver}"),
        ]
    }

    /// No NVIDIA driver, or one that supports no CUDA of its floor.
    fn host_problem(report: &LibraryReport) -> Option<String> {
        match report.version("CUDA driver")? {
            NO_DRIVER => Some(format!(
                "no NVIDIA driver was found: install {DRIVER_FLOOR}"
            )),
            driver if driver < DRIVER_FLOOR.cuda => Some(format!(
                "the NVIDIA driver supports CUDA {} only: install {DRIVER_FLOOR}",
                cuda_version(driver)
            )),
            _ => None,
        }
    }
}

/// `cudaStreamLegacy`, `((cudaStream_t)0x1)`: the legacy default stream,
/// which `cudaMemcpy` runs on. `cudart_12` lacks it, as bindgen does not
/// translate a macro that casts.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) const STREAM_LEGACY: usize = 1;

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
super::function_table::function_table! {
    /// Every CUDA Runtime function discovery looks up.
    #[expect(
        dead_code,
        reason = "the version rows are only looked up, so that a missing one is reported (the versions are read through CudaVersionFunctions)"
    )]
    struct CudaFunctions, declared in cudart_12 {
        required versions::cudaRuntimeGetVersion,
        required versions::cudaDriverGetVersion,
        required device::cudaGetDeviceCount,
        required device::cudaSetDevice,
        required device::cudaDeviceGetAttribute,
        required error_strings::cudaGetErrorString,
        required memory::cudaMemGetInfo,
        required memory::cudaMalloc,
        required memory::cudaFree,
        required memory::cudaMemcpy,
        required stream::cudaStreamCreateWithFlags,
        required stream::cudaStreamSynchronize,
        required stream::cudaStreamDestroy,
    }
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
super::function_table::function_table! {
    /// The functions that read the versions, each optional, looked up on
    /// their own (`NativeLibrary::probe_versions` says why). The main table
    /// reports those missing.
    struct CudaVersionFunctions, declared in cudart_12 {
        optional versions::cudaRuntimeGetVersion,
        optional versions::cudaDriverGetVersion,
        optional error_strings::cudaGetErrorString,
    }
}

/// Calls a CUDA version function, `cudaRuntimeGetVersion` or
/// `cudaDriverGetVersion`, and returns the version it writes, or its failure
/// with CUDA's status and text (when `cudaGetErrorString` was found).
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn probe_version(
    component: &'static str,
    probe: versions::VersionProbeFn,
    get_error_string: Option<error_strings::cudaGetErrorString>,
) -> Result<i32, LibraryFault> {
    let mut version = 0;
    // SAFETY: `version` is a valid writable `int` and both pointers were
    // resolved with their exact audited signatures from a retained library.
    let status = unsafe { probe(&raw mut version) };
    if status == cudart_12::cudaError_cudaSuccess {
        return Ok(version);
    }
    let Some(get_error_string) = get_error_string else {
        return Err(LibraryFault::VersionProbeFailed {
            component,
            status,
            message: "no text: cudaGetErrorString is missing".to_string(),
        });
    };
    // SAFETY: CUDA owns the returned null-terminated string; it is copied
    // immediately and never retained as a borrowed pointer.
    let message_ptr = unsafe { get_error_string(status) };
    let message = if message_ptr.is_null() {
        "CUDA returned a null error string".to_string()
    } else {
        // SAFETY: non-null CUDA error strings are documented as
        // null-terminated and library-owned.
        unsafe { CStr::from_ptr(message_ptr) }
            .to_string_lossy()
            .into_owned()
    };
    Err(LibraryFault::VersionProbeFailed {
        component,
        status,
        message,
    })
}

#[cfg(all(test, unix))]
mod tests;
