// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `SessionCalls` on the loaded libraries: the device, the stream and the
//! cuTensorNet handle, through the CUDA Runtime and cuTensorNet.

use crate::engine::CudaAlgebraError;
use crate::engine::native_calls::{
    NativePointer,
    session_calls::{CuTensorNetHandle, CudaStream, SessionCalls},
};
use crate::libraries::{LoadedLibraries, cuda_runtime::cudart_12};

use super::created;

impl SessionCalls for LoadedLibraries {
    fn device_count(&self) -> Result<i32, CudaAlgebraError> {
        let mut count = 0;
        // SAFETY: `count` is a valid writable CUDA `int` out-parameter.
        let status = unsafe { (self.cuda_runtime.functions.cudaGetDeviceCount)(&raw mut count) };
        self.check_cuda("cudaGetDeviceCount", status)?;
        Ok(count)
    }

    fn set_device(&self, ordinal: i32) -> Result<(), CudaAlgebraError> {
        // SAFETY: cudaSetDevice takes the ordinal by value. Its
        // documentation lists cudaErrorInvalidDevice among its returns, so an
        // ordinal no device has is an error status, never unsound.
        let status = unsafe { (self.cuda_runtime.functions.cudaSetDevice)(ordinal) };
        self.check_cuda("cudaSetDevice", status)
    }

    fn create_stream(&self) -> Result<CudaStream, CudaAlgebraError> {
        let mut stream = std::ptr::null_mut();
        // SAFETY: `stream` is a valid writable out-pointer and the flags value
        // is the audited CUDA nonblocking-stream constant.
        let status = unsafe {
            (self.cuda_runtime.functions.cudaStreamCreateWithFlags)(
                &raw mut stream,
                cudart_12::cudaStreamNonBlocking,
            )
        };
        self.check_cuda("cudaStreamCreateWithFlags", status)?;
        created(stream.cast(), "cudaStreamCreateWithFlags", "CUDA stream")
    }

    fn synchronize_stream(&self, stream: CudaStream) -> Result<(), CudaAlgebraError> {
        // SAFETY: `stream` is owned by the live session and has not been
        // destroyed.
        let status =
            unsafe { (self.cuda_runtime.functions.cudaStreamSynchronize)(stream.as_ptr().cast()) };
        self.check_cuda("cudaStreamSynchronize", status)
    }

    fn destroy_stream(&self, stream: CudaStream) -> Result<(), CudaAlgebraError> {
        // SAFETY: the session consumes this owned stream exactly once.
        let status =
            unsafe { (self.cuda_runtime.functions.cudaStreamDestroy)(stream.as_ptr().cast()) };
        self.check_cuda("cudaStreamDestroy", status)
    }

    fn create_handle(&self) -> Result<CuTensorNetHandle, CudaAlgebraError> {
        let mut handle = std::ptr::null_mut();
        // SAFETY: `handle` is a valid writable cuTensorNet out-pointer.
        let status = unsafe { (self.cutensornet.functions.cutensornetCreate)(&raw mut handle) };
        self.check_cutensornet("cutensornetCreate", status)?;
        created(handle, "cutensornetCreate", "cuTensorNet handle")
    }

    fn destroy_handle(&self, handle: CuTensorNetHandle) -> Result<(), CudaAlgebraError> {
        // SAFETY: the session consumes this handle exactly once after all
        // children.
        let status = unsafe { (self.cutensornet.functions.cutensornetDestroy)(handle.as_ptr()) };
        self.check_cutensornet("cutensornetDestroy", status)
    }
}
