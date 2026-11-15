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
//! scratch or cache, each lent apart; a slot cuTensorNet recommends 0 bytes
//! for is left empty. The caller allocates, lends and frees every buffer, so
//! it decides how much memory it holds: the engine for its states
//! (`engine/native_state.rs`), which lend only device scratch.
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
use crate::native_calls::error::NativeError;

/// The CUDA Runtime and cuTensorNet calls on device memory and workspace
/// descriptors, as their callers make them. Every implementer of
/// `StateCalls` and `TensorCalls` implements them.
pub(crate) trait MemoryCalls {
    /// The device's free and total memory, in bytes (`cudaMemGetInfo`).
    fn memory_info(&self) -> Result<(usize, usize), NativeError>;
    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, NativeError>;
    fn free(&self, allocation: OpaqueHandle) -> Result<(), NativeError>;
    /// Copies `source` to the device; the host slice may be reused when it
    /// returns, but the copy may not have reached the device yet.
    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), NativeError>;
    /// Copies `bytes` from one device allocation to another; the copy may
    /// still be running when it returns.
    fn copy_on_device(
        &self,
        destination: OpaqueHandle,
        source: OpaqueHandle,
        bytes: usize,
    ) -> Result<(), NativeError>;
    /// Waits for every copy made so far (`cudaStreamSynchronize` on
    /// `cudaStreamLegacy`).
    fn synchronize_copies(&self) -> Result<(), NativeError>;
    /// Copies the first `destination.len()` elements of the device
    /// allocation `source` to the host; the copy has finished when it
    /// returns.
    #[allow(
        dead_code,
        reason = "its planned caller, the canonical MPS's CudaAlgebra, is not written yet"
    )]
    fn copy_to_host(
        &self,
        destination: &mut [Complex64Abi],
        source: OpaqueHandle,
    ) -> Result<(), NativeError>;
    fn create_workspace(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, NativeError>;
    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), NativeError>;
    /// The bytes cuTensorNet recommends for `slot` of `workspace`, for what
    /// the workspace was last prepared for; 0 when the slot is not needed.
    fn workspace_size(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        slot: WorkspaceSlot,
    ) -> Result<i64, NativeError>;
    /// Lends `bytes` of `memory` to `slot` of `workspace`: a device
    /// allocation for a device slot, host memory for a host slot.
    fn set_workspace(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        slot: WorkspaceSlot,
        memory: OpaqueHandle,
        bytes: i64,
    ) -> Result<(), NativeError>;
}

/// One of the four memory slots of a workspace descriptor: where its memory
/// lives (`cutensornetMemspace_t`) and what cuTensorNet keeps in it
/// (`cutensornetWorkspaceKind_t`). Scratch holds what one call needs while
/// it runs; a cache holds what cuTensorNet may reuse across calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceSlot {
    DeviceScratch,
    #[allow(
        dead_code,
        reason = "the engine lends device scratch only; the tensor API's planned caller may lend any slot"
    )]
    DeviceCache,
    #[allow(
        dead_code,
        reason = "the engine lends device scratch only; the tensor API's planned caller may lend any slot"
    )]
    HostScratch,
    #[allow(
        dead_code,
        reason = "the engine lends device scratch only; the tensor API's planned caller may lend any slot"
    )]
    HostCache,
}
