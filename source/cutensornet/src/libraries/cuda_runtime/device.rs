// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime's devices: how many there are, the one the calls go to,
//! and their attributes. Each `XFn` is the type of `cudaX`.

use super::cudart_12;
use std::ffi::c_int;

pub(crate) type GetDeviceCountFn = unsafe extern "C" fn(*mut c_int) -> cudart_12::cudaError_t;

pub(crate) type SetDeviceFn = unsafe extern "C" fn(c_int) -> cudart_12::cudaError_t;

pub(crate) type DeviceGetAttributeFn =
    unsafe extern "C" fn(*mut c_int, cudart_12::cudaDeviceAttr, c_int) -> cudart_12::cudaError_t;
