// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Scratch`]: the memory a [`CudaAlgebra`](super::CudaAlgebra) lends
//! cuTensorNet, in the four slots of its one workspace descriptor.
//!
//! Each tensor call (a gate split, a QR, a contraction) asks, once its
//! workspace is sized for it, for memory in some of the four slots: device
//! or host, scratch or cache. The algebra holds one buffer per slot, lends
//! it to every call, and grows it when a call asks for more; it never
//! shrinks, so at rest the algebra holds its largest call's memory. A
//! device buffer grows as the cuTensorNet engine's scratch does: the
//! larger buffer is lent before the smaller is freed, so the descriptor
//! never holds freed memory. A slot a call asks no memory for keeps what
//! it was lent: cuTensorNet refuses a slot lent a pointer and 0 bytes
//! (status 7 on the A100).

use crate::device_session::DeviceSession;
use crate::native_calls::error::{NativeError, first_failure};
use crate::native_calls::{NativeCalls, OpaqueHandle, memory_calls::WorkspaceSlot};
use std::ptr::NonNull;

/// The alignment, in bytes, of the host memory lent to a workspace, the
/// alignment cuTensorNet's own allocations have.
const HOST_ALIGNMENT: usize = 256;

/// The memory of a workspace descriptor's four slots, each grown on
/// demand.
#[derive(Default)]
pub(super) struct Scratch {
    /// The device scratch and the device cache: a buffer and its bytes.
    device: [Option<(OpaqueHandle, usize)>; 2],
    /// The host scratch and the host cache, each `HOST_ALIGNMENT` bytes
    /// larger than what it lends, to align it.
    host: [Vec<u8>; 2],
    /// Device buffers replaced while the descriptor may still hold them (a
    /// lending of their successor failed): freed once it is destroyed.
    detached: Vec<OpaqueHandle>,
}

impl Scratch {
    /// Lends `workspace`, sized for one call, the memory each of its slots
    /// asks for, first growing the slot's buffer if it is smaller.
    ///
    /// No device work may be pending: a growth frees the smaller buffer.
    pub(super) fn lend(
        &mut self,
        device_session: &DeviceSession<dyn NativeCalls>,
        workspace: OpaqueHandle,
    ) -> Result<(), NativeError> {
        let (calls, handle) = (device_session.calls(), device_session.handle());
        for (index, slot) in [WorkspaceSlot::DeviceScratch, WorkspaceSlot::DeviceCache]
            .into_iter()
            .enumerate()
        {
            if let Some(bytes) = asked(calls, handle, workspace, slot)? {
                self.lend_device(index, bytes, calls, handle, workspace, slot)?;
            }
        }
        for (index, slot) in [WorkspaceSlot::HostScratch, WorkspaceSlot::HostCache]
            .into_iter()
            .enumerate()
        {
            if let Some(bytes) = asked(calls, handle, workspace, slot)? {
                let held = &mut self.host[index];
                if held.len() < bytes + HOST_ALIGNMENT {
                    *held = vec![0; bytes + HOST_ALIGNMENT];
                }
                let offset = held.as_ptr().align_offset(HOST_ALIGNMENT);
                let aligned = NonNull::from(&mut held[offset]).cast();
                let lent = size_i64(held.len() - offset)?;
                calls.set_workspace(handle, workspace, slot, aligned, lent)?;
            }
        }
        Ok(())
    }

    /// Frees every device buffer, once the descriptor that held them is
    /// destroyed; attempts every free and returns the first failure.
    pub(super) fn release(&mut self, calls: &dyn NativeCalls) -> Result<(), NativeError> {
        let held = self.device.iter_mut().filter_map(Option::take);
        let buffers: Vec<OpaqueHandle> = held
            .map(|(buffer, _)| buffer)
            .chain(self.detached.drain(..))
            .collect();
        first_failure(buffers.into_iter().map(|buffer| calls.free(buffer)))
    }

    /// Lends device slot `slot`, of index `index`, `bytes`, growing its
    /// buffer first if it is smaller.
    fn lend_device(
        &mut self,
        index: usize,
        bytes: usize,
        calls: &dyn NativeCalls,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        slot: WorkspaceSlot,
    ) -> Result<(), NativeError> {
        let held = self.device[index].map_or(0, |(_, size)| size);
        if bytes <= held {
            let (buffer, size) = self.device[index].expect("a slot holding bytes has a buffer");
            return calls.set_workspace(handle, workspace, slot, buffer, size_i64(size)?);
        }
        let (free, _) = calls.memory_info()?;
        if bytes > free {
            return Err(NativeError::WorkspaceLimitExceeded {
                required: bytes,
                maximum: free,
            });
        }
        let grown = calls.allocate(bytes)?;
        let previous = self.device[index].replace((grown, bytes));
        let lent = size_i64(bytes)
            .and_then(|size| calls.set_workspace(handle, workspace, slot, grown, size));
        match (previous, lent) {
            (Some((old, _)), Ok(())) => calls.free(old),
            (Some((old, _)), Err(error)) => {
                self.detached.push(old);
                Err(error)
            }
            (None, lent) => lent,
        }
    }
}

/// The bytes `slot` of `workspace` asks for, or `None` if it asks for
/// none.
fn asked(
    calls: &dyn NativeCalls,
    handle: OpaqueHandle,
    workspace: OpaqueHandle,
    slot: WorkspaceSlot,
) -> Result<Option<usize>, NativeError> {
    let bytes = calls.workspace_size(handle, workspace, slot)?;
    if bytes <= 0 {
        return Ok(None);
    }
    usize::try_from(bytes)
        .map(Some)
        .map_err(|_| NativeError::ResourceSizeOverflow {
            resource: "workspace",
        })
}

fn size_i64(bytes: usize) -> Result<i64, NativeError> {
    i64::try_from(bytes).map_err(|_| NativeError::ResourceSizeOverflow {
        resource: "workspace",
    })
}
