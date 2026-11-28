// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet, NVIDIA's tensor-network library.
//!
//! This module's `NativeLibrary` holds everything the crate assumes of
//! cuTensorNet: where it is looked for, the functions looked up in it
//! (`CuTensorNetFunctions`), and its versions. Its versions are
//! `cuTensorNet`, which must be 2.13.0, and `cuTensorNet's CUDA Runtime`,
//! the CUDA Runtime it was built against, which must be 12.9. cuTensorNet
//! encodes its version as `10000·major + 100·minor + patch` (`21300` is
//! 2.13.0). It encodes a CUDA Runtime version as CUDA does,
//! `1000·major + 10·minor` (`12090` is 12.9).
//!
//! One file per family of the table's functions declares their
//! function-pointer types (for instance `decomposition`: SVD, gate split,
//! QR). Each type is named as its function, as the table's fields are.
//!
//! `v2_13`, in `generated/`, declares cuTensorNet 2.13's API: the
//! functions, types and constants of its header. A function pointer is only
//! as right as the C signature it was declared with. `v2_13` is bindgen's
//! output of `scripts/generate_bindings.py`, from what `bindings.toml` in
//! this module's folder pins. It imports CUDA's stream type from the CUDA
//! Runtime's bindings. It also declares the header's version: the crate
//! does not compile unless it is the accepted one.

#![allow(
    non_camel_case_types,
    reason = "each function-pointer type is named as its C function"
)]

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod decomposition;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod error_strings;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod handle;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod network;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod tensor;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod versions;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod workspace;

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
    pub(crate) mod v2_13;
}
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) use generated::v2_13;

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use super::function_table::{FunctionTable, SymbolResolver};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use super::{NativeLibrary, ProbedVersions, cuda_runtime};

/// The cuTensorNet library (libcutensornet): where its file is looked for,
/// the version it must have and the CUDA Runtime it must be built against,
/// and the tables of the functions looked up in it.
///
/// [`discover`](crate::discover) opens it after the CUDA Runtime it runs
/// on, through [`LoadedLibraries::open`](super::LoadedLibraries::open).
/// Discovery's steps are in docs/loading.md, "The steps of discovery".
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) struct CuTensorNet;

/// cuTensorNet 2.13.0, the release the bindings were audited against.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
const AUDITED_VERSION: usize = 21_300;

// The bindings must come from the audited release's header, which declares
// its version. A new header changes the bindings, which must be audited
// before this version is changed to accept it.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
const _: () = assert!(
    v2_13::CUTENSORNET_VERSION as usize == AUDITED_VERSION,
    "the bindings come from another cuTensorNet release than the audited one"
);

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl NativeLibrary for CuTensorNet {
    const NAME: &'static str = "cuTensorNet";
    const VARIABLE: &'static str = "QDK_CUTENSORNET_LIBRARY";
    const DEFAULTS: &'static [&'static str] = &[
        "/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2",
        "libcutensornet.so.2",
    ];
    type Functions = CuTensorNetFunctions;

    /// Reads cuTensorNet's version, which must be 2.13.0, and the CUDA
    /// Runtime's it was built against, which must be 12.9. Neither call can
    /// fail. Each is read when its function is found, whether or not the
    /// other is.
    fn probe_versions<R: SymbolResolver>(resolver: &R) -> ProbedVersions {
        let mut probed = ProbedVersions::default();
        let functions = CuTensorNetVersionFunctions::resolve(resolver)
            .expect("a table of optional rows is resolved whatever is missing");
        if let Some(get_version) = functions.cutensornetGetVersion {
            // SAFETY: the function pointer was resolved with its exact
            // audited signature and the owning library is alive.
            let version = unsafe { get_version() };
            probed.audited("cuTensorNet", version as i128, AUDITED_VERSION as i128);
        }
        if let Some(get_cudart_version) = functions.cutensornetGetCudartVersion {
            // SAFETY: as for the preceding call.
            let cuda_runtime_version = unsafe { get_cudart_version() };
            probed.audited(
                "cuTensorNet's CUDA Runtime",
                cuda_runtime_version as i128,
                cuda_runtime::AUDITED_VERSION.into(),
            );
        }
        probed
    }
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
super::function_table::function_table! {
    /// Every cuTensorNet function discovery looks up.
    #[expect(
        dead_code,
        reason = "discovery only looks the functions up; the MPS engine, added later, calls them"
    )]
    struct CuTensorNetFunctions, declared in v2_13 {
        required handle::cutensornetCreate,
        required handle::cutensornetDestroy,
        required versions::cutensornetGetCudartVersion,
        required error_strings::cutensornetGetErrorString,
        // Adds detail to native error messages only; discovery does not
        // need it.
        optional error_strings::cutensornetGetLastError,
        required versions::cutensornetGetVersion,
        required tensor::cutensornetCreateTensorDescriptor,
        required tensor::cutensornetDestroyTensorDescriptor,
        required decomposition::cutensornetCreateTensorSVDConfig,
        required decomposition::cutensornetDestroyTensorSVDConfig,
        required decomposition::cutensornetTensorSVDConfigSetAttribute,
        required decomposition::cutensornetCreateTensorSVDInfo,
        required decomposition::cutensornetDestroyTensorSVDInfo,
        required decomposition::cutensornetTensorSVDInfoGetAttribute,
        required decomposition::cutensornetWorkspaceComputeGateSplitSizes,
        required decomposition::cutensornetGateSplit,
        required decomposition::cutensornetWorkspaceComputeQRSizes,
        required decomposition::cutensornetTensorQR,
        required network::cutensornetCreateNetwork,
        required network::cutensornetDestroyNetwork,
        required network::cutensornetNetworkAppendTensor,
        required network::cutensornetNetworkSetOutputTensor,
        required network::cutensornetCreateContractionOptimizerConfig,
        required network::cutensornetDestroyContractionOptimizerConfig,
        required network::cutensornetContractionOptimizerConfigSetAttribute,
        required network::cutensornetCreateContractionOptimizerInfo,
        required network::cutensornetDestroyContractionOptimizerInfo,
        required network::cutensornetContractionOptimize,
        required network::cutensornetWorkspaceComputeContractionSizes,
        required network::cutensornetNetworkPrepareContraction,
        required network::cutensornetNetworkSetInputTensorMemory,
        required network::cutensornetNetworkSetOutputTensorMemory,
        required network::cutensornetNetworkContract,
        required workspace::cutensornetCreateWorkspaceDescriptor,
        required workspace::cutensornetDestroyWorkspaceDescriptor,
        required workspace::cutensornetWorkspaceGetMemorySize,
        required workspace::cutensornetWorkspaceSetMemory,
    }
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
super::function_table::function_table! {
    /// The functions that read the versions, each optional, looked up on
    /// their own (`NativeLibrary::probe_versions` says why). The main table
    /// reports those missing.
    struct CuTensorNetVersionFunctions, declared in v2_13 {
        optional versions::cutensornetGetVersion,
        optional versions::cutensornetGetCudartVersion,
    }
}

#[cfg(all(test, unix))]
mod tests;
