// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Device memory and workspace descriptors: the CUDA and cuTensorNet calls
//! that allocate device buffers, copy into and out of them, and lend
//! scratch to cuTensorNet.
//!
//! cuTensorNet allocates no memory of its own. Its caller allocates the
//! memory a call works in, and lends it through a workspace descriptor at
//! the size cuTensorNet recommends (`CUTENSORNET_WORKSIZE_PREF_RECOMMENDED`:
//! "the most suitable algorithm will be available for each contraction").
//! A descriptor has four slots ([`WorkspaceSlot`]), device or host memory,
//! scratch or cache, each lent apart. The caller allocates, lends and frees
//! every buffer, so it decides how much memory it holds.
//!
//! The copies use `cudaMemcpy`, CUDA's simplest copy. It takes no stream,
//! so it runs on CUDA's legacy default stream. A session's stream is
//! non-blocking, so that its work never waits for other code's work on that
//! default stream. For the same reason, it does not wait for these copies.
//! A copy to the device may still be running when it returns. A copy from
//! pageable host memory returns once its source is staged; a copy between
//! device buffers may return at once. So the caller calls
//! [`synchronize_copies`](MemoryCalls::synchronize_copies) before any work
//! on its stream reads what they wrote. A copy to the host has finished
//! when it returns, but does not wait for the session's stream, so the
//! caller synchronizes that stream first.
//!
//! A caller keeps three rules:
//!
//! - it frees no memory a workspace descriptor still holds;
//! - it starts no work on its stream before the copies are done;
//! - it copies nothing to the host before the work that writes it is done.
//!
//! Alternatives to evaluate for a later version:
//!
//! - `cudaMemcpyAsync` on the session's stream. The stream would then order
//!   the copies with the work, so the last two rules and
//!   `synchronize_copies` would go. The loader would load one more function.
//! - Pinned host memory (`cudaMallocHost`), with which an asynchronous copy
//!   can overlap with the work on the device.

use super::{
    NativePointer, complex::Complex64Abi, native_pointer, session_calls::CuTensorNetHandle,
};
use crate::engine::CudaAlgebraError;
use std::{ffi::c_void, ptr::NonNull};

/// The CUDA Runtime and cuTensorNet calls on device memory and workspace
/// descriptors, as their callers make them. Every implementer of
/// `TensorCalls` implements them.
pub(crate) trait MemoryCalls {
    /// The device's free and total memory, in bytes (`cudaMemGetInfo`).
    fn memory_info(&self) -> Result<(usize, usize), CudaAlgebraError>;
    /// A new device buffer of `bytes` (`cudaMalloc`): a tensor, a call's
    /// intermediate or a device slot's memory.
    fn allocate(&self, bytes: usize) -> Result<DeviceBuffer, CudaAlgebraError>;
    /// Frees `allocation` (`cudaFree`), once, after the work and copies that
    /// use it, and after no workspace descriptor holds it.
    fn free(&self, allocation: DeviceBuffer) -> Result<(), CudaAlgebraError>;
    /// Copies `source` to the device. The host slice may be reused when it
    /// returns, but the copy may not have reached the device yet. An empty
    /// `source` is refused as an invalid call.
    fn copy_to_device(
        &self,
        destination: DeviceBuffer,
        source: &[Complex64Abi],
    ) -> Result<(), CudaAlgebraError>;
    /// Copies `bytes` from one device allocation to another. The copy may
    /// still be running when it returns.
    fn copy_on_device(
        &self,
        destination: DeviceBuffer,
        source: DeviceBuffer,
        bytes: usize,
    ) -> Result<(), CudaAlgebraError>;
    /// Waits for every copy made so far (`cudaStreamSynchronize` on
    /// `cudaStreamLegacy`).
    fn synchronize_copies(&self) -> Result<(), CudaAlgebraError>;
    /// Copies the first `destination.len()` elements of the device
    /// allocation `source` to the host. The copy has finished when it
    /// returns. An empty `destination` is refused as an invalid call.
    fn copy_to_host(
        &self,
        destination: &mut [Complex64Abi],
        source: DeviceBuffer,
    ) -> Result<(), CudaAlgebraError>;
    /// A new workspace descriptor of `handle`, its four slots empty
    /// (`cutensornetCreateWorkspaceDescriptor`).
    fn create_workspace(
        &self,
        handle: CuTensorNetHandle,
    ) -> Result<WorkspaceDescriptor, CudaAlgebraError>;
    /// Releases `workspace` (`cutensornetDestroyWorkspaceDescriptor`), once,
    /// before the memory lent to it is freed.
    fn destroy_workspace(&self, workspace: WorkspaceDescriptor) -> Result<(), CudaAlgebraError>;
    /// The bytes cuTensorNet recommends for `slot` of `workspace`, for the
    /// call the workspace was last sized for (a tensor call's
    /// `…_workspace_sizes`), or 0 when the slot is not needed.
    fn workspace_size(
        &self,
        handle: CuTensorNetHandle,
        workspace: WorkspaceDescriptor,
        slot: WorkspaceSlot,
    ) -> Result<i64, CudaAlgebraError>;
    /// Lends `bytes` of `memory` to `slot` of `workspace`: a device
    /// buffer for a device slot, host memory for a host slot.
    fn set_workspace(
        &self,
        handle: CuTensorNetHandle,
        workspace: WorkspaceDescriptor,
        slot: WorkspaceSlot,
        memory: WorkspaceMemory,
        bytes: i64,
    ) -> Result<(), CudaAlgebraError>;
}

/// One of the four memory slots of a workspace descriptor: where its memory
/// lives (`cutensornetMemspace_t`) and what cuTensorNet keeps in it
/// (`cutensornetWorkspaceKind_t`). Scratch holds what one call needs while
/// it runs. A cache holds what cuTensorNet may reuse across calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceSlot {
    DeviceScratch,
    DeviceCache,
    HostScratch,
    HostCache,
}

native_pointer! {
    /// A device buffer (`cudaMalloc`): a tensor, a call's intermediate or a
    /// device slot's memory.
    DeviceBuffer
}

native_pointer! {
    /// A cuTensorNet workspace descriptor
    /// (`cutensornetWorkspaceDescriptor_t`): the memory lent to a tensor
    /// call, slot by slot.
    WorkspaceDescriptor
}

/// The memory lent to a slot of a workspace descriptor: a device buffer for
/// a device slot, host memory for a host slot. Its owner keeps it until the
/// descriptor is destroyed or lent other memory for the slot.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceMemory(NonNull<c_void>);

impl WorkspaceMemory {
    /// The host memory `memory`, from its first byte.
    pub(crate) fn on_host(memory: &mut [u8]) -> Self {
        Self(NonNull::from(memory).cast())
    }

    /// The pointer to pass to cuTensorNet.
    pub(crate) fn as_ptr(self) -> *mut c_void {
        self.0.as_ptr()
    }
}

impl From<DeviceBuffer> for WorkspaceMemory {
    fn from(buffer: DeviceBuffer) -> Self {
        Self(buffer.pointer())
    }
}
