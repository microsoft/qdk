// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime's versions: its own, and the driver's. Each `XFn` is the
//! type of `cudaX`.

use super::cudart_12;
use std::ffi::c_int;

pub(crate) type RuntimeGetVersionFn = unsafe extern "C" fn(*mut c_int) -> cudart_12::cudaError_t;

pub(crate) type DriverGetVersionFn = unsafe extern "C" fn(*mut c_int) -> cudart_12::cudaError_t;
