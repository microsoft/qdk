// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The session a caller's native work runs in (today the engine's states;
//! later the tensor API's caller): one CUDA device, one CUDA stream on it,
//! and one cuTensorNet handle.
//!
//! A caller needs them before it makes any native call. A CUDA stream is
//! the queue the device runs work in, in order: every call the caller
//! queues (the engine's computes and expectations, the tensor API's splits
//! and contractions) goes to one stream, so their order on the device is
//! the order the caller issued them, and one synchronization waits for all
//! of it. A cuTensorNet handle is the library's context; every native
//! object (a state, a workspace descriptor, a tensor descriptor) belongs
//! to the handle it was created on, and must be released before it. The
//! engine restarts a state on a handle of its own
//! (`engine/native_state.rs`), so the session's handle is the one its new
//! states start on.
//!
//! `DeviceSession` selects the device, then creates the stream and the
//! handle, and releases them when dropped, the handle first. It is `!Send`
//! and `!Sync` (a `PhantomData<Rc<()>>`): CUDA's current device is
//! per-thread, and the stream and handle stay on the thread that made them,
//! so its caller does too. It makes its native calls through
//! `SessionCalls` (`native_calls/session_calls.rs`).

use crate::native_calls::error::{
    NativeError, combine_execution_and_cleanup, first_failure, report_failed_release,
};
use crate::native_calls::{OpaqueHandle, Stream, session_calls::SessionCalls};
use std::{marker::PhantomData, rc::Rc, sync::Arc};

/// A caller's CUDA stream and cuTensorNet handle on one device (today the
/// engine's; later the tensor API caller's), created on the calling thread
/// and released when dropped, the handle first.
///
/// It holds the native calls, and with them the libraries, but no state or
/// buffer: its holders keep it by `Rc` (the engine and each of its
/// states), so it is released after the last of them. The
/// `PhantomData<Rc<()>>` keeps it `!Send` and `!Sync` whatever the handles'
/// type.
pub(crate) struct DeviceSession<Calls: SessionCalls + ?Sized> {
    calls: Arc<Calls>,
    device_ordinal: i32,
    stream: Option<Stream>,
    handle: Option<OpaqueHandle>,
    _bound_to_its_thread: PhantomData<Rc<()>>,
}

impl<Calls: SessionCalls + ?Sized> DeviceSession<Calls> {
    /// Selects device `device_ordinal`, then creates the stream and the
    /// handle on it. A host with no such device is `NoDevice`, refused
    /// before any selection; a failed creation releases what was created.
    pub(crate) fn new(calls: Arc<Calls>, device_ordinal: i32) -> Result<Self, NativeError> {
        let device_count = calls.device_count()?;
        if device_ordinal < 0 || device_ordinal >= device_count {
            return Err(NativeError::NoDevice);
        }
        calls.set_device(device_ordinal)?;
        let stream = calls.create_stream()?;
        let mut session = Self {
            calls,
            device_ordinal,
            stream: Some(stream),
            handle: None,
            _bound_to_its_thread: PhantomData,
        };
        match session.calls.create_handle() {
            Ok(handle) => session.handle = Some(handle),
            Err(error) => {
                return combine_execution_and_cleanup(Err(error), session.release());
            }
        }
        Ok(session)
    }

    pub(crate) fn calls(&self) -> &Calls {
        self.calls.as_ref()
    }

    /// Selects the session's device on the calling thread, as a caller does
    /// before its native calls (every engine call does it first): CUDA's
    /// current device is per thread.
    pub(crate) fn bind_device(&self) -> Result<(), NativeError> {
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

    fn release(&mut self) -> Result<(), NativeError> {
        if self.stream.is_none() && self.handle.is_none() {
            return Ok(());
        }
        first_failure(
            [
                Some(self.bind_device()),
                self.stream
                    .map(|stream| self.calls.synchronize_stream(stream)),
                self.handle
                    .take()
                    .map(|handle| self.calls.destroy_handle(handle)),
                self.stream
                    .take()
                    .map(|stream| self.calls.destroy_stream(stream)),
            ]
            .into_iter()
            .flatten(),
        )
    }
}

impl<Calls: SessionCalls + ?Sized> Drop for DeviceSession<Calls> {
    fn drop(&mut self) {
        report_failed_release("the device session", self.release());
    }
}
