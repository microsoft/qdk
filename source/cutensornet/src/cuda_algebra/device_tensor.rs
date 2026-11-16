// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`DeviceTensor`]: one site tensor of a [`CudaAlgebra`](super::CudaAlgebra),
//! a device buffer of complex doubles, freed when dropped.

use super::algebra_session::{AlgebraSession, Retired};
use super::call_scope::Owned;
use crate::native_calls::{OpaqueHandle, error::NativeError, error::report_failed_release};
use std::{fmt, rc::Rc};

/// One site tensor of a [`CudaAlgebra`](crate::CudaAlgebra): a buffer of
/// complex doubles on the algebra's CUDA device, laid out as
/// `canonical_mps`'s algebra says, which the algebra's calls read and
/// write. Only the algebra makes one.
///
/// It keeps the algebra's session alive, so it may outlive the algebra.
/// Dropping it frees its buffer; a failed free is not returned, as `Drop`
/// cannot return one, and debug builds write it to stderr. Every call of
/// the algebra returns with its device work done, so nothing uses the
/// buffer then, except after a failed call that could not wait for the
/// device (the algebra is poisoned): the buffer is then freed only after
/// a later wait for the device succeeds, or leaked.
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
        let freed = self
            .session
            .release(Retired::Object(Owned::Buffer(self.buffer)));
        report_failed_release("a device tensor", freed);
    }
}
