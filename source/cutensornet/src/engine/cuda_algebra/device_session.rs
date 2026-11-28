// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The session the CUDA algebra's native work runs in: one CUDA device, one
//! CUDA stream on it, and one cuTensorNet handle.
//!
//! The algebra needs them before any native call. A CUDA stream is the
//! queue the device runs work in, in order. The algebra queues all its work
//! (its splits, QRs and contractions) on one stream, so the device runs it
//! in the order issued, and one synchronization waits for all of it. A
//! cuTensorNet handle is the library's context. Every native object (a
//! workspace descriptor, a tensor descriptor, a network) belongs to the
//! handle it was created on, and is released before it.
//!
//! `DeviceSession` selects the device, then creates the stream and the
//! handle, and releases them when dropped, the handle first. It is `!Send`
//! and `!Sync` (a `PhantomData<Rc<()>>`), so the algebra and its tensors
//! are too: CUDA's current device is per thread, and the stream and handle
//! stay on the thread that made them. It makes its native calls through
//! `SessionCalls` (`native_calls/session_calls.rs`).

use crate::engine::cuda_algebra_error::{
    CudaAlgebraError, combine_execution_and_cleanup, first_failure, report_failed_release,
};
use crate::engine::native_calls::{
    NativeCalls,
    session_calls::{CuTensorNetHandle, CudaStream},
};
use std::{marker::PhantomData, rc::Rc, sync::Arc};

/// The CUDA algebra's stream and cuTensorNet handle on one device.
///
/// It holds the native calls, and with them the libraries, but no tensor or
/// buffer: the algebra and each of its tensors hold it, through their
/// shared `AlgebraSession`, so it is released after the last of them.
pub(super) struct DeviceSession {
    /// The native calls, which keep the libraries loaded.
    calls: Arc<dyn NativeCalls>,
    /// The device, selected again before every call (`bind_device`).
    device_ordinal: i32,
    /// `None` only once released.
    stream: Option<CudaStream>,
    /// `None` only while `new` creates it, and once released.
    handle: Option<CuTensorNetHandle>,
    _bound_to_its_thread: PhantomData<Rc<()>>,
}

impl DeviceSession {
    /// Selects device `device_ordinal`, then creates the stream and the
    /// handle on it. A host with no such device is `NoDevice`, refused
    /// before any selection; a failed creation releases what was created.
    pub(super) fn new(
        calls: Arc<dyn NativeCalls>,
        device_ordinal: i32,
    ) -> Result<Self, CudaAlgebraError> {
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

    /// The native calls every step of the algebra goes through.
    pub(super) fn calls(&self) -> &dyn NativeCalls {
        self.calls.as_ref()
    }

    /// Selects the session's device on the calling thread, as every call of
    /// the algebra does first: CUDA's current device is per thread.
    pub(super) fn bind_device(&self) -> Result<(), CudaAlgebraError> {
        self.calls.set_device(self.device_ordinal)
    }

    /// The stream the algebra queues all its device work on.
    pub(super) fn stream(&self) -> CudaStream {
        self.stream
            .expect("a live session always owns its CUDA stream")
    }

    /// The cuTensorNet handle every native object of the algebra belongs
    /// to.
    pub(super) fn handle(&self) -> CuTensorNetHandle {
        self.handle
            .expect("a live session always owns its cuTensorNet handle")
    }

    /// Selects the device, waits for the stream, then destroys the handle
    /// and the stream. Each step is attempted even after an earlier one
    /// failed, and the first failure is returned. Unlike the algebra, the
    /// session destroys its objects even when it cannot wait for the
    /// device. It holds no caller's data for pending work to write: the
    /// workspace and every buffer are the algebra's, released or leaked
    /// before the session. And CUDA releases a destroyed stream only once
    /// its queued work is done. A handle the library refuses to destroy is
    /// leaked, not retried. A library may refuse while an object of the
    /// handle lives, which an earlier failed release leaked.
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

impl Drop for DeviceSession {
    fn drop(&mut self) {
        report_failed_release("the device session", self.release());
    }
}
