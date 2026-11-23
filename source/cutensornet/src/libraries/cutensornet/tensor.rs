// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's tensor descriptors, which give a tensor's modes, extents,
//! strides and data type to its other calls. Each `XFn` is the type of
//! `cutensornetX`.

use super::v2_13;

pub(crate) type CreateTensorDescriptorFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    i32,
    *const i64,
    *const i64,
    *const i32,
    v2_13::cudaDataType_t,
    *mut v2_13::cutensornetTensorDescriptor_t,
) -> v2_13::cutensornetStatus_t;

pub(crate) type DestroyTensorDescriptorFn =
    unsafe extern "C" fn(v2_13::cutensornetTensorDescriptor_t) -> v2_13::cutensornetStatus_t;
