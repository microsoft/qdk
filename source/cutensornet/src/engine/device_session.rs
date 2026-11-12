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
//! restarts on a handle of its own (`native_state.rs`), so the session's
//! handle is the one new states start on.
//!
//! `DeviceSession` selects the device, then creates the stream and the
//! handle, and releases them when dropped, the handle first. It is `!Send`
//! and `!Sync` (a `PhantomData<Rc<()>>`): CUDA's current device is
//! per-thread, and the stream and handle stay on the thread that made them,
//! so the engine and its states do too. It makes its native calls through
//! `SessionCalls` (`native_calls/session_calls.rs`).

use super::dev_tuning::{DevTimes, DevTuning};
use super::{
    EngineError,
    error::{combine_execution_and_cleanup, first_failure, report_failed_release},
    native_calls::{OpaqueHandle, Stream, session_calls::SessionCalls},
};
use std::{cell::Cell, marker::PhantomData, rc::Rc, sync::Arc};

/// The engine's CUDA stream and cuTensorNet handle on one device, created
/// on the calling thread and released when dropped, the handle first.
///
/// It holds the native calls, and with them the libraries, but no state or
/// buffer: the engine and each of its states hold it by `Rc`, so
/// it is released after the last of them. The `PhantomData<Rc<()>>` keeps
/// it `!Send` and `!Sync` whatever the handles' type.
pub(crate) struct DeviceSession<Calls: SessionCalls + ?Sized> {
    calls: Arc<Calls>,
    device_ordinal: i32,
    stream: Option<Stream>,
    handle: Option<OpaqueHandle>,
    /// Development only: the tuning bench's knobs and clocks.
    dev_tuning: Cell<DevTuning>,
    dev_times: Cell<DevTimes>,
    _bound_to_its_thread: PhantomData<Rc<()>>,
}

impl<Calls: SessionCalls + ?Sized> DeviceSession<Calls> {
    /// Selects device `device_ordinal`, then creates the stream and the
    /// handle on it. A host with no such device is `NoDevice`, refused
    /// before any selection; a failed creation releases what was created.
    pub(crate) fn new(calls: Arc<Calls>, device_ordinal: i32) -> Result<Self, EngineError> {
        let device_count = calls.device_count()?;
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
            dev_tuning: Cell::new(DevTuning::default()),
            dev_times: Cell::new(DevTimes::default()),
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

    /// Selects the session's device on the calling thread, as every engine
    /// call does first: CUDA's current device is per thread.
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

    /// Development only: the tuning bench's knobs.
    pub(crate) fn dev_tuning(&self) -> DevTuning {
        self.dev_tuning.get()
    }

    pub(crate) fn set_dev_tuning(&self, tuning: DevTuning) {
        self.dev_tuning.set(tuning);
    }

    /// Development only: the tuning bench's clocks.
    pub(crate) fn dev_times(&self) -> DevTimes {
        self.dev_times.get()
    }

    pub(crate) fn reset_dev_times(&self) {
        self.dev_times.set(DevTimes::default());
    }

    pub(crate) fn record_dev_time(&self, update: impl FnOnce(&mut DevTimes)) {
        let mut times = self.dev_times.get();
        update(&mut times);
        self.dev_times.set(times);
    }

    fn release(&mut self) -> Result<(), EngineError> {
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
