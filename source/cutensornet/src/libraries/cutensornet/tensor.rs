// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's tensor API: tensor descriptors, the SVD's configuration and
//! report, the gate split, the QR, and pairwise contractions.
//!
//! These functions factor and contract one or two MPS tensors at a time,
//! for a caller that holds the MPS itself (`native_calls::tensor_calls`);
//! the state API (`state`) holds it inside cuTensorNet instead. They are
//! looked up as one optional table, [`CuTensorNetTensorFunctions`]: a
//! cuTensorNet that lacks them is still usable through the state API, and
//! whoever needs them reports every missing one.

use super::v2_13;
use crate::libraries::cuda_runtime::cudart_12;
use std::os::raw::c_void;

crate::libraries::loader::function_table! {
    /// cuTensorNet's tensor API, looked up apart from the rest of the
    /// library (`CuTensorNetFunctions::tensor`).
    struct CuTensorNetTensorFunctions, declared in v2_13 {
        required create_tensor_descriptor: CreateTensorDescriptorFn =
            cutensornetCreateTensorDescriptor,
        required destroy_tensor_descriptor: DestroyTensorDescriptorFn =
            cutensornetDestroyTensorDescriptor,
        required create_svd_config: CreateTensorSvdConfigFn = cutensornetCreateTensorSVDConfig,
        required destroy_svd_config: DestroyTensorSvdConfigFn =
            cutensornetDestroyTensorSVDConfig,
        required svd_config_set_attribute: TensorSvdConfigSetAttributeFn =
            cutensornetTensorSVDConfigSetAttribute,
        required create_svd_info: CreateTensorSvdInfoFn = cutensornetCreateTensorSVDInfo,
        required destroy_svd_info: DestroyTensorSvdInfoFn = cutensornetDestroyTensorSVDInfo,
        required svd_info_get_attribute: TensorSvdInfoGetAttributeFn =
            cutensornetTensorSVDInfoGetAttribute,
        required gate_split_sizes: WorkspaceComputeGateSplitSizesFn =
            cutensornetWorkspaceComputeGateSplitSizes,
        required gate_split: GateSplitFn = cutensornetGateSplit,
        required qr_sizes: WorkspaceComputeQrSizesFn = cutensornetWorkspaceComputeQRSizes,
        required tensor_qr: TensorQrFn = cutensornetTensorQR,
        required create_network: CreateNetworkDescriptorFn = cutensornetCreateNetworkDescriptor,
        required destroy_network: DestroyNetworkDescriptorFn =
            cutensornetDestroyNetworkDescriptor,
        required create_optimizer_config: CreateContractionOptimizerConfigFn =
            cutensornetCreateContractionOptimizerConfig,
        required destroy_optimizer_config: DestroyContractionOptimizerConfigFn =
            cutensornetDestroyContractionOptimizerConfig,
        required create_optimizer_info: CreateContractionOptimizerInfoFn =
            cutensornetCreateContractionOptimizerInfo,
        required destroy_optimizer_info: DestroyContractionOptimizerInfoFn =
            cutensornetDestroyContractionOptimizerInfo,
        required optimize: ContractionOptimizeFn = cutensornetContractionOptimize,
        required contraction_sizes: WorkspaceComputeContractionSizesFn =
            cutensornetWorkspaceComputeContractionSizes,
        required create_plan: CreateContractionPlanFn = cutensornetCreateContractionPlan,
        required destroy_plan: DestroyContractionPlanFn = cutensornetDestroyContractionPlan,
        required contraction: ContractionFn = cutensornetContraction,
    }
}

/// Function pointer for `cutensornetCreateTensorDescriptor`.
pub(crate) type CreateTensorDescriptorFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    i32,
    *const i64,
    *const i64,
    *const i32,
    v2_13::cudaDataType_t,
    *mut v2_13::cutensornetTensorDescriptor_t,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetDestroyTensorDescriptor`.
pub(crate) type DestroyTensorDescriptorFn =
    unsafe extern "C" fn(v2_13::cutensornetTensorDescriptor_t) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetCreateTensorSVDConfig`.
pub(crate) type CreateTensorSvdConfigFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    *mut v2_13::cutensornetTensorSVDConfig_t,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetDestroyTensorSVDConfig`.
pub(crate) type DestroyTensorSvdConfigFn =
    unsafe extern "C" fn(v2_13::cutensornetTensorSVDConfig_t) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetTensorSVDConfigSetAttribute`.
pub(crate) type TensorSvdConfigSetAttributeFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorSVDConfig_t,
    v2_13::cutensornetTensorSVDConfigAttributes_t,
    *const c_void,
    usize,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetCreateTensorSVDInfo`.
pub(crate) type CreateTensorSvdInfoFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    *mut v2_13::cutensornetTensorSVDInfo_t,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetDestroyTensorSVDInfo`.
pub(crate) type DestroyTensorSvdInfoFn =
    unsafe extern "C" fn(v2_13::cutensornetTensorSVDInfo_t) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetTensorSVDInfoGetAttribute`.
pub(crate) type TensorSvdInfoGetAttributeFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorSVDInfo_t,
    v2_13::cutensornetTensorSVDInfoAttributes_t,
    *mut c_void,
    usize,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetWorkspaceComputeGateSplitSizes`.
pub(crate) type WorkspaceComputeGateSplitSizesFn =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetTensorDescriptor_t,
        v2_13::cutensornetTensorDescriptor_t,
        v2_13::cutensornetTensorDescriptor_t,
        v2_13::cutensornetTensorDescriptor_t,
        v2_13::cutensornetTensorDescriptor_t,
        v2_13::cutensornetGateSplitAlgo_t,
        v2_13::cutensornetTensorSVDConfig_t,
        v2_13::cutensornetComputeType_t,
        v2_13::cutensornetWorkspaceDescriptor_t,
    ) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetGateSplit`.
pub(crate) type GateSplitFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorDescriptor_t,
    *const c_void,
    v2_13::cutensornetTensorDescriptor_t,
    *const c_void,
    v2_13::cutensornetTensorDescriptor_t,
    *const c_void,
    v2_13::cutensornetTensorDescriptor_t,
    *mut c_void,
    *mut c_void,
    v2_13::cutensornetTensorDescriptor_t,
    *mut c_void,
    v2_13::cutensornetGateSplitAlgo_t,
    v2_13::cutensornetTensorSVDConfig_t,
    v2_13::cutensornetComputeType_t,
    v2_13::cutensornetTensorSVDInfo_t,
    v2_13::cutensornetWorkspaceDescriptor_t,
    cudart_12::cudaStream_t,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetWorkspaceComputeQRSizes`.
pub(crate) type WorkspaceComputeQrSizesFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetWorkspaceDescriptor_t,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetTensorQR`.
pub(crate) type TensorQrFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorDescriptor_t,
    *const c_void,
    v2_13::cutensornetTensorDescriptor_t,
    *mut c_void,
    v2_13::cutensornetTensorDescriptor_t,
    *mut c_void,
    v2_13::cutensornetWorkspaceDescriptor_t,
    cudart_12::cudaStream_t,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetCreateNetworkDescriptor`.
pub(crate) type CreateNetworkDescriptorFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    i32,
    *const i32,
    *const *const i64,
    *const *const i64,
    *const *const i32,
    *const v2_13::cutensornetTensorQualifiers_t,
    i32,
    *const i64,
    *const i64,
    *const i32,
    v2_13::cudaDataType_t,
    v2_13::cutensornetComputeType_t,
    *mut v2_13::cutensornetNetworkDescriptor_t,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetDestroyNetworkDescriptor`.
pub(crate) type DestroyNetworkDescriptorFn =
    unsafe extern "C" fn(v2_13::cutensornetNetworkDescriptor_t) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetCreateContractionOptimizerConfig`.
pub(crate) type CreateContractionOptimizerConfigFn =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        *mut v2_13::cutensornetContractionOptimizerConfig_t,
    ) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetDestroyContractionOptimizerConfig`.
pub(crate) type DestroyContractionOptimizerConfigFn =
    unsafe extern "C" fn(
        v2_13::cutensornetContractionOptimizerConfig_t,
    ) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetCreateContractionOptimizerInfo`.
pub(crate) type CreateContractionOptimizerInfoFn =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetNetworkDescriptor_t,
        *mut v2_13::cutensornetContractionOptimizerInfo_t,
    ) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetDestroyContractionOptimizerInfo`.
pub(crate) type DestroyContractionOptimizerInfoFn =
    unsafe extern "C" fn(
        v2_13::cutensornetContractionOptimizerInfo_t,
    ) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetContractionOptimize`.
pub(crate) type ContractionOptimizeFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    v2_13::cutensornetContractionOptimizerConfig_t,
    u64,
    v2_13::cutensornetContractionOptimizerInfo_t,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetWorkspaceComputeContractionSizes`.
pub(crate) type WorkspaceComputeContractionSizesFn =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetNetworkDescriptor_t,
        v2_13::cutensornetContractionOptimizerInfo_t,
        v2_13::cutensornetWorkspaceDescriptor_t,
    ) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetCreateContractionPlan`.
pub(crate) type CreateContractionPlanFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    v2_13::cutensornetContractionOptimizerInfo_t,
    v2_13::cutensornetWorkspaceDescriptor_t,
    *mut v2_13::cutensornetContractionPlan_t,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetDestroyContractionPlan`.
pub(crate) type DestroyContractionPlanFn =
    unsafe extern "C" fn(v2_13::cutensornetContractionPlan_t) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetContraction`.
pub(crate) type ContractionFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetContractionPlan_t,
    *const *const c_void,
    *mut c_void,
    v2_13::cutensornetWorkspaceDescriptor_t,
    i64,
    cudart_12::cudaStream_t,
) -> v2_13::cutensornetStatus_t;
