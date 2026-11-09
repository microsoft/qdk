// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The one scratch buffer of a native state. cuTensorNet works in scratch
//! memory its caller attaches to a workspace descriptor. The state holds one
//! buffer, attached to every descriptor it uses, and grows it, attaching the
//! larger buffer before freeing the smaller, when a call needs more; so no
//! descriptor ever holds freed memory. At rest a state holds one buffer, its
//! largest call's; while it grows, the old and the new.

use super::NativeState;
use crate::engine::{EngineError, native_calls::OpaqueHandle, policy::MAXIMUM_WORKSPACE_BYTES};
use std::rc::Rc;

impl NativeState {
    /// Attaches the state's scratch to `workspace`, first growing it to
    /// `bytes` if it is smaller. A larger buffer is attached to the state's
    /// own workspace descriptor before the smaller one is freed, so no
    /// descriptor holds freed memory.
    pub(super) fn attach_scratch(
        &mut self,
        workspace: OpaqueHandle,
        bytes: i64,
        resource: &'static str,
    ) -> Result<(), EngineError> {
        let device_session = Rc::clone(&self.device_session);
        let (calls, handle) = (device_session.calls(), self.handle());
        let needed =
            usize::try_from(bytes).map_err(|_| EngineError::ResourceSizeOverflow { resource })?;
        let held = self.scratch.map_or(0, |(_, size)| size);
        if needed > held {
            let (free, _) = calls.memory_info()?;
            validate_workspace_size(needed, MAXIMUM_WORKSPACE_BYTES, free)?;
            let grown = calls.allocate(needed)?;
            let previous = self.scratch.replace((grown, needed));
            // Only the state's own descriptor can hold the previous buffer:
            // a read's descriptor is new, and attached after this.
            let attached = self
                .workspace()
                .and_then(|own| calls.set_workspace(handle, own, grown, size_i64(needed)?));
            if let Some((old, _)) = previous {
                match attached {
                    Ok(()) => calls.free(old)?,
                    Err(error) => {
                        self.retired_scratch.push(old);
                        return Err(error);
                    }
                }
            } else {
                attached?;
            }
        }
        let (scratch, size) = self
            .scratch
            .expect("the scratch was grown to the call's size");
        calls.set_workspace(handle, workspace, scratch, size_i64(size)?)
    }

    /// The scratch a call may take: the budget, or the larger of the
    /// device's free memory and the scratch the state holds, whichever is
    /// less. Not their sum: a larger buffer is allocated before the held
    /// one is freed, so a growth must fit in the free memory alone.
    pub(super) fn available_workspace_bytes(&self) -> Result<usize, EngineError> {
        let (free, _) = self.device_session.calls().memory_info()?;
        let held = self.scratch.map_or(0, |(_, size)| size);
        Ok(MAXIMUM_WORKSPACE_BYTES.min(free.max(held)))
    }
}

fn validate_workspace_size(
    required: usize,
    policy_maximum: usize,
    available: usize,
) -> Result<(), EngineError> {
    let maximum = policy_maximum.min(available);
    if required > maximum {
        return Err(EngineError::WorkspaceLimitExceeded { required, maximum });
    }
    Ok(())
}

fn size_i64(bytes: usize) -> Result<i64, EngineError> {
    i64::try_from(bytes).map_err(|_| EngineError::ResourceSizeOverflow {
        resource: "device workspace",
    })
}
