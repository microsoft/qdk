// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's error text: the text of a status, and the detail of the
//! last failure. Each `XFn` is the type of `cutensornetX`.

use super::v2_13;
use std::ffi::c_char;

pub(crate) type GetErrorStringFn =
    unsafe extern "C" fn(v2_13::cutensornetStatus_t) -> *const c_char;

pub(crate) type GetLastErrorFn = unsafe extern "C" fn() -> *const c_char;
