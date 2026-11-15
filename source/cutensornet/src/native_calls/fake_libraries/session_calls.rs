// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `SessionCalls` of the fake libraries: devices, streams and handles.

use super::{FakeLibraries, Object, failed, id};
use crate::native_calls::error::NativeError;
use crate::native_calls::{OpaqueHandle, Stream, session_calls::SessionCalls};

impl SessionCalls for FakeLibraries {
    fn device_count(&self) -> Result<i32, NativeError> {
        Ok(self.call("device_count")?.device_count.unwrap_or(1))
    }

    fn set_device(&self, ordinal: i32) -> Result<(), NativeError> {
        drop(self.call("set_device")?);
        if ordinal == 0 {
            Ok(())
        } else {
            Err(failed("set_device", "no such device"))
        }
    }

    fn create_stream(&self) -> Result<Stream, NativeError> {
        Ok(self.call("create_stream")?.create(Object::Stream))
    }

    fn synchronize_stream(&self, stream: Stream) -> Result<(), NativeError> {
        let mut inner = self.call("synchronize_stream")?;
        inner.require(stream, "synchronize_stream", |o| {
            matches!(o, Object::Stream)
        })?;
        inner
            .pending_writes
            .retain(|_, writer| *writer != id(stream));
        Ok(())
    }

    fn destroy_stream(&self, stream: Stream) -> Result<(), NativeError> {
        self.call("destroy_stream")?
            .destroy(stream, "destroy_stream", |o| matches!(o, Object::Stream))
    }

    fn create_handle(&self) -> Result<OpaqueHandle, NativeError> {
        Ok(self.call("create_handle")?.create(Object::Handle))
    }

    fn destroy_handle(&self, handle: OpaqueHandle) -> Result<(), NativeError> {
        let mut inner = self.call("destroy_handle")?;
        if inner.owners.values().any(|&owner| owner == id(handle)) {
            return Err(failed(
                "destroy_handle",
                "an object of the handle is still live",
            ));
        }
        inner.destroy(handle, "destroy_handle", |o| matches!(o, Object::Handle))
    }
}
