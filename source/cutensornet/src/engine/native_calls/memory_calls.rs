// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Device memory and workspace descriptors: the CUDA and cuTensorNet calls
//! that allocate device buffers, copy into and out of them, and lend
//! scratch to cuTensorNet.
//!
//! cuTensorNet allocates no memory of its own: its caller allocates the
//! memory a call works in and lends it through a workspace descriptor, at
//! the size cuTensorNet recommends (`CUTENSORNET_WORKSIZE_PREF_RECOMMENDED`:
//! "the most suitable algorithm will be available for each contraction").
//! A descriptor has four slots ([`WorkspaceSlot`]), device or host memory,
//! scratch or cache, each lent apart. The caller allocates, lends and frees
//! every buffer, so it decides how much memory it holds: the CUDA algebra, for
//! its tensors and its workspace (`cuda_algebra/`).
//!
//! The copies run on CUDA's legacy default stream, which a session's
//! non-blocking stream does not wait for. A copy to the device may still be
//! running when it returns: a copy from pageable host memory returns once
//! its source is staged, a copy between device buffers may return at once.
//! So the caller calls [`synchronize_copies`](MemoryCalls::synchronize_copies)
//! before any work on its stream reads what they wrote. A copy to the host
//! has finished when it returns, but does not wait for the session's
//! stream: the caller synchronizes that stream first.
//!
//! A caller keeps three rules: it frees no memory a workspace descriptor
//! still holds, it starts no work on its stream before the copies are done,
//! and it copies nothing to the host before the work that writes it is done.

use super::{OpaqueHandle, complex::Complex64Abi};
use crate::engine::CudaAlgebraError;

/// The CUDA Runtime and cuTensorNet calls on device memory and workspace
/// descriptors, as their callers make them. Every implementer of
/// `TensorCalls` implements them.
pub(crate) trait MemoryCalls {
    /// The device's free and total memory, in bytes (`cudaMemGetInfo`).
    fn memory_info(&self) -> Result<(usize, usize), CudaAlgebraError>;
    /// A new device buffer of `bytes` (`cudaMalloc`): a tensor, a call's
    /// intermediate or a device slot's memory.
    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, CudaAlgebraError>;
    /// Frees `allocation` (`cudaFree`), once, after the work and copies that
    /// use it, and after no workspace descriptor holds it.
    fn free(&self, allocation: OpaqueHandle) -> Result<(), CudaAlgebraError>;
    /// Copies `source` to the device; the host slice may be reused when it
    /// returns, but the copy may not have reached the device yet. An empty
    /// `source` is refused as an invalid call.
    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), CudaAlgebraError>;
    /// Copies `bytes` from one device allocation to another; the copy may
    /// still be running when it returns.
    fn copy_on_device(
        &self,
        destination: OpaqueHandle,
        source: OpaqueHandle,
        bytes: usize,
    ) -> Result<(), CudaAlgebraError>;
    /// Waits for every copy made so far (`cudaStreamSynchronize` on
    /// `cudaStreamLegacy`).
    fn synchronize_copies(&self) -> Result<(), CudaAlgebraError>;
    /// Copies the first `destination.len()` elements of the device
    /// allocation `source` to the host; the copy has finished when it
    /// returns. An empty `destination` is refused as an invalid call.
    fn copy_to_host(
        &self,
        destination: &mut [Complex64Abi],
        source: OpaqueHandle,
    ) -> Result<(), CudaAlgebraError>;
    /// A new workspace descriptor of `handle`, its four slots empty
    /// (`cutensornetCreateWorkspaceDescriptor`).
    fn create_workspace(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, CudaAlgebraError>;
    /// Releases `workspace` (`cutensornetDestroyWorkspaceDescriptor`), once,
    /// before the memory lent to it is freed.
    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), CudaAlgebraError>;
    /// The bytes cuTensorNet recommends for `slot` of `workspace`, for the
    /// call the workspace was last sized for (a tensor call's
    /// `…_workspace_sizes`); 0 when the slot is not needed.
    fn workspace_size(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        slot: WorkspaceSlot,
    ) -> Result<i64, CudaAlgebraError>;
    /// Lends `bytes` of `memory` to `slot` of `workspace`: a device
    /// allocation for a device slot, host memory for a host slot.
    fn set_workspace(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        slot: WorkspaceSlot,
        memory: OpaqueHandle,
        bytes: i64,
    ) -> Result<(), CudaAlgebraError>;
}

/// One of the four memory slots of a workspace descriptor: where its memory
/// lives (`cutensornetMemspace_t`) and what cuTensorNet keeps in it
/// (`cutensornetWorkspaceKind_t`). Scratch holds what one call needs while
/// it runs; a cache holds what cuTensorNet may reuse across calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceSlot {
    DeviceScratch,
    DeviceCache,
    HostScratch,
    HostCache,
}
