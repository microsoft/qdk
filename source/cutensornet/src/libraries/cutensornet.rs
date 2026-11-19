// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet, NVIDIA's tensor-network library, which the MPS engine calls.
//!
//! What discovery and the setup check's library steps assume of cuTensorNet is
//! in this module: as its `NativeLibrary`, where it is looked for, the
//! functions looked up in it (`CuTensorNetFunctions`), and its versions
//! (`cuTensorNet`, which must be 2.13.0, and `cuTensorNet's CUDA Runtime`, the
//! CUDA Runtime it was built against, which must be 12.9); as its
//! `LibrarySetupGuide`, what the setup check shows of it and advises.
//! cuTensorNet encodes its version as `10000·major + 100·minor + patch`
//! (`21300` is 2.13.0), and a CUDA Runtime version as CUDA does,
//! `1000·major + 10·minor` (`12090` is 12.9). The families' files (`context`,
//! `tensor`, `workspace`) declare the function-pointer types of the table's
//! rows.
//!
//! `v2_13`, in `generated/`, declares cuTensorNet 2.13's API, the functions,
//! types and constants of its header: a function pointer is only as right as
//! the C signature it was declared with. It is bindgen's output of
//! `scripts/generate_bindings.py` (its header says how), from what
//! `bindings.toml` in this module's folder pins, and imports CUDA's stream
//! type from the CUDA Runtime's bindings. It also declares the header's
//! version, which must be the accepted one: the crate does not compile
//! otherwise.
//!
//! The table is the list of the functions the crate uses; the bindings
//! declare the whole API, so binding another function is its row here, with
//! a field named for its symbol (the tests check it), and its type in its
//! family's file, which the crate checks, when it compiles, against the
//! function's declaration in `v2_13`.

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod context;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod tensor;
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
use super::{
    NativeLibrary, ProbedVersions, cuda_runtime, setup_guide::LibrarySetupGuide,
    setup_guide::or_unread,
};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use crate::LibraryReport;

/// The cuTensorNet library (libcutensornet): where its file is looked for,
/// the version it must have and the CUDA Runtime it must be built against,
/// and the tables of the functions looked up in it.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) struct CuTensorNet;

/// cuTensorNet 2.13.0, the release the bindings were audited against.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
const AUDITED_VERSION: usize = 21_300;

// The bindings must come from the audited release's header, which declares
// its version: a new header changes the bindings, which must be audited
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
    const FILE_NAME: &'static str = "libcutensornet.so.2";
    const DEFAULTS: &'static [&'static str] = &[
        "/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2",
        Self::FILE_NAME,
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
        if let Some(get_version) = functions.get_version {
            // SAFETY: the function pointer was resolved with its exact
            // audited signature and the owning library is alive.
            let version = unsafe { get_version() };
            probed.audited("cuTensorNet", version as i128, AUDITED_VERSION as i128);
        }
        if let Some(get_cudart_version) = functions.get_cudart_version {
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
impl LibrarySetupGuide for CuTensorNet {
    const INSTALL: &'static str = "cuTensorNet 2.13";

    /// Its path and version, and the CUDA Runtime it was built for.
    #[expect(
        clippy::unnecessary_debug_formatting,
        reason = "paths are quoted, as in the faults, so a line break adds no line"
    )]
    fn describe(report: &LibraryReport) -> Vec<String> {
        vec![format!(
            "cuTensorNet: {} ({}, built for CUDA Runtime {})",
            or_unread(report.path.as_ref().map(|path| format!("{path:?}"))),
            or_unread(report.version("cuTensorNet").map(cutensornet_version)),
            or_unread(
                report
                    .version("cuTensorNet's CUDA Runtime")
                    .map(cuda_runtime::cuda_version)
            ),
        )]
    }
}

/// cuTensorNet's `10000·major + 100·minor + patch` as `major.minor.patch`.
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
fn cutensornet_version(version: i128) -> String {
    format!(
        "{}.{}.{}",
        version / 10_000,
        version / 100 % 100,
        version % 100
    )
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
super::function_table::function_table! {
    /// Every cuTensorNet function discovery looks up.
    #[expect(
        dead_code,
        reason = "the version rows are only looked up, so that a missing one is reported; the versions are read through CuTensorNetVersionFunctions"
    )]
    struct CuTensorNetFunctions, declared in v2_13 {
        required create: context::CreateFn = cutensornetCreate,
        required destroy: context::DestroyFn = cutensornetDestroy,
        required get_cudart_version: context::GetCudartVersionFn = cutensornetGetCudartVersion,
        required get_error_string: context::GetErrorStringFn = cutensornetGetErrorString,
        // Adds detail to native error messages only; discovery does not
        // need it.
        optional get_last_error: context::GetLastErrorFn = cutensornetGetLastError,
        required get_version: context::GetVersionFn = cutensornetGetVersion,
        required create_tensor_descriptor: tensor::CreateTensorDescriptorFn =
            cutensornetCreateTensorDescriptor,
        required destroy_tensor_descriptor: tensor::DestroyTensorDescriptorFn =
            cutensornetDestroyTensorDescriptor,
        required create_svd_config: tensor::CreateTensorSvdConfigFn =
            cutensornetCreateTensorSVDConfig,
        required destroy_svd_config: tensor::DestroyTensorSvdConfigFn =
            cutensornetDestroyTensorSVDConfig,
        required svd_config_set_attribute: tensor::TensorSvdConfigSetAttributeFn =
            cutensornetTensorSVDConfigSetAttribute,
        required create_svd_info: tensor::CreateTensorSvdInfoFn = cutensornetCreateTensorSVDInfo,
        required destroy_svd_info: tensor::DestroyTensorSvdInfoFn =
            cutensornetDestroyTensorSVDInfo,
        required svd_info_get_attribute: tensor::TensorSvdInfoGetAttributeFn =
            cutensornetTensorSVDInfoGetAttribute,
        required gate_split_sizes: tensor::WorkspaceComputeGateSplitSizesFn =
            cutensornetWorkspaceComputeGateSplitSizes,
        required gate_split: tensor::GateSplitFn = cutensornetGateSplit,
        required qr_sizes: tensor::WorkspaceComputeQrSizesFn = cutensornetWorkspaceComputeQRSizes,
        required tensor_qr: tensor::TensorQrFn = cutensornetTensorQR,
        required create_network: tensor::CreateNetworkFn = cutensornetCreateNetwork,
        required destroy_network: tensor::DestroyNetworkFn = cutensornetDestroyNetwork,
        required network_append_tensor: tensor::NetworkAppendTensorFn =
            cutensornetNetworkAppendTensor,
        required network_set_output_tensor: tensor::NetworkSetOutputTensorFn =
            cutensornetNetworkSetOutputTensor,
        required create_optimizer_config: tensor::CreateContractionOptimizerConfigFn =
            cutensornetCreateContractionOptimizerConfig,
        required destroy_optimizer_config: tensor::DestroyContractionOptimizerConfigFn =
            cutensornetDestroyContractionOptimizerConfig,
        required optimizer_config_set_attribute: tensor::ContractionOptimizerConfigSetAttributeFn =
            cutensornetContractionOptimizerConfigSetAttribute,
        required create_optimizer_info: tensor::CreateContractionOptimizerInfoFn =
            cutensornetCreateContractionOptimizerInfo,
        required destroy_optimizer_info: tensor::DestroyContractionOptimizerInfoFn =
            cutensornetDestroyContractionOptimizerInfo,
        required optimize: tensor::ContractionOptimizeFn = cutensornetContractionOptimize,
        required contraction_sizes: tensor::WorkspaceComputeContractionSizesFn =
            cutensornetWorkspaceComputeContractionSizes,
        required network_prepare_contraction: tensor::NetworkPrepareContractionFn =
            cutensornetNetworkPrepareContraction,
        required network_set_input_tensor_memory: tensor::NetworkSetInputTensorMemoryFn =
            cutensornetNetworkSetInputTensorMemory,
        required network_set_output_tensor_memory: tensor::NetworkSetOutputTensorMemoryFn =
            cutensornetNetworkSetOutputTensorMemory,
        required network_contract: tensor::NetworkContractFn = cutensornetNetworkContract,
        required create_workspace: workspace::CreateWorkspaceFn =
            cutensornetCreateWorkspaceDescriptor,
        required destroy_workspace: workspace::DestroyWorkspaceFn =
            cutensornetDestroyWorkspaceDescriptor,
        required workspace_get_memory_size: workspace::WorkspaceGetMemorySizeFn =
            cutensornetWorkspaceGetMemorySize,
        required workspace_set_memory: workspace::WorkspaceSetMemoryFn =
            cutensornetWorkspaceSetMemory,
    }
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
super::function_table::function_table! {
    /// The functions that read the versions, looked up on their own, and
    /// each optional, so that each version is read even when other
    /// functions are missing; the main table reports those missing.
    struct CuTensorNetVersionFunctions, declared in v2_13 {
        optional get_version: context::GetVersionFn = cutensornetGetVersion,
        optional get_cudart_version: context::GetCudartVersionFn = cutensornetGetCudartVersion,
    }
}
