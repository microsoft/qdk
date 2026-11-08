// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet, NVIDIA's tensor-network library, which the MPS engine calls.
//!
//! Everything the crate assumes of cuTensorNet is in this module, as its
//! `NativeLibrary`: where it is looked for, the functions looked up in it
//! (`CuTensorNetFunctions`), and its versions: `cuTensorNet`, which must be
//! 2.13.0, and `cuTensorNet's CUDA Runtime`, the CUDA Runtime it was built
//! against, which must be 12.9. cuTensorNet encodes its version as
//! `10000·major + 100·minor + patch` (`21300` is 2.13.0), and a CUDA Runtime
//! version as CUDA does, `1000·major + 10·minor` (`12090` is 12.9). The
//! families' files (`context`, `state`, `workspace`, `operator`,
//! `expectation`) declare the function-pointer types of the table's rows.
//!
//! `v2_13`, in `generated/`, declares cuTensorNet 2.13's API, the functions,
//! types and constants of its header: a function pointer is only as right as
//! the C signature it was declared with. It is bindgen's output of
//! `scripts/generate_bindings.py` (its header says how), from what
//! `bindings.toml` in this module's folder pins, and imports CUDA's stream
//! type from the CUDA Runtime's bindings. It also declares the header's
//! version, which must be the accepted one: the crate does not compile
//! otherwise. `layout` checks, at compile time, the layout of the one type a
//! cuTensorNet function takes by value.
//!
//! The table is the list of the functions the crate uses; the bindings
//! declare the whole API, so binding another function is its row here and its
//! type in its family's file, which the crate checks, when it compiles,
//! against the function's declaration in `v2_13`.

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod context;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod expectation;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
mod layout;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod operator;
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod state;
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
use super::loader::{FunctionTable, SymbolResolver};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use super::{NativeLibrary, ProbedVersions, cuda_runtime};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use crate::{LibraryReport, PackagedLibraries, diagnosis::or_unread};
#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
use std::path::Path;

/// cuTensorNet.
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
    const DEFAULTS: &'static [&'static str] = &[
        "/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2",
        "libcutensornet.so.2",
    ];
    const INSTALL: &'static str = "cuTensorNet 2.13";
    type Functions = CuTensorNetFunctions;

    fn packaged(packaged: &PackagedLibraries) -> Option<&Path> {
        packaged.cutensornet.as_deref()
    }

    /// Reads cuTensorNet's version, which must be 2.13.0, and the CUDA
    /// Runtime's it was built against, which must be 12.9. Neither call can
    /// fail.
    fn probe_versions<R: SymbolResolver>(resolver: &R) -> ProbedVersions {
        let mut probed = ProbedVersions::default();
        let Ok(functions) = CuTensorNetVersionFunctions::resolve(resolver) else {
            return probed;
        };
        // SAFETY: the function pointer was resolved with its exact audited
        // signature and the owning library is alive.
        let version = unsafe { (functions.get_version)() };
        // SAFETY: as for the preceding call.
        let cuda_runtime_version = unsafe { (functions.get_cudart_version)() };
        probed.audited("cuTensorNet", version as i128, AUDITED_VERSION as i128);
        probed.audited(
            "cuTensorNet's CUDA Runtime",
            cuda_runtime_version as i128,
            cuda_runtime::AUDITED_VERSION.into(),
        );
        probed
    }

    /// Its path and version, and the CUDA Runtime it was built for.
    fn describe(report: &LibraryReport) -> Vec<String> {
        vec![format!(
            "cuTensorNet: {} ({}, built for CUDA Runtime {})",
            or_unread(report.path.as_ref().map(|path| path.display())),
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
super::loader::function_table! {
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
        required apply_tensor_operator: state::ApplyTensorOperatorFn =
            cutensornetStateApplyTensorOperator,
        required capture_mps: state::CaptureMpsFn = cutensornetStateCaptureMPS,
        required create_state: state::CreateStateFn = cutensornetCreateState,
        required destroy_state: state::DestroyStateFn = cutensornetDestroyState,
        required finalize_mps: state::FinalizeMpsFn = cutensornetStateFinalizeMPS,
        required initialize_mps: state::InitializeMpsFn = cutensornetStateInitializeMPS,
        required state_compute: state::StateComputeFn = cutensornetStateCompute,
        required state_configure: state::StateConfigureFn = cutensornetStateConfigure,
        required state_prepare: state::StatePrepareFn = cutensornetStatePrepare,
        required create_workspace: workspace::CreateWorkspaceFn =
            cutensornetCreateWorkspaceDescriptor,
        required destroy_workspace: workspace::DestroyWorkspaceFn =
            cutensornetDestroyWorkspaceDescriptor,
        required workspace_get_memory_size: workspace::WorkspaceGetMemorySizeFn =
            cutensornetWorkspaceGetMemorySize,
        required workspace_set_memory: workspace::WorkspaceSetMemoryFn =
            cutensornetWorkspaceSetMemory,
        required append_product: operator::AppendProductFn =
            cutensornetNetworkOperatorAppendProduct,
        required create_network_operator: operator::CreateNetworkOperatorFn =
            cutensornetCreateNetworkOperator,
        required destroy_network_operator: operator::DestroyNetworkOperatorFn =
            cutensornetDestroyNetworkOperator,
        required create_expectation: expectation::CreateExpectationFn =
            cutensornetCreateExpectation,
        required destroy_expectation: expectation::DestroyExpectationFn =
            cutensornetDestroyExpectation,
        required expectation_compute: expectation::ExpectationComputeFn =
            cutensornetExpectationCompute,
        required expectation_configure: expectation::ExpectationConfigureFn =
            cutensornetExpectationConfigure,
        required expectation_prepare: expectation::ExpectationPrepareFn =
            cutensornetExpectationPrepare,
    }
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
super::loader::function_table! {
    /// The functions that read the versions, looked up on their own so
    /// that the versions are read even when other functions are missing.
    struct CuTensorNetVersionFunctions, declared in v2_13 {
        required get_version: context::GetVersionFn = cutensornetGetVersion,
        required get_cudart_version: context::GetCudartVersionFn = cutensornetGetCudartVersion,
    }
}
