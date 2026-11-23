// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's contraction of a network of tensors: the network, the
//! optimizer that chooses the contraction's order (its configuration and
//! report), the preparation, and the contraction itself. Each `XFn` is the
//! type of `cutensornetX`.

use super::v2_13;
use crate::libraries::cuda_runtime::cudart_12;
use std::ffi::c_void;

pub(crate) type CreateNetworkFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    *mut v2_13::cutensornetNetworkDescriptor_t,
) -> v2_13::cutensornetStatus_t;

pub(crate) type DestroyNetworkFn =
    unsafe extern "C" fn(v2_13::cutensornetNetworkDescriptor_t) -> v2_13::cutensornetStatus_t;

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

pub(crate) type NetworkSetOutputTensorFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    i32,
    *const i32,
    v2_13::cudaDataType_t,
) -> v2_13::cutensornetStatus_t;

pub(crate) type CreateContractionOptimizerConfigFn =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        *mut v2_13::cutensornetContractionOptimizerConfig_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type DestroyContractionOptimizerConfigFn =
    unsafe extern "C" fn(
        v2_13::cutensornetContractionOptimizerConfig_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type ContractionOptimizerConfigSetAttributeFn =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetContractionOptimizerConfig_t,
        v2_13::cutensornetContractionOptimizerConfigAttributes_t,
        *const c_void,
        usize,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type CreateContractionOptimizerInfoFn =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetNetworkDescriptor_t,
        *mut v2_13::cutensornetContractionOptimizerInfo_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type DestroyContractionOptimizerInfoFn =
    unsafe extern "C" fn(
        v2_13::cutensornetContractionOptimizerInfo_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type ContractionOptimizeFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    v2_13::cutensornetContractionOptimizerConfig_t,
    u64,
    v2_13::cutensornetContractionOptimizerInfo_t,
) -> v2_13::cutensornetStatus_t;

pub(crate) type WorkspaceComputeContractionSizesFn =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetNetworkDescriptor_t,
        v2_13::cutensornetContractionOptimizerInfo_t,
        v2_13::cutensornetWorkspaceDescriptor_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type NetworkPrepareContractionFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    v2_13::cutensornetWorkspaceDescriptor_t,
) -> v2_13::cutensornetStatus_t;

pub(crate) type NetworkSetInputTensorMemoryFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    i64,
    *const c_void,
    *const i64,
) -> v2_13::cutensornetStatus_t;

pub(crate) type NetworkSetOutputTensorMemoryFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    *mut c_void,
    *const i64,
)
    -> v2_13::cutensornetStatus_t;

pub(crate) type NetworkContractFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    i32,
    v2_13::cutensornetWorkspaceDescriptor_t,
    v2_13::cutensornetSliceGroup_t,
    cudart_12::cudaStream_t,
) -> v2_13::cutensornetStatus_t;
