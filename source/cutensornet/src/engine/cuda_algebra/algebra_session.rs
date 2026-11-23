// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`AlgebraSession`]: what a [`CudaAlgebra`](super::CudaAlgebra) and its
//! tensors share (the device session, and the poison a failed call may
//! leave), so that none of them frees a buffer the device may still use.
//!
//! Every call of the algebra returns with its device work done, so a
//! buffer is freed at once when nothing uses it any more: a tensor when
//! dropped, the algebra's own memory when it is dropped. The exception is
//! a failed call whose wait for the device failed too: nothing then says
//! when the device is done, with that call's objects and buffers, its
//! inputs (any of the algebra's tensors), or the algebra's own memory.
//! From then on the session is poisoned:
//!
//! - every later call of the algebra returns [`CudaAlgebraError::Poisoned`];
//! - whatever is released (the failed call's objects, a tensor dropped,
//!   the algebra's memory when it is dropped) is retired instead, kept
//!   by the session;
//! - the retired objects are released after a wait for the device (its
//!   stream and its copies) succeeds, tried when the algebra is dropped
//!   and again when the session ends (the algebra and its last tensor
//!   dropped);
//! - if neither wait succeeds, they are leaked, never freed.

use super::device_session::DeviceSession;
use super::resources::AlgebraResources;
use crate::engine::cuda_algebra_error::{CudaAlgebraError, first_failure, report_failed_release};
use crate::engine::native_calls::{NativeCalls, OpaqueHandle};
use std::cell::RefCell;

/// An algebra's poison, and the device session it shares with its
/// tensors: whether a failed call left device work nobody could wait for,
/// and what that work's buffers keep until a wait succeeds (the module's
/// header says what follows).
pub(super) struct AlgebraSession {
    device_session: DeviceSession,
    unwaited: RefCell<Option<UnwaitedWork>>,
}

/// What a session keeps once poisoned: why, and what the device may still
/// use, released only after a wait for the device succeeds.
struct UnwaitedWork {
    reason: String,
    retired: Vec<Retired>,
}

/// Something the device may still use, released once it is done.
pub(super) enum Retired {
    /// A call's native object or buffer, or a tensor's buffer.
    Object(NativeObject),
    /// The algebra's own memory and objects, once it is dropped.
    Resources(AlgebraResources),
}

/// One native object or buffer, released by the call its kind needs: a
/// call's tensor descriptor, SVD configuration or report, or a buffer (a
/// call's or a tensor's).
pub(super) enum NativeObject {
    Descriptor(OpaqueHandle),
    SvdConfig(OpaqueHandle),
    SvdInfo(OpaqueHandle),
    Buffer(OpaqueHandle),
}

impl NativeObject {
    pub(super) fn release(self, calls: &dyn NativeCalls) -> Result<(), CudaAlgebraError> {
        match self {
            Self::Descriptor(descriptor) => calls.destroy_tensor_descriptor(descriptor),
            Self::SvdConfig(config) => calls.destroy_svd_config(config),
            Self::SvdInfo(info) => calls.destroy_svd_info(info),
            Self::Buffer(buffer) => calls.free(buffer),
        }
    }
}

impl Retired {
    fn release(self, calls: &dyn NativeCalls) -> Result<(), CudaAlgebraError> {
        match self {
            Self::Object(object) => object.release(calls),
            Self::Resources(resources) => resources.release(calls),
        }
    }
}

impl AlgebraSession {
    pub(super) fn new(device_session: DeviceSession) -> Self {
        Self {
            device_session,
            unwaited: RefCell::new(None),
        }
    }

    pub(super) fn device_session(&self) -> &DeviceSession {
        &self.device_session
    }

    /// `Poisoned`, with its reason, once a failed call could not wait for
    /// the device.
    pub(super) fn check_usable(&self) -> Result<(), CudaAlgebraError> {
        match &*self.unwaited.borrow() {
            Some(unwaited) => Err(CudaAlgebraError::Poisoned {
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
    /// session is poisoned. If the device cannot be bound, `retired` is
    /// dropped unreleased (leaked), since its release needs that device.
    pub(super) fn release(&self, retired: Retired) -> Result<(), CudaAlgebraError> {
        if let Some(unwaited) = &mut *self.unwaited.borrow_mut() {
            unwaited.retired.push(retired);
            return Ok(());
        }
        // The algebra's device, which another algebra's call may have
        // changed.
        self.device_session.bind_device()?;
        retired.release(self.device_session.calls())
    }

    /// Once poisoned, waits for the device (its stream and its copies)
    /// and, if both waits succeed, releases what was retired and lifts the
    /// poison: the device is done. If either fails, keeps them and returns
    /// the first failure.
    pub(super) fn wait_then_release_retired(&self) -> Result<(), CudaAlgebraError> {
        if self.unwaited.borrow().is_none() {
            return Ok(());
        }
        let session = &self.device_session;
        let calls = session.calls();
        session.bind_device()?;
        first_failure([
            calls.synchronize_stream(session.stream()),
            calls.synchronize_copies(),
        ])?;
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
        let released = self.wait_then_release_retired();
        if let Some(unwaited) = self.unwaited.take() {
            // The device may still use them, and no wait will come: leaked.
            std::mem::forget(unwaited.retired);
        }
        report_failed_release("the CUDA algebra's retired objects", released);
    }
}
