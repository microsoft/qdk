// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA and cuTensorNet calls that open and close the algebra's
//! session: select the device, and create, synchronize and release its
//! stream and its cuTensorNet handle.

use super::{OpaqueHandle, Stream};
use crate::engine::CudaAlgebraError;

/// The native calls a `DeviceSession` makes: CUDA's device selection and
/// stream, and cuTensorNet's handle.
///
/// An implementation is shared between threads; the streams and handles it
/// creates are not, which `DeviceSession` enforces. A caller selects the
/// device before it creates an object or runs work on one, and releases
/// each object once, after its dependents (a handle's descriptors and
/// networks) and after the stream's work that uses it.
pub(crate) trait SessionCalls: Send + Sync {
    /// The number of CUDA devices the process sees (`cudaGetDeviceCount`),
    /// which the session checks before it selects one.
    fn device_count(&self) -> Result<i32, CudaAlgebraError>;
    /// Makes device `ordinal` the calling thread's current device
    /// (`cudaSetDevice`), which every later call on this thread runs on.
    fn set_device(&self, ordinal: i32) -> Result<(), CudaAlgebraError>;
    /// A new non-blocking stream on the current device
    /// (`cudaStreamCreateWithFlags`), the queue the session's work runs in.
    fn create_stream(&self) -> Result<Stream, CudaAlgebraError>;
    /// Waits until the work queued on `stream` is done
    /// (`cudaStreamSynchronize`), so its results may be read and its
    /// buffers reused.
    fn synchronize_stream(&self, stream: Stream) -> Result<(), CudaAlgebraError>;
    /// Releases `stream` (`cudaStreamDestroy`), once, after its work.
    fn destroy_stream(&self, stream: Stream) -> Result<(), CudaAlgebraError>;
    /// A new cuTensorNet handle (`cutensornetCreate`), the context every
    /// cuTensorNet object of the session is created with.
    fn create_handle(&self) -> Result<OpaqueHandle, CudaAlgebraError>;
    /// Releases `handle` (`cutensornetDestroy`), once, after every object
    /// created with it.
    fn destroy_handle(&self, handle: OpaqueHandle) -> Result<(), CudaAlgebraError>;
}
