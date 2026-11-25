// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`DeviceTensor`]: one site tensor of a [`CudaAlgebra`](super::CudaAlgebra),
//! a device buffer of complex doubles, freed when dropped.

use super::algebra_session::{AlgebraSession, NativeObject, Retired};
use crate::engine::cuda_algebra_error::{CudaAlgebraError, report_failed_release};
use crate::engine::native_calls::memory_calls::DeviceBuffer;
use std::{fmt, rc::Rc};

/// One site tensor of a [`CudaAlgebra`](crate::CudaAlgebra): a buffer of
/// complex doubles on the algebra's CUDA device, in the layout of
/// `canonical_mps`'s `SiteAlgebra`, which the algebra's calls read and
/// write. Only the algebra makes one, and only the algebra that made it
/// accepts it. Another refuses it with [`CudaAlgebraError::ForeignTensor`],
/// unless that algebra is poisoned and so refuses every call with
/// `Poisoned`.
///
/// It keeps the algebra's session alive, so it may outlive the algebra.
/// Dropping it frees its buffer, on the algebra's device. A poisoned
/// algebra's session keeps the buffer instead (`algebra_session.rs`). If
/// CUDA refuses to free it, or to select the device (only that device can
/// free it), the buffer stays allocated until the process ends, and a
/// warning is logged, since `Drop` cannot return the failure.
///
/// It is `!Send`, as its algebra is, since it shares the algebra's
/// session. This does not compile:
///
/// ```compile_fail,E0277
/// fn require_send<T: Send>() {}
/// require_send::<qdk_cutensornet::DeviceTensor>();
/// ```
pub struct DeviceTensor {
    /// The tensor's values, owned by the tensor.
    buffer: DeviceBuffer,
    /// How many complex doubles the buffer holds.
    elements: usize,
    /// The session of the algebra that made the tensor, which frees the
    /// buffer and tells this algebra's tensors from another's.
    session: Rc<AlgebraSession>,
}

impl DeviceTensor {
    /// The tensor of the `elements` complex doubles of `buffer`, a buffer
    /// of `session`'s device, which it now owns.
    pub(super) fn new(buffer: DeviceBuffer, elements: usize, session: &Rc<AlgebraSession>) -> Self {
        Self {
            buffer,
            elements,
            session: Rc::clone(session),
        }
    }

    /// Refuses the tensor unless the algebra of `session` made it. A
    /// tensor's buffer is freed through the session of the algebra that
    /// made it, which knows nothing of another algebra's device work. So
    /// after a failed call of another algebra, nothing would keep the
    /// buffer from being freed while that call's work may still read it.
    pub(super) fn check_made_by(
        &self,
        session: &Rc<AlgebraSession>,
    ) -> Result<(), CudaAlgebraError> {
        if Rc::ptr_eq(&self.session, session) {
            Ok(())
        } else {
            Err(CudaAlgebraError::ForeignTensor)
        }
    }

    /// The tensor's buffer, once checked to hold `elements` complex
    /// doubles: a call that would read past its end is a defect of its
    /// caller, refused before the device reads memory it does not own.
    pub(super) fn buffer_holding(&self, elements: usize) -> Result<DeviceBuffer, CudaAlgebraError> {
        if elements > self.elements {
            return Err(CudaAlgebraError::InvalidNativeCall {
                reason: format!(
                    "read of {elements} values from a site tensor that holds {}: the \
                     caller's bond dimensions do not match the tensor",
                    self.elements
                ),
            });
        }
        Ok(self.buffer)
    }

    /// Whether `buffer` is this tensor's.
    pub(super) fn uses_buffer(&self, buffer: DeviceBuffer) -> bool {
        self.buffer == buffer
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
        let freed = self
            .session
            .release(Retired::Object(NativeObject::Buffer(self.buffer)));
        report_failed_release("a device tensor", freed);
    }
}
