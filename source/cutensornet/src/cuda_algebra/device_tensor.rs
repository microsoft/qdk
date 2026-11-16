// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`DeviceTensor`]: one site tensor of a [`CudaAlgebra`](super::CudaAlgebra),
//! a device buffer of complex doubles, freed when dropped.

use crate::device_session::DeviceSession;
use crate::native_calls::{
    NativeCalls, OpaqueHandle, error::NativeError, error::report_failed_release,
};
use std::{fmt, rc::Rc};

/// One site tensor of a [`CudaAlgebra`](crate::CudaAlgebra): a buffer of
/// complex doubles on the algebra's CUDA device, laid out as
/// `canonical_mps`'s algebra says, which the algebra's calls read and
/// write. Only the algebra makes one.
///
/// It keeps the algebra's device session alive, so it may outlive the
/// algebra. Dropping it frees its buffer; a failed free is not returned,
/// as `Drop` cannot return one, and debug builds write it to stderr. No
/// device work is pending on it then: every call of the algebra returns
/// with its work done.
pub struct DeviceTensor {
    buffer: OpaqueHandle,
    elements: usize,
    device_session: Rc<DeviceSession<dyn NativeCalls>>,
}

impl DeviceTensor {
    /// The tensor of the `elements` complex doubles of `buffer`, a buffer
    /// of `device_session`'s device, which it now owns.
    pub(super) fn new(
        buffer: OpaqueHandle,
        elements: usize,
        device_session: &Rc<DeviceSession<dyn NativeCalls>>,
    ) -> Self {
        Self {
            buffer,
            elements,
            device_session: Rc::clone(device_session),
        }
    }

    /// The tensor's buffer, once checked to hold `elements` complex
    /// doubles: a call that would read past its end is a defect of its
    /// caller, refused before the device reads memory it does not own.
    pub(super) fn holding(&self, elements: usize) -> Result<OpaqueHandle, NativeError> {
        if elements > self.elements {
            return Err(NativeError::InvalidNativeCall {
                reason: format!(
                    "a tensor of {} elements read as one of {elements}",
                    self.elements
                ),
            });
        }
        Ok(self.buffer)
    }
}

impl fmt::Debug for DeviceTensor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceTensor")
            .field("elements", &self.elements)
            .finish_non_exhaustive()
    }
}

impl Drop for DeviceTensor {
    fn drop(&mut self) {
        let session = &self.device_session;
        // The algebra's device, which another algebra's call may have
        // changed.
        let freed = session
            .bind_device()
            .and_then(|()| session.calls().free(self.buffer));
        report_failed_release("a device tensor", freed);
    }
}
