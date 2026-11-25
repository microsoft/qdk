// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime's device memory: how much is free, its allocation and
//! release, and copies to and from it. Each type is the pointer type of the C
//! function of its name.

use super::cudart_12;
use std::ffi::c_void;

pub(crate) type cudaMemGetInfo =
    unsafe extern "C" fn(*mut usize, *mut usize) -> cudart_12::cudaError_t;

pub(crate) type cudaMalloc =
    unsafe extern "C" fn(*mut *mut c_void, usize) -> cudart_12::cudaError_t;

pub(crate) type cudaFree = unsafe extern "C" fn(*mut c_void) -> cudart_12::cudaError_t;

pub(crate) type cudaMemcpy = unsafe extern "C" fn(
    *mut c_void,
    *const c_void,
    usize,
    cudart_12::cudaMemcpyKind,
) -> cudart_12::cudaError_t;
