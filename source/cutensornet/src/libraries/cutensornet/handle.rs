// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's library handle, which its other calls take: its creation
//! and destruction. Each `XFn` is the type of `cutensornetX`.

use super::v2_13;

pub(crate) type CreateFn =
    unsafe extern "C" fn(*mut v2_13::cutensornetHandle_t) -> v2_13::cutensornetStatus_t;

pub(crate) type DestroyFn =
    unsafe extern "C" fn(v2_13::cutensornetHandle_t) -> v2_13::cutensornetStatus_t;
