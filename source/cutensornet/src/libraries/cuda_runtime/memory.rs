// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime's device memory: how much is free, its allocation and
//! release, and copies to and from it. Each `XFn` is the type of `cudaX`.

use super::cudart_12;
use std::ffi::c_void;

pub(crate) type MemGetInfoFn =
    unsafe extern "C" fn(*mut usize, *mut usize) -> cudart_12::cudaError_t;

pub(crate) type MallocFn = unsafe extern "C" fn(*mut *mut c_void, usize) -> cudart_12::cudaError_t;

pub(crate) type FreeFn = unsafe extern "C" fn(*mut c_void) -> cudart_12::cudaError_t;

pub(crate) type MemcpyFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_void,
    usize,
    cudart_12::cudaMemcpyKind,
) -> cudart_12::cudaError_t;
