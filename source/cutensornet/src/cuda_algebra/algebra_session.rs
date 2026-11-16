// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`AlgebraSession`]: what a [`CudaAlgebra`](super::CudaAlgebra) and its
//! tensors share, so that none of them frees memory the device may still
//! use.
//!
//! Every call of the algebra returns with its device work done, so a
//! buffer is freed at once when nothing uses it any more: a tensor when
//! dropped, the algebra's own memory when it is dropped. The exception is
//! a failed call whose wait for the device failed too: nothing then says
//! when the device is done, with that call's objects and buffers, its
//! inputs (any of the algebra's tensors), or the algebra's own memory.
//! From then on the session is poisoned:
//!
//! - every later call of the algebra returns [`NativeError::Poisoned`];
//! - whatever is released (the failed call's objects, a tensor dropped,
//!   the algebra's memory when it is dropped) is retired instead, kept
//!   by the session;
//! - the retired objects are released after a wait for the stream
//!   succeeds, tried when the algebra is dropped and again when the
//!   session ends (the algebra and its last tensor dropped);
//! - if neither wait succeeds, they are leaked, never freed.
//!
//! So a buffer the device may still use is freed only after a successful
//! wait for its stream; otherwise it is kept until one succeeds, or
//! leaked.

use super::AlgebraResources;
use super::call_scope::Owned;
use crate::device_session::DeviceSession;
use crate::native_calls::NativeCalls;
use crate::native_calls::error::{NativeError, first_failure, report_failed_release};
use std::cell::RefCell;

/// The device session of an algebra and its tensors, and whether a failed
/// call left device work nobody could wait for (the module's header says
/// what follows).
pub(super) struct AlgebraSession {
    device_session: DeviceSession<dyn NativeCalls>,
    unwaited: RefCell<Option<UnwaitedWork>>,
}

/// What a session keeps once poisoned: why, and what the device may still
/// use, released only after a wait for the stream succeeds.
struct UnwaitedWork {
    reason: String,
    retired: Vec<Retired>,
}

/// Something the device may still use, released once it is done.
pub(super) enum Retired {
    /// A call's native object or buffer, or a tensor's buffer.
    Object(Owned),
    /// The algebra's own memory and objects, once it is dropped.
    Resources(AlgebraResources),
}

impl Retired {
    fn release(self, calls: &dyn NativeCalls) -> Result<(), NativeError> {
        match self {
            Self::Object(owned) => owned.release(calls),
            Self::Resources(resources) => resources.release(calls),
        }
    }
}

impl AlgebraSession {
    pub(super) fn new(device_session: DeviceSession<dyn NativeCalls>) -> Self {
        Self {
            device_session,
            unwaited: RefCell::new(None),
        }
    }

    pub(super) fn device_session(&self) -> &DeviceSession<dyn NativeCalls> {
        &self.device_session
    }

    /// `Poisoned`, with its reason, once a failed call could not wait for
    /// the device.
    pub(super) fn check_usable(&self) -> Result<(), NativeError> {
        match &*self.unwaited.borrow() {
            Some(unwaited) => Err(NativeError::Poisoned {
                reason: unwaited.reason.clone(),
            }),
            None => Ok(()),
        }
    }

    /// Poisons the session after a failed call could not wait for the
    /// device, retiring `retired`, which that call's work may still use.
    pub(super) fn poison(&self, reason: String, retired: impl IntoIterator<Item = Retired>) {
        let mut unwaited = self.unwaited.borrow_mut();
        let unwaited = unwaited.get_or_insert_with(|| UnwaitedWork {
            reason,
            retired: Vec::new(),
        });
        unwaited.retired.extend(retired);
    }

    /// Releases `retired` now, with the device bound, or retires it if the
    /// session is poisoned.
    pub(super) fn release(&self, retired: Retired) -> Result<(), NativeError> {
        if let Some(unwaited) = &mut *self.unwaited.borrow_mut() {
            unwaited.retired.push(retired);
            return Ok(());
        }
        // The algebra's device, which another algebra's call may have
        // changed.
        self.device_session.bind_device()?;
        retired.release(self.device_session.calls())
    }

    /// Once poisoned, waits for the stream and, if the wait succeeds,
    /// releases what was retired, the latest first, and lifts the poison:
    /// the device is done. If the wait fails, keeps them and returns its
    /// failure.
    pub(super) fn settle(&self) -> Result<(), NativeError> {
        if self.unwaited.borrow().is_none() {
            return Ok(());
        }
        let session = &self.device_session;
        let calls = session.calls();
        session
            .bind_device()
            .and_then(|()| calls.synchronize_stream(session.stream()))?;
        let unwaited = self.unwaited.take().expect("a poisoned session");
        first_failure(
            unwaited
                .retired
                .into_iter()
                .rev()
                .map(|retired| retired.release(calls)),
        )
    }
}

impl Drop for AlgebraSession {
    fn drop(&mut self) {
        let settled = self.settle();
        if let Some(unwaited) = self.unwaited.take() {
            // The device may still use them, and no wait will come: leaked.
            std::mem::forget(unwaited.retired);
        }
        report_failed_release("the CUDA algebra's retired objects", settled);
    }
}
