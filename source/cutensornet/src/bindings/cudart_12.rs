// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime 12 functions, types and constants the crate uses,
//! written by hand.
//!
//! They are transcribed from the CUDA Toolkit 12.9 header
//! `cuda_runtime_api.h`; this file is not generated. The `12` in the file
//! name is CUDA's major version. Widening this surface means a declaration
//! and a function-pointer type here, a row of `CudaFunctions` in
//! `src/library/symbols.rs`, and the row in the crate's tests.

use super::v2_13::cudaStream_t;
use std::ffi::{c_char, c_int, c_uint, c_void};

pub(crate) type CudaError = c_uint;
pub(crate) type CudaMemcpyKind = c_uint;

pub(crate) const CUDA_MEMCPY_HOST_TO_DEVICE: CudaMemcpyKind = 1;
pub(crate) const CUDA_STREAM_NON_BLOCKING: c_uint = 1;
/// `cudaDeviceAttr`, the attribute `cudaDeviceGetAttribute` reads.
pub(crate) type CudaDeviceAttr = c_uint;
/// `cudaDevAttrComputeCapabilityMajor`.
pub(crate) const CUDA_DEV_ATTR_COMPUTE_CAPABILITY_MAJOR: CudaDeviceAttr = 75;
/// `cudaDevAttrComputeCapabilityMinor`.
pub(crate) const CUDA_DEV_ATTR_COMPUTE_CAPABILITY_MINOR: CudaDeviceAttr = 76;

pub(crate) type CudaRuntimeGetVersionFn = unsafe extern "C" fn(*mut c_int) -> CudaError;
pub(crate) type CudaDriverGetVersionFn = unsafe extern "C" fn(*mut c_int) -> CudaError;
pub(crate) type CudaGetDeviceCountFn = unsafe extern "C" fn(*mut c_int) -> CudaError;
pub(crate) type CudaSetDeviceFn = unsafe extern "C" fn(c_int) -> CudaError;
pub(crate) type CudaDeviceGetAttributeFn =
    unsafe extern "C" fn(*mut c_int, CudaDeviceAttr, c_int) -> CudaError;
pub(crate) type CudaGetErrorStringFn = unsafe extern "C" fn(CudaError) -> *const c_char;
pub(crate) type CudaMemGetInfoFn = unsafe extern "C" fn(*mut usize, *mut usize) -> CudaError;
pub(crate) type CudaMallocFn = unsafe extern "C" fn(*mut *mut c_void, usize) -> CudaError;
pub(crate) type CudaFreeFn = unsafe extern "C" fn(*mut c_void) -> CudaError;
pub(crate) type CudaMemcpyFn =
    unsafe extern "C" fn(*mut c_void, *const c_void, usize, CudaMemcpyKind) -> CudaError;
pub(crate) type CudaStreamCreateWithFlagsFn =
    unsafe extern "C" fn(*mut cudaStream_t, c_uint) -> CudaError;
pub(crate) type CudaStreamSynchronizeFn = unsafe extern "C" fn(cudaStream_t) -> CudaError;
pub(crate) type CudaStreamDestroyFn = unsafe extern "C" fn(cudaStream_t) -> CudaError;

// The header's declarations, which the types above are checked against.
// Nothing calls them, so nothing links them.
unsafe extern "C" {
    pub(crate) fn cudaRuntimeGetVersion(runtime_version: *mut c_int) -> CudaError;
    pub(crate) fn cudaDriverGetVersion(driver_version: *mut c_int) -> CudaError;
    pub(crate) fn cudaGetDeviceCount(count: *mut c_int) -> CudaError;
    pub(crate) fn cudaSetDevice(device: c_int) -> CudaError;
    pub(crate) fn cudaDeviceGetAttribute(
        value: *mut c_int,
        attr: CudaDeviceAttr,
        device: c_int,
    ) -> CudaError;
    pub(crate) fn cudaGetErrorString(error: CudaError) -> *const c_char;
    pub(crate) fn cudaMemGetInfo(free: *mut usize, total: *mut usize) -> CudaError;
    pub(crate) fn cudaMalloc(dev_ptr: *mut *mut c_void, size: usize) -> CudaError;
    pub(crate) fn cudaFree(dev_ptr: *mut c_void) -> CudaError;
    pub(crate) fn cudaMemcpy(
        dst: *mut c_void,
        src: *const c_void,
        count: usize,
        kind: CudaMemcpyKind,
    ) -> CudaError;
    pub(crate) fn cudaStreamCreateWithFlags(stream: *mut cudaStream_t, flags: c_uint) -> CudaError;
    pub(crate) fn cudaStreamSynchronize(stream: cudaStream_t) -> CudaError;
    pub(crate) fn cudaStreamDestroy(stream: cudaStream_t) -> CudaError;
}
