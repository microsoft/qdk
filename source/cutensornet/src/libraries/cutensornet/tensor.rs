// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's functions for tensors: their descriptors, the SVD's
//! configuration and report, the gate split, the QR, and contracting a
//! network of tensors.

use super::v2_13;
use crate::libraries::cuda_runtime::cudart_12;
use std::os::raw::c_void;

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

/// Function pointer for `cutensornetCreateNetwork`.
pub(crate) type CreateNetworkFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    *mut v2_13::cutensornetNetworkDescriptor_t,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetDestroyNetwork`.
pub(crate) type DestroyNetworkFn =
    unsafe extern "C" fn(v2_13::cutensornetNetworkDescriptor_t) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetNetworkAppendTensor`.
pub(crate) type NetworkAppendTensorFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    i32,
    *const i64,
    *const i32,
    *const v2_13::cutensornetTensorQualifiers_t,
    v2_13::cudaDataType_t,
    *mut i64,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetNetworkSetOutputTensor`.
pub(crate) type NetworkSetOutputTensorFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    i32,
    *const i32,
    v2_13::cudaDataType_t,
) -> v2_13::cutensornetStatus_t;

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

/// Function pointer for `cutensornetContractionOptimizerConfigSetAttribute`.
pub(crate) type ContractionOptimizerConfigSetAttributeFn =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetContractionOptimizerConfig_t,
        v2_13::cutensornetContractionOptimizerConfigAttributes_t,
        *const c_void,
        usize,
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

/// Function pointer for `cutensornetNetworkPrepareContraction`.
pub(crate) type NetworkPrepareContractionFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    v2_13::cutensornetWorkspaceDescriptor_t,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetNetworkSetInputTensorMemory`.
pub(crate) type NetworkSetInputTensorMemoryFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    i64,
    *const c_void,
    *const i64,
) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetNetworkSetOutputTensorMemory`.
pub(crate) type NetworkSetOutputTensorMemoryFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    *mut c_void,
    *const i64,
)
    -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetNetworkContract`.
pub(crate) type NetworkContractFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    i32,
    v2_13::cutensornetWorkspaceDescriptor_t,
    v2_13::cutensornetSliceGroup_t,
    cudart_12::cudaStream_t,
) -> v2_13::cutensornetStatus_t;
