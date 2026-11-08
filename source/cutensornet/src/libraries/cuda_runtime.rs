// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime, NVIDIA's API to the GPU, on which cuTensorNet runs.
//!
//! Everything the crate assumes of the CUDA Runtime is in this module, as its
//! `NativeLibrary`: where it is looked for, the functions looked up in it
//! (`CudaFunctions`), and its versions: `CUDA Runtime`, which must be 12.9,
//! and `CUDA driver`, reported only. CUDA encodes a version as
//! `1000·major + 10·minor` (`12090` is 12.9). This module also declares the
//! function-pointer types of the table's rows.
//!
//! `cudart_12`, in `generated/`, declares the CUDA Runtime 12's API, the
//! functions, types and constants of its header: a function pointer is only
//! as right as the C signature it was declared with. It is bindgen's output of
//! `scripts/generate_bindings.py`, from what `bindings.toml` in this module's
//! folder pins; it also declares the CUDA stream type that cuTensorNet's
//! bindings import, and the header's version, which must be the accepted
//! one: the crate does not compile otherwise. The table is the list of the
//! functions the crate uses; the bindings declare the whole API, so binding
//! another function is its row here and its type below, which the crate
//! checks, when it compiles, against the function's declaration in
//! `cudart_12`.

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
use super::{NativeLibrary, ProbedVersions};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use crate::{
    LibraryFault, LibraryReport, PackagedLibraries,
    diagnosis::or_unread,
    libraries::loader::{FunctionTable, SymbolResolver},
};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use std::{ffi::CStr, path::Path};

/// The CUDA Runtime.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) struct CudaRuntime;

/// The oldest CUDA a driver must support, as CUDA encodes it: 12.0. The
/// floor is NVIDIA's for CUDA 12 (driver [`minimum_driver!`]); the QDK adds
/// none.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
const MINIMUM_DRIVER_CUDA: i128 = 12_000;

/// The oldest NVIDIA driver for CUDA 12, `525.60.13`, as every piece of
/// advice that names it writes it. A macro rather than a `const`, since
/// `concat!` builds the constant advice texts from it and takes literals
/// only.
macro_rules! minimum_driver {
    () => {
        "525.60.13"
    };
}
pub(crate) use minimum_driver;

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
    const DEFAULTS: &'static [&'static str] = &[
        "/usr/local/cuda-12.9/targets/x86_64-linux/lib/libcudart.so.12",
        "libcudart.so.12",
    ];
    const INSTALL: &'static str = "the CUDA Runtime 12.9";
    const VERSION_PROBE_ADVICE: Option<&'static str> = Some(concat!(
        "CUDA could not report its version, usually because the NVIDIA driver is missing or \
         older than CUDA 12 needs (",
        minimum_driver!(),
        "): check `nvidia-smi`"
    ));
    type Functions = CudaFunctions;

    fn packaged(packaged: &PackagedLibraries) -> Option<&Path> {
        packaged.cuda_runtime.as_deref()
    }

    /// Reads the CUDA Runtime's version, which must be 12.9, and the
    /// driver's, which is not checked.
    fn probe_versions<R: SymbolResolver>(resolver: &R) -> ProbedVersions {
        let mut probed = ProbedVersions::default();
        let Ok(functions) = CudaVersionFunctions::resolve(resolver) else {
            return probed;
        };
        let runtime = probe_version(
            "CUDA Runtime",
            functions.runtime_get_version,
            functions.get_error_string,
        );
        match runtime {
            Ok(found) => probed.audited("CUDA Runtime", found.into(), AUDITED_VERSION.into()),
            Err(fault) => probed.faults.push(fault),
        }
        let driver = probe_version(
            "CUDA driver",
            functions.driver_get_version,
            functions.get_error_string,
        );
        match driver {
            Ok(found) => probed.reported("CUDA driver", found.into()),
            Err(fault) => probed.faults.push(fault),
        }
        probed
    }

    /// Its path and version, and the CUDA the driver supports.
    fn describe(report: &LibraryReport) -> Vec<String> {
        vec![
            format!(
                "CUDA Runtime: {} ({})",
                or_unread(report.path.as_ref().map(|path| path.display())),
                or_unread(report.version("CUDA Runtime").map(cuda_version)),
            ),
            format!(
                "CUDA driver: supports CUDA {}",
                or_unread(report.version("CUDA driver").map(cuda_version))
            ),
        ]
    }

    /// A driver that supports no CUDA 12.
    fn host_problem(report: &LibraryReport) -> Option<String> {
        let driver = report.version("CUDA driver")?;
        (driver < MINIMUM_DRIVER_CUDA).then(|| {
            format!(
                concat!(
                    "the NVIDIA driver supports CUDA {} only: install a driver for CUDA 12 (",
                    minimum_driver!(),
                    " or later)"
                ),
                cuda_version(driver)
            )
        })
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
super::loader::function_table! {
    /// Every CUDA Runtime function discovery looks up.
    #[expect(
        dead_code,
        reason = "the version rows are only looked up, so that a missing one is reported (the versions are read through CudaVersionFunctions)"
    )]
    struct CudaFunctions, declared in cudart_12 {
        required runtime_get_version: RuntimeGetVersionFn = cudaRuntimeGetVersion,
        required driver_get_version: DriverGetVersionFn = cudaDriverGetVersion,
        required get_device_count: GetDeviceCountFn = cudaGetDeviceCount,
        required set_device: SetDeviceFn = cudaSetDevice,
        required device_get_attribute: DeviceGetAttributeFn =
            cudaDeviceGetAttribute,
        required get_error_string: GetErrorStringFn = cudaGetErrorString,
        required mem_get_info: MemGetInfoFn = cudaMemGetInfo,
        required malloc: MallocFn = cudaMalloc,
        required free: FreeFn = cudaFree,
        required memcpy: MemcpyFn = cudaMemcpy,
        required stream_create_with_flags: StreamCreateWithFlagsFn =
            cudaStreamCreateWithFlags,
        required stream_synchronize: StreamSynchronizeFn = cudaStreamSynchronize,
        required stream_destroy: StreamDestroyFn = cudaStreamDestroy,
    }
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
super::loader::function_table! {
    /// The functions that read the versions, looked up on their own so
    /// that the versions are read even when other functions are missing.
    struct CudaVersionFunctions, declared in cudart_12 {
        required runtime_get_version: RuntimeGetVersionFn = cudaRuntimeGetVersion,
        required driver_get_version: DriverGetVersionFn = cudaDriverGetVersion,
        required get_error_string: GetErrorStringFn = cudaGetErrorString,
    }
}

/// Function pointer for `cudaRuntimeGetVersion`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type RuntimeGetVersionFn =
    unsafe extern "C" fn(*mut ::std::os::raw::c_int) -> cudart_12::cudaError_t;

/// Function pointer for `cudaDriverGetVersion`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type DriverGetVersionFn =
    unsafe extern "C" fn(*mut ::std::os::raw::c_int) -> cudart_12::cudaError_t;

/// Function pointer for `cudaGetDeviceCount`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type GetDeviceCountFn =
    unsafe extern "C" fn(*mut ::std::os::raw::c_int) -> cudart_12::cudaError_t;

/// Function pointer for `cudaSetDevice`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type SetDeviceFn = unsafe extern "C" fn(::std::os::raw::c_int) -> cudart_12::cudaError_t;

/// Function pointer for `cudaDeviceGetAttribute`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type DeviceGetAttributeFn = unsafe extern "C" fn(
    *mut ::std::os::raw::c_int,
    cudart_12::cudaDeviceAttr,
    ::std::os::raw::c_int,
) -> cudart_12::cudaError_t;

/// Function pointer for `cudaGetErrorString`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type GetErrorStringFn =
    unsafe extern "C" fn(cudart_12::cudaError_t) -> *const ::std::os::raw::c_char;

/// Function pointer for `cudaMemGetInfo`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type MemGetInfoFn =
    unsafe extern "C" fn(*mut usize, *mut usize) -> cudart_12::cudaError_t;

/// Function pointer for `cudaMalloc`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type MallocFn =
    unsafe extern "C" fn(*mut *mut ::std::os::raw::c_void, usize) -> cudart_12::cudaError_t;

/// Function pointer for `cudaFree`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type FreeFn =
    unsafe extern "C" fn(*mut ::std::os::raw::c_void) -> cudart_12::cudaError_t;

/// Function pointer for `cudaMemcpy`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type MemcpyFn = unsafe extern "C" fn(
    *mut ::std::os::raw::c_void,
    *const ::std::os::raw::c_void,
    usize,
    cudart_12::cudaMemcpyKind,
) -> cudart_12::cudaError_t;

/// Function pointer for `cudaStreamCreateWithFlags`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type StreamCreateWithFlagsFn = unsafe extern "C" fn(
    *mut cudart_12::cudaStream_t,
    ::std::os::raw::c_uint,
) -> cudart_12::cudaError_t;

/// Function pointer for `cudaStreamSynchronize`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type StreamSynchronizeFn =
    unsafe extern "C" fn(cudart_12::cudaStream_t) -> cudart_12::cudaError_t;

/// Function pointer for `cudaStreamDestroy`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) type StreamDestroyFn =
    unsafe extern "C" fn(cudart_12::cudaStream_t) -> cudart_12::cudaError_t;

/// Calls a CUDA version function, `cudaRuntimeGetVersion` or
/// `cudaDriverGetVersion`, and returns the version it writes, or its failure
/// with CUDA's status and text.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn probe_version(
    component: &'static str,
    probe: unsafe extern "C" fn(*mut i32) -> cudart_12::cudaError_t,
    get_error_string: GetErrorStringFn,
) -> Result<i32, LibraryFault> {
    let mut version = 0;
    // SAFETY: `version` is a valid writable `int` and both pointers were
    // resolved with their exact audited signatures from a retained library.
    let status = unsafe { probe(&raw mut version) };
    if status == cudart_12::cudaError_cudaSuccess {
        return Ok(version);
    }
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
