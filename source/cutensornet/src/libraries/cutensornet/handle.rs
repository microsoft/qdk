// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's library handle, which its other calls take: its creation and
//! destruction. Each type is the pointer type of the C function of its name.

use super::v2_13;

pub(crate) type cutensornetCreate =
    unsafe extern "C" fn(*mut v2_13::cutensornetHandle_t) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetDestroy =
    unsafe extern "C" fn(v2_13::cutensornetHandle_t) -> v2_13::cutensornetStatus_t;
