use super::v2_13::cudaStream_t;
use std::ffi::{c_char, c_int, c_uint, c_void};

pub(crate) type CudaError = c_uint;
pub(crate) type CudaMemcpyKind = c_uint;

pub(crate) const CUDA_MEMCPY_HOST_TO_DEVICE: CudaMemcpyKind = 1;
pub(crate) const CUDA_MEMCPY_DEVICE_TO_HOST: CudaMemcpyKind = 2;
pub(crate) const CUDA_STREAM_NON_BLOCKING: c_uint = 1;

pub(crate) type CudaRuntimeGetVersionFn = unsafe extern "C" fn(*mut c_int) -> CudaError;
pub(crate) type CudaDriverGetVersionFn = unsafe extern "C" fn(*mut c_int) -> CudaError;
pub(crate) type CudaGetDeviceCountFn = unsafe extern "C" fn(*mut c_int) -> CudaError;
pub(crate) type CudaSetDeviceFn = unsafe extern "C" fn(c_int) -> CudaError;
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
