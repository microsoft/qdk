// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's contraction of a network of tensors: the network, the
//! optimizer that chooses the contraction's order (its configuration and
//! report), the preparation, and the contraction itself. Each type is the
//! pointer type of the C function of its name.

use super::v2_13;
use crate::libraries::cuda_runtime::cudart_12;
use std::ffi::c_void;

pub(crate) type cutensornetCreateNetwork = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    *mut v2_13::cutensornetNetworkDescriptor_t,
) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetDestroyNetwork =
    unsafe extern "C" fn(v2_13::cutensornetNetworkDescriptor_t) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetNetworkAppendTensor = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    i32,
    *const i64,
    *const i32,
    *const v2_13::cutensornetTensorQualifiers_t,
    v2_13::cudaDataType_t,
    *mut i64,
)
    -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetNetworkSetOutputTensor =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetNetworkDescriptor_t,
        i32,
        *const i32,
        v2_13::cudaDataType_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetCreateContractionOptimizerConfig =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        *mut v2_13::cutensornetContractionOptimizerConfig_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetDestroyContractionOptimizerConfig =
    unsafe extern "C" fn(
        v2_13::cutensornetContractionOptimizerConfig_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetContractionOptimizerConfigSetAttribute =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetContractionOptimizerConfig_t,
        v2_13::cutensornetContractionOptimizerConfigAttributes_t,
        *const c_void,
        usize,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetCreateContractionOptimizerInfo =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetNetworkDescriptor_t,
        *mut v2_13::cutensornetContractionOptimizerInfo_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetDestroyContractionOptimizerInfo =
    unsafe extern "C" fn(
        v2_13::cutensornetContractionOptimizerInfo_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetContractionOptimize = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    v2_13::cutensornetContractionOptimizerConfig_t,
    u64,
    v2_13::cutensornetContractionOptimizerInfo_t,
)
    -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetWorkspaceComputeContractionSizes =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetNetworkDescriptor_t,
        v2_13::cutensornetContractionOptimizerInfo_t,
        v2_13::cutensornetWorkspaceDescriptor_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetNetworkPrepareContraction =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetNetworkDescriptor_t,
        v2_13::cutensornetWorkspaceDescriptor_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetNetworkSetInputTensorMemory =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetNetworkDescriptor_t,
        i64,
        *const c_void,
        *const i64,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetNetworkSetOutputTensorMemory =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetNetworkDescriptor_t,
        *mut c_void,
        *const i64,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetNetworkContract = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetNetworkDescriptor_t,
    i32,
    v2_13::cutensornetWorkspaceDescriptor_t,
    v2_13::cutensornetSliceGroup_t,
    cudart_12::cudaStream_t,
) -> v2_13::cutensornetStatus_t;
