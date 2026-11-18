// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The session the CUDA algebra's native work runs in: one CUDA device, one
//! CUDA stream on it, and one cuTensorNet handle.
//!
//! The algebra needs them before it makes any native call. A CUDA stream is
//! the queue the device runs work in, in order: every call the algebra
//! queues (its splits, QRs and contractions) goes to one stream, so their
//! order on the device is the order the algebra issued them, and one
//! synchronization waits for all of it. A cuTensorNet handle is the
//! library's context; every native object (a workspace descriptor, a tensor
//! descriptor, a network) belongs to the handle it was created on, and must
//! be released before it.
//!
//! `DeviceSession` selects the device, then creates the stream and the
//! handle, and releases them when dropped, the handle first. It is `!Send`
//! and `!Sync` (a `PhantomData<Rc<()>>`): CUDA's current device is
//! per-thread, and the stream and handle stay on the thread that made them,
//! so the algebra and its tensors do too. It makes its native calls through
//! `SessionCalls` (`native_calls/session_calls.rs`).

use super::cuda_algebra::error::{
    CudaAlgebraError, combine_execution_and_cleanup, first_failure, report_failed_release,
};
use super::native_calls::{OpaqueHandle, Stream, session_calls::SessionCalls};
use std::{marker::PhantomData, rc::Rc, sync::Arc};

/// The CUDA algebra's stream and cuTensorNet handle on one device, created
/// on the calling thread and released when dropped, the handle first.
///
/// It holds the native calls, and with them the libraries, but no tensor or
/// buffer: the algebra and each of its tensors hold it, through their
/// shared `AlgebraSession`, so it is released after the last of them. The
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
    pub(crate) fn new(calls: Arc<Calls>, device_ordinal: i32) -> Result<Self, CudaAlgebraError> {
        let device_count = calls.device_count()?;
        if device_ordinal < 0 || device_ordinal >= device_count {
            return Err(CudaAlgebraError::NoDevice);
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

    /// Selects the session's device on the calling thread, as every call of
    /// the algebra does first: CUDA's current device is per thread.
    pub(crate) fn bind_device(&self) -> Result<(), CudaAlgebraError> {
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

    /// Selects the device, waits for the stream, then destroys the handle
    /// and the stream, each step attempted even after an earlier one
    /// failed; returns the first failure. Unlike the algebra, which leaks
    /// its buffers when it cannot wait for the device, the session holds no
    /// caller's data for pending work to write (the workspace and every
    /// buffer are the algebra's, released or leaked by it before the
    /// session), and CUDA releases a destroyed stream only once its queued
    /// work is done.
    fn release(&mut self) -> Result<(), CudaAlgebraError> {
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
