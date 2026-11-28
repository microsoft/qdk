// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's error text: the text of a status, and the detail of the last
//! failure. Each type is the pointer type of the C function of its name.

use super::v2_13;
use std::ffi::c_char;

pub(crate) type cutensornetGetErrorString =
    unsafe extern "C" fn(v2_13::cutensornetStatus_t) -> *const c_char;

pub(crate) type cutensornetGetLastError = unsafe extern "C" fn() -> *const c_char;
