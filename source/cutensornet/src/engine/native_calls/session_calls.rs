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
    fn device_count(&self) -> Result<i32, CudaAlgebraError>;
    fn set_device(&self, ordinal: i32) -> Result<(), CudaAlgebraError>;
    fn create_stream(&self) -> Result<Stream, CudaAlgebraError>;
    fn synchronize_stream(&self, stream: Stream) -> Result<(), CudaAlgebraError>;
    fn destroy_stream(&self, stream: Stream) -> Result<(), CudaAlgebraError>;
    fn create_handle(&self) -> Result<OpaqueHandle, CudaAlgebraError>;
    fn destroy_handle(&self, handle: OpaqueHandle) -> Result<(), CudaAlgebraError>;
}
