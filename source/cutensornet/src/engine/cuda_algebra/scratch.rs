// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Scratch`]: the memory a [`CudaAlgebra`](super::CudaAlgebra) lends
//! cuTensorNet, in the four slots of its one workspace descriptor.
//!
//! Each tensor call (a gate split, a QR, a contraction) asks, once its
//! workspace is sized for it, for memory in some of the four slots: device or
//! host, scratch or cache. The algebra holds one buffer per slot, lends it to
//! every call, and grows it when a call asks for more; it never shrinks, so at
//! rest the algebra holds its largest call's memory. When a buffer grows, the
//! larger buffer is lent before the smaller is freed, so the descriptor never
//! holds freed memory. Only a device buffer is bounded: it never grows beyond
//! the device's free memory. A host slot's growth that the host's allocator
//! refuses fails the call, as an error. A slot a call asks no memory for keeps
//! what it was lent: cuTensorNet refuses a slot lent a pointer and 0 bytes
//! (status 7 on the A100). A slot that asks for a negative size fails the
//! call before its tensor work runs, as a result the library may not give.

use super::error::{CudaAlgebraError, first_failure};
use crate::engine::device_session::DeviceSession;
use crate::engine::native_calls::{NativeCalls, OpaqueHandle, memory_calls::WorkspaceSlot};
use std::ptr::NonNull;

/// The alignment, in bytes, of the host memory lent to a workspace, the
/// alignment cuTensorNet's own allocations have.
const HOST_ALIGNMENT: usize = 256;

/// The bytes a host slot holds beyond what it lends: the most it may skip
/// to reach a `HOST_ALIGNMENT`-aligned byte, when its memory starts one
/// byte past one.
const HOST_PADDING: usize = HOST_ALIGNMENT - 1;

/// The memory of a workspace descriptor's four slots, each grown on
/// demand.
#[derive(Default)]
pub(super) struct Scratch {
    /// The device scratch and the device cache: a buffer and its bytes.
    device: [Option<(OpaqueHandle, usize)>; 2],
    /// The host scratch and the host cache, each `HOST_PADDING` bytes
    /// larger than what it lends, to align it.
    host: [Vec<u8>; 2],
    /// Device buffers replaced while the descriptor may still hold them (a
    /// lending of their successor failed): freed once it is destroyed.
    detached: Vec<OpaqueHandle>,
}

impl Scratch {
    /// Lends `workspace`, sized for one call, the memory each of its slots
    /// asks for, first growing the slot's buffer if it is smaller. A device
    /// buffer just allocated is lent only if `check_allocation` accepts it
    /// (the call would write it), and is otherwise neither kept nor freed.
    ///
    /// No device work may be pending: a growth frees the smaller buffer.
    pub(super) fn lend(
        &mut self,
        device_session: &DeviceSession<dyn NativeCalls>,
        workspace: OpaqueHandle,
        check_allocation: &dyn Fn(OpaqueHandle) -> Result<OpaqueHandle, CudaAlgebraError>,
    ) -> Result<(), CudaAlgebraError> {
        let (calls, handle) = (device_session.calls(), device_session.handle());
        for (index, slot) in [WorkspaceSlot::DeviceScratch, WorkspaceSlot::DeviceCache]
            .into_iter()
            .enumerate()
        {
            if let Some(bytes) = asked(calls, handle, workspace, slot)? {
                self.lend_device(
                    index,
                    bytes,
                    device_session,
                    workspace,
                    slot,
                    check_allocation,
                )?;
            }
        }
        for (index, slot) in [WorkspaceSlot::HostScratch, WorkspaceSlot::HostCache]
            .into_iter()
            .enumerate()
        {
            if let Some(bytes) = asked(calls, handle, workspace, slot)? {
                let padded = bytes.checked_add(HOST_PADDING).ok_or(
                    CudaAlgebraError::ResourceSizeOverflow {
                        resource: "host workspace",
                    },
                )?;
                let held = &mut self.host[index];
                if held.len() < padded {
                    // The smaller memory is dropped only once the
                    // descriptor holds the larger, as a device buffer is.
                    let mut grown = host_memory(padded)
                        .ok_or(CudaAlgebraError::HostAllocationFailed { bytes })?;
                    lend_host(calls, handle, workspace, slot, &mut grown)?;
                    *held = grown;
                } else {
                    lend_host(calls, handle, workspace, slot, held)?;
                }
            }
        }
        Ok(())
    }

    /// Frees every device buffer, once the descriptor that held them is
    /// destroyed; attempts every free and returns the first failure.
    pub(super) fn release(&mut self, calls: &dyn NativeCalls) -> Result<(), CudaAlgebraError> {
        let held = self.device.iter_mut().filter_map(Option::take);
        let buffers: Vec<OpaqueHandle> = held
            .map(|(buffer, _)| buffer)
            .chain(self.detached.drain(..))
            .collect();
        first_failure(buffers.into_iter().map(|buffer| calls.free(buffer)))
    }

    /// Lends device slot `slot`, of index `index`, `bytes`, growing its
    /// buffer first, with one `check_allocation` accepts, if it is smaller.
    fn lend_device(
        &mut self,
        index: usize,
        bytes: usize,
        device_session: &DeviceSession<dyn NativeCalls>,
        workspace: OpaqueHandle,
        slot: WorkspaceSlot,
        check_allocation: &dyn Fn(OpaqueHandle) -> Result<OpaqueHandle, CudaAlgebraError>,
    ) -> Result<(), CudaAlgebraError> {
        let (calls, handle) = (device_session.calls(), device_session.handle());
        let held = self.device[index].map_or(0, |(_, size)| size);
        if bytes <= held {
            let (buffer, size) = self.device[index].expect("a slot holding bytes has a buffer");
            return calls.set_workspace(handle, workspace, slot, buffer, size_i64(size)?);
        }
        let (free, _) = calls.memory_info()?;
        if bytes > free {
            return Err(CudaAlgebraError::WorkspaceLimitExceeded {
                required: bytes,
                maximum: free,
            });
        }
        let grown = check_allocation(calls.allocate(bytes)?)?;
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
/// none; a negative size is an `InvalidNativeResult`.
fn asked(
    calls: &dyn NativeCalls,
    handle: OpaqueHandle,
    workspace: OpaqueHandle,
    slot: WorkspaceSlot,
) -> Result<Option<usize>, CudaAlgebraError> {
    let bytes = calls.workspace_size(handle, workspace, slot)?;
    if bytes < 0 {
        return Err(CudaAlgebraError::InvalidNativeResult {
            reason: format!("the workspace's {slot:?} slot asks for {bytes} bytes"),
        });
    }
    if bytes == 0 {
        return Ok(None);
    }
    usize::try_from(bytes)
        .map(Some)
        .map_err(|_| CudaAlgebraError::ResourceSizeOverflow {
            resource: "workspace",
        })
}

/// `bytes` zeroed bytes of host memory, or `None` if the allocator refuses
/// them (beyond `isize::MAX` bytes, a `Vec` cannot hold them), so the
/// call's cleanup runs, where `vec!` would abort. Memory the system
/// over-commits, or a memory cap, may still end the process once `resize`
/// touches the pages: it writes every byte, where `vec!` would let the system
/// zero them on first use, a cost only a large host slot would show.
fn host_memory(bytes: usize) -> Option<Vec<u8>> {
    let mut memory = Vec::new();
    memory.try_reserve_exact(bytes).ok()?;
    memory.resize(bytes, 0);
    Some(memory)
}

/// Lends host slot `slot` of `workspace` `memory`, from its first
/// `HOST_ALIGNMENT`-aligned byte.
fn lend_host(
    calls: &dyn NativeCalls,
    handle: OpaqueHandle,
    workspace: OpaqueHandle,
    slot: WorkspaceSlot,
    memory: &mut [u8],
) -> Result<(), CudaAlgebraError> {
    let offset = memory.as_ptr().align_offset(HOST_ALIGNMENT);
    let lent = size_i64(memory.len() - offset)?;
    let aligned = NonNull::from(&mut memory[offset]).cast();
    calls.set_workspace(handle, workspace, slot, aligned, lent)
}

fn size_i64(bytes: usize) -> Result<i64, CudaAlgebraError> {
    i64::try_from(bytes).map_err(|_| CudaAlgebraError::ResourceSizeOverflow {
        resource: "workspace",
    })
}
