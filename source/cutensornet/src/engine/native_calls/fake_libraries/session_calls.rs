// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `SessionCalls` of the fake libraries: devices, streams and handles.

use super::{FakeLibraries, Object, failed, id};
use crate::engine::CudaAlgebraError;
use crate::engine::native_calls::session_calls::{CuTensorNetHandle, CudaStream, SessionCalls};

impl SessionCalls for FakeLibraries {
    fn device_count(&self) -> Result<i32, CudaAlgebraError> {
        Ok(self.call("device_count")?.device_count.unwrap_or(1))
    }

    fn set_device(&self, ordinal: i32) -> Result<(), CudaAlgebraError> {
        drop(self.call("set_device")?);
        if ordinal == 0 {
            Ok(())
        } else {
            Err(failed("set_device", "no such device"))
        }
    }

    fn create_stream(&self) -> Result<CudaStream, CudaAlgebraError> {
        Ok(self.call("create_stream")?.create(Object::Stream))
    }

    fn synchronize_stream(&self, stream: CudaStream) -> Result<(), CudaAlgebraError> {
        let mut inner = self.call("synchronize_stream")?;
        inner.require(stream, "synchronize_stream", |o| {
            matches!(o, Object::Stream)
        })?;
        inner
            .pending_writes
            .retain(|_, writer| *writer != id(stream));
        Ok(())
    }

    fn destroy_stream(&self, stream: CudaStream) -> Result<(), CudaAlgebraError> {
        self.call("destroy_stream")?
            .destroy(stream, "destroy_stream", |o| matches!(o, Object::Stream))
    }

    fn create_handle(&self) -> Result<CuTensorNetHandle, CudaAlgebraError> {
        Ok(self.call("create_handle")?.create(Object::Handle))
    }

    fn destroy_handle(&self, handle: CuTensorNetHandle) -> Result<(), CudaAlgebraError> {
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
