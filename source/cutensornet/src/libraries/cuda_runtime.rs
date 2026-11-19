// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime, NVIDIA's API to the GPU, on which cuTensorNet runs.
//!
//! What discovery and the setup check's library steps assume of the CUDA
//! Runtime is in this module: as its `NativeLibrary`, where it is looked for,
//! the functions looked up in it (`CudaFunctions`), and its versions
//! (`CUDA Runtime`, which must be 12.9, and `CUDA driver`, reported only); as
//! its `LibrarySetupGuide`, what the setup check shows of it and advises. CUDA
//! encodes a version as `1000·major + 10·minor` (`12090` is 12.9). This module
//! also declares the function-pointer types of the table's rows.
//!
//! `cudart_12`, in `generated/`, declares the CUDA Runtime 12's API, the
//! functions, types and constants of its header: a function pointer is only
//! as right as the C signature it was declared with. It is bindgen's output of
//! `scripts/generate_bindings.py`, from what `bindings.toml` in this module's
//! folder pins; it also declares the CUDA stream type that cuTensorNet's
//! bindings import, and the header's version, which must be the accepted
//! one: the crate does not compile otherwise.

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
    NativeLibrary, ProbedVersions, setup_guide::LibrarySetupGuide, setup_guide::or_unread,
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
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) struct CudaRuntime;

/// The oldest CUDA a driver must support, as CUDA encodes it: 12.0. The
/// floor is NVIDIA's for CUDA 12 ([`cuda_12_driver!`]); the QDK adds
/// none.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
const MINIMUM_DRIVER_CUDA: i128 = 12_000;

/// The driver version CUDA reports, with success, when no NVIDIA driver is
/// installed (`cudaDriverGetVersion`'s documentation).
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
const NO_DRIVER: i128 = 0;

/// The NVIDIA driver CUDA 12 needs, NVIDIA's 525.60.13 or later, as every
/// piece of advice that asks for it writes it. A macro rather than a
/// `const`, since `concat!` builds the constant advice texts from it and
/// takes literals only.
macro_rules! cuda_12_driver {
    () => {
        "a driver for CUDA 12 (525.60.13 or later)"
    };
}
pub(crate) use cuda_12_driver;

/// The CUDA Runtime 12.9, the release the bindings were audited against.
/// cuTensorNet must also have been built against it.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(super) const AUDITED_VERSION: i32 = 12_090;

// The bindings must come from the audited release's header, which declares
// its version: a new header changes the bindings, which must be audited
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
        if let Some(runtime_get_version) = functions.runtime_get_version {
            let runtime = probe_version(
                "CUDA Runtime",
                runtime_get_version,
                functions.get_error_string,
            );
            match runtime {
                Ok(found) => probed.audited("CUDA Runtime", found.into(), AUDITED_VERSION.into()),
                Err(fault) => probed.faults.push(fault),
            }
        }
        if let Some(driver_get_version) = functions.driver_get_version {
            let driver = probe_version(
                "CUDA driver",
                driver_get_version,
                functions.get_error_string,
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
    const VERSION_PROBE_ADVICE: Option<&'static str> = Some(concat!(
        "CUDA could not report its version, usually because the NVIDIA driver is too old: check \
         `nvidia-smi`, and install ",
        cuda_12_driver!()
    ));

    /// Its path and version, and the CUDA the driver supports, or that
    /// there is no driver.
    #[expect(
        clippy::unnecessary_debug_formatting,
        reason = "paths are quoted, as in the faults, so a line break adds no line"
    )]
    fn describe(report: &LibraryReport) -> Vec<String> {
        let driver = match report.version("CUDA driver") {
            Some(NO_DRIVER) => "none".to_string(),
            driver => format!("supports CUDA {}", or_unread(driver.map(cuda_version))),
        };
        vec![
            format!(
                "CUDA Runtime: {} ({})",
                or_unread(report.path.as_ref().map(|path| format!("{path:?}"))),
                or_unread(report.version("CUDA Runtime").map(cuda_version)),
            ),
            format!("CUDA driver: {driver}"),
        ]
    }

    /// No NVIDIA driver, or one that supports no CUDA 12.
    fn host_problem(report: &LibraryReport) -> Option<String> {
        match report.version("CUDA driver")? {
            NO_DRIVER => {
                Some(concat!("no NVIDIA driver was found: install ", cuda_12_driver!()).to_string())
            }
            driver if driver < MINIMUM_DRIVER_CUDA => Some(format!(
                concat!(
                    "the NVIDIA driver supports CUDA {} only: install ",
                    cuda_12_driver!()
                ),
                cuda_version(driver)
            )),
            _ => None,
        }
    }
}

/// CUDA's `1000·major + 10·minor` as `major.minor`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(super) fn cuda_version(version: i128) -> String {
    format!("{}.{}", version / 1000, version % 1000 / 10)
}

/// `cudaStreamLegacy`, `((cudaStream_t)0x1)`: the legacy default stream,
/// which `cudaMemcpy` runs on. bindgen does not translate a macro that casts,
/// so `cudart_12` lacks it.
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
        required runtime_get_version: versions::RuntimeGetVersionFn = cudaRuntimeGetVersion,
        required driver_get_version: versions::DriverGetVersionFn = cudaDriverGetVersion,
        required get_device_count: device::GetDeviceCountFn = cudaGetDeviceCount,
        required set_device: device::SetDeviceFn = cudaSetDevice,
        required device_get_attribute: device::DeviceGetAttributeFn =
            cudaDeviceGetAttribute,
        required get_error_string: error_strings::GetErrorStringFn = cudaGetErrorString,
        required mem_get_info: memory::MemGetInfoFn = cudaMemGetInfo,
        required malloc: memory::MallocFn = cudaMalloc,
        required free: memory::FreeFn = cudaFree,
        required memcpy: memory::MemcpyFn = cudaMemcpy,
        required stream_create_with_flags: stream::StreamCreateWithFlagsFn =
            cudaStreamCreateWithFlags,
        required stream_synchronize: stream::StreamSynchronizeFn = cudaStreamSynchronize,
        required stream_destroy: stream::StreamDestroyFn = cudaStreamDestroy,
    }
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
super::function_table::function_table! {
    /// The functions that read the versions, looked up on their own, and
    /// each optional, so that each version is read even when other
    /// functions are missing; the main table reports those missing.
    struct CudaVersionFunctions, declared in cudart_12 {
        optional runtime_get_version: versions::RuntimeGetVersionFn = cudaRuntimeGetVersion,
        optional driver_get_version: versions::DriverGetVersionFn = cudaDriverGetVersion,
        optional get_error_string: error_strings::GetErrorStringFn = cudaGetErrorString,
    }
}

/// Calls a CUDA version function, `cudaRuntimeGetVersion` or
/// `cudaDriverGetVersion`, and returns the version it writes, or its failure
/// with CUDA's status and text (when `cudaGetErrorString` was found).
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn probe_version(
    component: &'static str,
    probe: versions::VersionProbeFn,
    get_error_string: Option<error_strings::GetErrorStringFn>,
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
