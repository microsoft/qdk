// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `SessionCalls` on the loaded libraries: the device, the stream and the
//! cuTensorNet handle, through the CUDA Runtime and cuTensorNet.

use crate::engine::{
    EngineError,
    native_calls::{OpaqueHandle, Stream, session_calls::SessionCalls},
};
use crate::libraries::{LoadedLibraries, cuda_runtime::cudart_12};
use std::ptr::NonNull;

impl SessionCalls for LoadedLibraries {
    fn device_count(&self) -> Result<i32, EngineError> {
        let mut count = 0;
        // SAFETY: `count` is a valid writable CUDA `int` out-parameter.
        let status = unsafe { (self.cuda_runtime.functions.get_device_count)(&raw mut count) };
        self.check_cuda("cudaGetDeviceCount", status)?;
        Ok(count)
    }

    fn set_device(&self, ordinal: i32) -> Result<(), EngineError> {
        // SAFETY: the call takes no pointer, and CUDA checks the ordinal,
        // returning an error for an invalid one.
        let status = unsafe { (self.cuda_runtime.functions.set_device)(ordinal) };
        self.check_cuda("cudaSetDevice", status)
    }

    fn create_stream(&self) -> Result<Stream, EngineError> {
        let mut stream = std::ptr::null_mut();
        // SAFETY: `stream` is a valid writable out-pointer and the flags value
        // is the audited CUDA nonblocking-stream constant.
        let status = unsafe {
            (self.cuda_runtime.functions.stream_create_with_flags)(
                &raw mut stream,
                cudart_12::cudaStreamNonBlocking,
            )
        };
        self.check_cuda("cudaStreamCreateWithFlags", status)?;
        NonNull::new(stream.cast()).ok_or(EngineError::MissingNativeResource {
            operation: "cudaStreamCreateWithFlags",
            resource: "CUDA stream",
        })
    }

    fn synchronize_stream(&self, stream: Stream) -> Result<(), EngineError> {
        // SAFETY: `stream` is owned by the live session and has not been
        // destroyed.
        let status =
            unsafe { (self.cuda_runtime.functions.stream_synchronize)(stream.as_ptr().cast()) };
        self.check_cuda("cudaStreamSynchronize", status)
    }

    fn destroy_stream(&self, stream: Stream) -> Result<(), EngineError> {
        // SAFETY: the session consumes this owned stream exactly once.
        let status =
            unsafe { (self.cuda_runtime.functions.stream_destroy)(stream.as_ptr().cast()) };
        self.check_cuda("cudaStreamDestroy", status)
    }

    fn create_handle(&self) -> Result<OpaqueHandle, EngineError> {
        let mut handle = std::ptr::null_mut();
        // SAFETY: `handle` is a valid writable cuTensorNet out-pointer.
        let status = unsafe { (self.cutensornet.functions.create)(&raw mut handle) };
        self.check_cutensornet("cutensornetCreate", status)?;
        NonNull::new(handle).ok_or(EngineError::MissingNativeResource {
            operation: "cutensornetCreate",
            resource: "cuTensorNet handle",
        })
    }

    fn destroy_handle(&self, handle: OpaqueHandle) -> Result<(), EngineError> {
        // SAFETY: the session consumes this handle exactly once after all
        // children.
        let status = unsafe { (self.cutensornet.functions.destroy)(handle.as_ptr()) };
        self.check_cutensornet("cutensornetDestroy", status)
    }
}
