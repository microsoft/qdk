// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's decompositions of tensors: the SVD's configuration and
//! report, the gate split (a gate applied to two tensors, then split back by
//! an SVD), and the QR. Each `XFn` is the type of `cutensornetX`, its
//! acronyms in Rust's case (`Svd` for `SVD`, `Qr` for `QR`).

use super::v2_13;
use crate::libraries::cuda_runtime::cudart_12;
use std::ffi::c_void;

pub(crate) type CreateTensorSvdConfigFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    *mut v2_13::cutensornetTensorSVDConfig_t,
) -> v2_13::cutensornetStatus_t;

pub(crate) type DestroyTensorSvdConfigFn =
    unsafe extern "C" fn(v2_13::cutensornetTensorSVDConfig_t) -> v2_13::cutensornetStatus_t;

pub(crate) type TensorSvdConfigSetAttributeFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorSVDConfig_t,
    v2_13::cutensornetTensorSVDConfigAttributes_t,
    *const c_void,
    usize,
) -> v2_13::cutensornetStatus_t;

pub(crate) type CreateTensorSvdInfoFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    *mut v2_13::cutensornetTensorSVDInfo_t,
) -> v2_13::cutensornetStatus_t;

pub(crate) type DestroyTensorSvdInfoFn =
    unsafe extern "C" fn(v2_13::cutensornetTensorSVDInfo_t) -> v2_13::cutensornetStatus_t;

pub(crate) type TensorSvdInfoGetAttributeFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorSVDInfo_t,
    v2_13::cutensornetTensorSVDInfoAttributes_t,
    *mut c_void,
    usize,
) -> v2_13::cutensornetStatus_t;

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

pub(crate) type WorkspaceComputeQrSizesFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetWorkspaceDescriptor_t,
) -> v2_13::cutensornetStatus_t;

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
