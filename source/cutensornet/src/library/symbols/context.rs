// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's functions for the library handle, versions and error text.

use crate::bindings::v2_13;

/// Function pointer for `cutensornetCreate`.
pub(crate) type CreateFn =
    unsafe extern "C" fn(*mut v2_13::cutensornetHandle_t) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetDestroy`.
pub(crate) type DestroyFn =
    unsafe extern "C" fn(v2_13::cutensornetHandle_t) -> v2_13::cutensornetStatus_t;

/// Function pointer for `cutensornetGetCudartVersion`.
pub(crate) type GetCudartVersionFn = unsafe extern "C" fn() -> usize;

/// Function pointer for `cutensornetGetErrorString`.
pub(crate) type GetErrorStringFn =
    unsafe extern "C" fn(v2_13::cutensornetStatus_t) -> *const ::std::os::raw::c_char;

/// Function pointer for `cutensornetGetLastError`.
pub(crate) type GetLastErrorFn = unsafe extern "C" fn() -> *const ::std::os::raw::c_char;

/// Function pointer for `cutensornetGetVersion`.
pub(crate) type GetVersionFn = unsafe extern "C" fn() -> usize;
