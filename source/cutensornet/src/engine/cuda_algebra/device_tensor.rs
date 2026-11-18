// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`DeviceTensor`]: one site tensor of a [`CudaAlgebra`](super::CudaAlgebra),
//! a device buffer of complex doubles, freed when dropped; and the sizes of
//! a tensor of given extents, checked for overflow: its element count, and
//! its extents as a descriptor takes them.

use super::algebra_session::{AlgebraSession, NativeObject, Retired};
use super::error::{CudaAlgebraError, report_failed_release};
use crate::engine::native_calls::OpaqueHandle;
use std::{fmt, rc::Rc};

/// One site tensor of a [`CudaAlgebra`](crate::CudaAlgebra): a buffer of
/// complex doubles on the algebra's CUDA device, laid out as
/// `canonical_mps`'s algebra says, which the algebra's calls read and
/// write. Only the algebra makes one, and only the algebra that made it
/// accepts it: another refuses it with [`CudaAlgebraError::ForeignTensor`].
///
/// It keeps the algebra's session alive, so it may outlive the algebra.
/// Dropping it frees its buffer; a failed free is not returned, as `Drop`
/// cannot return one, and debug builds write it to stderr. Every call of
/// the algebra returns with its device work done, so nothing uses the
/// buffer then, except after a failed call that could not wait for the
/// device (the algebra is poisoned): the buffer is then freed only after
/// a later wait for the device succeeds, or leaked. A drop that cannot
/// select the algebra's device leaks the buffer too, since its free needs
/// that device; debug builds write that failure as well.
pub struct DeviceTensor {
    buffer: OpaqueHandle,
    elements: usize,
    session: Rc<AlgebraSession>,
}

impl DeviceTensor {
    /// The tensor of the `elements` complex doubles of `buffer`, a buffer
    /// of `session`'s device, which it now owns.
    pub(super) fn new(buffer: OpaqueHandle, elements: usize, session: &Rc<AlgebraSession>) -> Self {
        Self {
            buffer,
            elements,
            session: Rc::clone(session),
        }
    }

    /// Refuses the tensor unless the algebra of `session` made it: a
    /// tensor's buffer is freed through the session of the algebra that
    /// made it, which knows nothing of another algebra's device work, so a
    /// failed call of another algebra could not keep the buffer from being
    /// freed while that call's work may still read it.
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
    pub(super) fn buffer_holding(&self, elements: usize) -> Result<OpaqueHandle, CudaAlgebraError> {
        if elements > self.elements {
            return Err(CudaAlgebraError::InvalidNativeCall {
                reason: format!(
                    "a tensor of {} elements read as one of {elements}",
                    self.elements
                ),
            });
        }
        Ok(self.buffer)
    }

    /// Whether `buffer` is this tensor's.
    pub(super) fn uses_buffer(&self, buffer: OpaqueHandle) -> bool {
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

/// `extents` as a descriptor takes them.
pub(super) fn native_extents(extents: &[usize]) -> Result<Vec<i64>, CudaAlgebraError> {
    extents
        .iter()
        .map(|&extent| {
            i64::try_from(extent).map_err(|_| CudaAlgebraError::ResourceSizeOverflow {
                resource: "tensor extent",
            })
        })
        .collect()
}

/// The element count of a tensor of `extents`.
pub(super) fn element_count(extents: &[usize]) -> Result<usize, CudaAlgebraError> {
    extents
        .iter()
        .try_fold(1_usize, |count, &extent| count.checked_mul(extent))
        .ok_or(CudaAlgebraError::ResourceSizeOverflow {
            resource: "device tensor",
        })
}
