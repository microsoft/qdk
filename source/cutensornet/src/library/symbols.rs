// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tables of the functions that discovery looks up in each library.
//!
//! The MPS engine, added in a later change, calls cuTensorNet and the CUDA
//! Runtime only through these tables, so that no NVIDIA symbol is linked
//! into the QDK. Each table is a list of rows `required field: Type =
//! symbol` (or `optional`), from which `function_table!` makes the field,
//! its lookup by the symbol's name, and a compile-time check that `Type` is
//! the symbol's declaration in the bindings; so a field is looked up by its
//! own symbol only, and with its declared signature. The crate's tests check
//! that the cuTensorNet rows are the rows of
//! `scripts/cutensornet-symbols.txt`. The families' files declare the
//! cuTensorNet function-pointer types; `cudart_12` the CUDA Runtime's.

pub(crate) mod context;
pub(crate) mod expectation;
pub(crate) mod operator;
pub(crate) mod state;
pub(crate) mod workspace;

use super::{CUDA_RUNTIME, CUTENSORNET, NativeLibrary, SymbolResolver};
use crate::{
    AvailabilityError,
    bindings::{cudart_12, v2_13},
};
use std::path::Path;

/// Declares a table of function pointers, one row per function:
/// `required field: Type = symbol` or `optional field: Type = symbol`.
///
/// It makes the field (an `Option` when optional), the lookup of the field
/// by `stringify!(symbol)` in the table's library, and `const _: Type =
/// declarations::symbol`, which compiles only if `Type` is the symbol's
/// declared signature. A missing required symbol fails the lookup, naming
/// it; a missing optional one is `None`.
macro_rules! function_table {
    (
        $(#[$attribute:meta])*
        struct $table:ident in $library:ident, declared in $declarations:ident {
            $($need:ident $field:ident: $type:ty = $symbol:ident,)*
        }
    ) => {
        $(#[$attribute])*
        pub(crate) struct $table {
            $(pub(crate) $field: function_table!(@field $need $type),)*
        }

        $(const _: $type = $declarations::$symbol;)*

        impl $table {
            /// Looks up every function of the table in `resolver`, the
            /// library opened from `path`.
            pub(crate) fn resolve<R: SymbolResolver>(
                resolver: &R,
                path: &Path,
            ) -> Result<Self, AvailabilityError> {
                Ok(Self {
                    $($field: function_table!(
                        @lookup $need resolver, &$library, path, $symbol
                    ),)*
                })
            }

            /// The rows, as `(field, symbol, required)`.
            #[cfg(test)]
            pub(crate) const ROWS: &[(&str, &str, bool)] = &[
                $((
                    stringify!($field),
                    stringify!($symbol),
                    function_table!(@required $need),
                ),)*
            ];
        }
    };
    (@field required $type:ty) => { $type };
    (@field optional $type:ty) => { Option<$type> };
    (@lookup required $resolver:ident, $library:expr, $path:ident, $symbol:ident) => {
        resolve_required($resolver, $library, $path, stringify!($symbol))?
    };
    (@lookup optional $resolver:ident, $library:expr, $path:ident, $symbol:ident) => {
        resolve_optional($resolver, stringify!($symbol))
    };
    (@required required) => { true };
    (@required optional) => { false };
}

function_table! {
    /// Every CUDA Runtime function the loader looks up.
    #[expect(
        dead_code,
        reason = "discovery calls only the version probes; the MPS engine and the setup check, added later, call the rest"
    )]
    struct CudaFunctions in CUDA_RUNTIME, declared in cudart_12 {
        required runtime_get_version: cudart_12::CudaRuntimeGetVersionFn = cudaRuntimeGetVersion,
        required driver_get_version: cudart_12::CudaDriverGetVersionFn = cudaDriverGetVersion,
        required get_device_count: cudart_12::CudaGetDeviceCountFn = cudaGetDeviceCount,
        required set_device: cudart_12::CudaSetDeviceFn = cudaSetDevice,
        required device_get_attribute: cudart_12::CudaDeviceGetAttributeFn =
            cudaDeviceGetAttribute,
        required get_error_string: cudart_12::CudaGetErrorStringFn = cudaGetErrorString,
        required mem_get_info: cudart_12::CudaMemGetInfoFn = cudaMemGetInfo,
        required malloc: cudart_12::CudaMallocFn = cudaMalloc,
        required free: cudart_12::CudaFreeFn = cudaFree,
        required memcpy: cudart_12::CudaMemcpyFn = cudaMemcpy,
        required stream_create_with_flags: cudart_12::CudaStreamCreateWithFlagsFn =
            cudaStreamCreateWithFlags,
        required stream_synchronize: cudart_12::CudaStreamSynchronizeFn = cudaStreamSynchronize,
        required stream_destroy: cudart_12::CudaStreamDestroyFn = cudaStreamDestroy,
    }
}

function_table! {
    /// Every cuTensorNet function the loader looks up.
    #[expect(
        dead_code,
        reason = "discovery calls only the version probes; the MPS engine, added later, calls the rest"
    )]
    struct CuTensorNetFunctions in CUTENSORNET, declared in v2_13 {
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

fn resolve_required<R: SymbolResolver, T: Copy>(
    resolver: &R,
    library: &'static NativeLibrary,
    path: &Path,
    symbol: &'static str,
) -> Result<T, AvailabilityError> {
    // SAFETY: `function_table!` gives `T` as the type checked against the
    // symbol's declaration; the library is retained by `LoadedLibraries`.
    unsafe { resolver.resolve(symbol) }.ok_or_else(|| AvailabilityError::MissingRequiredSymbol {
        library: library.name,
        path: path.to_path_buf(),
        symbol,
    })
}

fn resolve_optional<R: SymbolResolver, T: Copy>(resolver: &R, symbol: &'static str) -> Option<T> {
    // SAFETY: as in `resolve_required`.
    unsafe { resolver.resolve(symbol) }
}
