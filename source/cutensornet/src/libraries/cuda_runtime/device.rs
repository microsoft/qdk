// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime's devices: how many there are, the one the calls go to, and
//! their attributes. Each type is the pointer type of the C function of its
//! name.

use super::cudart_12;
use std::ffi::c_int;

pub(crate) type cudaGetDeviceCount = unsafe extern "C" fn(*mut c_int) -> cudart_12::cudaError_t;

pub(crate) type cudaSetDevice = unsafe extern "C" fn(c_int) -> cudart_12::cudaError_t;

pub(crate) type cudaDeviceGetAttribute =
    unsafe extern "C" fn(*mut c_int, cudart_12::cudaDeviceAttr, c_int) -> cudart_12::cudaError_t;
