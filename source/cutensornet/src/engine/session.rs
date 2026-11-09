// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The session every state of an engine runs in: one CUDA device, one CUDA
//! stream on it, and one cuTensorNet handle.
//!
//! The engine needs them before it can hold a state. A CUDA stream is the
//! queue the device runs work in, in order: every compute and expectation
//! of the engine's states goes to one stream, so their order on the device
//! is the order the engine issued them, and one synchronization waits for
//! all of it. A cuTensorNet handle is the library's context; every native
//! object (a state, a workspace descriptor, an expectation) belongs to the
//! handle it was created on, and must be released before it. A state
//! restarts on a handle of its own (`state.rs`), so the session's handle is
//! the one new states start on.
//!
//! `DeviceSession` selects the device, then creates the stream and the
//! handle, and releases them when dropped, the handle first. It is `!Send`
//! and `!Sync` (a `PhantomData<Rc<()>>`): CUDA's current device is
//! per-thread, and the stream and handle stay on the thread that made them,
//! so the engine and its states do too. `SessionCalls` is the set of native
//! calls it makes: cuTensorNet's in the library, a double's in the host
//! tests.

use super::{
    EngineError, OpaqueHandle, Stream,
    error::{combine_execution_and_cleanup, report_failed_release},
};
use std::{marker::PhantomData, rc::Rc, sync::Arc};

/// Injected CUDA Runtime/cuTensorNet context operations.
///
/// The immutable API may be shared; the streams and handles it creates may
/// not. Callers select the intended device before creation and
/// device-dependent work, keep parents alive through child cleanup and
/// release each acquired object once.
pub(crate) trait SessionCalls: Send + Sync {
    fn device_count(&self) -> Result<i32, EngineError>;
    fn set_device(&self, ordinal: i32) -> Result<(), EngineError>;
    fn create_stream(&self) -> Result<Stream, EngineError>;
    fn synchronize_stream(&self, stream: Stream) -> Result<(), EngineError>;
    fn destroy_stream(&self, stream: Stream) -> Result<(), EngineError>;
    fn create_handle(&self) -> Result<OpaqueHandle, EngineError>;
    fn destroy_handle(&self, handle: OpaqueHandle) -> Result<(), EngineError>;
}

/// Owns a CUDA stream and cuTensorNet handle on one worker thread.
///
/// Retains the API/library owner and records the selected device; it does not
/// own the GPU, a quantum state or numerical buffers. The `Rc` marker
/// forbids Send/Sync even if the raw-handle representation changes. Child
/// borrows prevent consuming this owner while they are live.
pub(crate) struct DeviceSession<Calls: SessionCalls + ?Sized> {
    calls: Arc<Calls>,
    device_ordinal: i32,
    stream: Option<Stream>,
    handle: Option<OpaqueHandle>,
    _thread_marker: PhantomData<Rc<()>>,
}

impl<Calls: SessionCalls + ?Sized> DeviceSession<Calls> {
    pub(crate) fn calls(&self) -> &Calls {
        self.calls.as_ref()
    }

    pub(crate) fn bind_device(&self) -> Result<(), EngineError> {
        self.calls.set_device(self.device_ordinal)
    }

    pub(crate) fn stream(&self) -> Stream {
        self.stream
            .expect("a live session always owns its CUDA stream")
    }

    pub(crate) fn handle(&self) -> OpaqueHandle {
        self.handle
            .expect("a live session always owns its cuTensorNet handle")
    }

    pub(crate) fn new(calls: Arc<Calls>, device_ordinal: i32) -> Result<Self, EngineError> {
        let device_count = calls.device_count()?;
        if device_count <= 0 {
            return Err(EngineError::NoDevice);
        }
        if device_ordinal < 0 || device_ordinal >= device_count {
            return Err(EngineError::NoDevice);
        }
        calls.set_device(device_ordinal)?;
        let stream = calls.create_stream()?;
        let mut session = Self {
            calls,
            device_ordinal,
            stream: Some(stream),
            handle: None,
            _thread_marker: PhantomData,
        };
        match session.calls.create_handle() {
            Ok(handle) => session.handle = Some(handle),
            Err(error) => {
                return combine_execution_and_cleanup(Err(error), session.release());
            }
        }
        Ok(session)
    }

    fn release(&mut self) -> Result<(), EngineError> {
        if self.stream.is_none() && self.handle.is_none() {
            return Ok(());
        }
        let mut first_error = self.bind_device().err();
        if let Some(stream) = self.stream
            && let Err(error) = self.calls.synchronize_stream(stream)
        {
            first_error.get_or_insert(error);
        }
        if let Some(handle) = self.handle.take()
            && let Err(error) = self.calls.destroy_handle(handle)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Some(stream) = self.stream.take()
            && let Err(error) = self.calls.destroy_stream(stream)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl<Calls: SessionCalls + ?Sized> Drop for DeviceSession<Calls> {
    fn drop(&mut self) {
        report_failed_release("the device session", self.release());
    }
}
