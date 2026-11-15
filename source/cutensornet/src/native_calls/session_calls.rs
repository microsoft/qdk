// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA and cuTensorNet calls that open and close a `DeviceSession`,
//! the engine's or the tensor API caller's: select the device, and create,
//! synchronize and release its stream and its cuTensorNet handle.

use super::{OpaqueHandle, Stream};
use crate::native_calls::error::NativeError;

/// The native calls a `DeviceSession` makes: CUDA's device selection and
/// stream, and cuTensorNet's handle.
///
/// An implementation is shared between threads; the streams and handles it
/// creates are not, which `DeviceSession` enforces. A caller selects the
/// device before it creates an object or runs work on one, and releases
/// each object once, after its dependents (a handle's states) and after
/// the stream's work that uses it.
pub(crate) trait SessionCalls: Send + Sync {
    fn device_count(&self) -> Result<i32, NativeError>;
    fn set_device(&self, ordinal: i32) -> Result<(), NativeError>;
    fn create_stream(&self) -> Result<Stream, NativeError>;
    fn synchronize_stream(&self, stream: Stream) -> Result<(), NativeError>;
    fn destroy_stream(&self, stream: Stream) -> Result<(), NativeError>;
    fn create_handle(&self) -> Result<OpaqueHandle, NativeError>;
    fn destroy_handle(&self, handle: OpaqueHandle) -> Result<(), NativeError>;
}
