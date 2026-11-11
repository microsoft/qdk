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

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use super::BoundLibrary;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use super::{
    NativeLibrary, PackagedLibraries, ProbedVersions, copy_message, setup_guide::or_unread,
};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use crate::{
    LibraryFault, LibraryReport,
    libraries::loader::{FunctionTable, SymbolResolver},
};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use std::path::Path;

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

/// The CUDA Runtime's calls that both the MPS engine and the setup check's
/// device reads make, and the check of a status every CUDA call returns:
/// one implementation each, on the bound library, so the engine and the
/// device reads share it rather than one calling through the other.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl BoundLibrary<CudaRuntime> {
    /// `Ok` when `status`, which `operation` returned, is CUDA's success;
    /// or else the failure, with CUDA's text for the status.
    pub(crate) fn check(
        &self,
        operation: &'static str,
        status: cudart_12::cudaError_t,
    ) -> Result<(), CudaCallFailed> {
        if status == cudart_12::cudaError_cudaSuccess {
            Ok(())
        } else {
            Err(CudaCallFailed {
                operation,
                status,
                message: error_message(self.functions.get_error_string, status),
            })
        }
    }

    /// How many CUDA devices the process sees (`cudaGetDeviceCount`).
    pub(crate) fn device_count(&self) -> Result<i32, CudaCallFailed> {
        let mut count = 0;
        // SAFETY: `count` is a valid writable CUDA `int` out-parameter.
        let status = unsafe { (self.functions.get_device_count)(&raw mut count) };
        self.check("cudaGetDeviceCount", status)?;
        Ok(count)
    }

    /// Makes device `ordinal` current on the calling thread
    /// (`cudaSetDevice`).
    pub(crate) fn set_device(&self, ordinal: i32) -> Result<(), CudaCallFailed> {
        // SAFETY: the call takes no pointer, and CUDA checks the ordinal,
        // returning an error for an invalid one.
        let status = unsafe { (self.functions.set_device)(ordinal) };
        self.check("cudaSetDevice", status)
    }

    /// The current device's free and total memory, in bytes
    /// (`cudaMemGetInfo`).
    pub(crate) fn memory_info(&self) -> Result<(usize, usize), CudaCallFailed> {
        let mut free = 0;
        let mut total = 0;
        // SAFETY: both arguments are valid writable `size_t` out-pointers.
        let status = unsafe { (self.functions.mem_get_info)(&raw mut free, &raw mut total) };
        self.check("cudaMemGetInfo", status)?;
        Ok((free, total))
    }
}

/// A CUDA Runtime call that returned an error status: the call, the status,
/// and CUDA's text for it. Its caller turns it into its own error.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) struct CudaCallFailed {
    pub(crate) operation: &'static str,
    pub(crate) status: cudart_12::cudaError_t,
    pub(crate) message: String,
}

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
    Err(LibraryFault::VersionProbeFailed {
        component,
        status,
        message: error_message(get_error_string, status),
    })
}

/// CUDA's text for `status`, read with `get_error_string`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn error_message(get_error_string: GetErrorStringFn, status: cudart_12::cudaError_t) -> String {
    // SAFETY: the pointer was resolved with the audited signature from a
    // retained library, and CUDA returns null or a null-terminated string it
    // owns, which is copied before this call returns.
    unsafe {
        copy_message(
            get_error_string(status),
            "CUDA returned a null error string",
        )
    }
}
