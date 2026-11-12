// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Device memory and workspace descriptors: the CUDA and cuTensorNet calls
//! that allocate device buffers, copy into them, and lend scratch to
//! cuTensorNet.
//!
//! cuTensorNet allocates no memory of its own: its caller allocates the
//! scratch a compute or an expectation works in and lends it through a
//! workspace descriptor, at the size cuTensorNet recommends
//! (`CUTENSORNET_WORKSIZE_PREF_RECOMMENDED`: "the most suitable algorithm
//! will be available for each contraction"). The engine allocates, lends and
//! frees every buffer, so it decides how much a state holds
//! (`native_state.rs`).
//!
//! The copies run on CUDA's legacy default stream, which the engine's
//! non-blocking stream does not wait for, and may still be running when
//! they return: a copy from pageable host memory returns once its source is
//! staged, a copy between device buffers may return at once. So the engine
//! calls [`synchronize_copies`](MemoryCalls::synchronize_copies)
//! before any work on its stream reads what they wrote.
//!
//! A caller keeps two rules: it frees no scratch a workspace descriptor
//! still holds, and it starts no compute before the copies are done.

use super::{OpaqueHandle, complex::Complex64Abi};
use crate::engine::EngineError;

/// The CUDA Runtime and cuTensorNet calls on device memory and workspace
/// descriptors, as the engine makes them. Every implementer of
/// `StateCalls` implements them.
pub(crate) trait MemoryCalls {
    /// The device's free and total memory, in bytes (`cudaMemGetInfo`).
    fn memory_info(&self) -> Result<(usize, usize), EngineError>;
    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, EngineError>;
    fn free(&self, allocation: OpaqueHandle) -> Result<(), EngineError>;
    /// Copies `source` to the device; the host slice may be reused when it
    /// returns, but the copy may not have reached the device yet.
    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), EngineError>;
    /// Copies `bytes` from one device allocation to another; the copy may
    /// still be running when it returns.
    fn copy_on_device(
        &self,
        destination: OpaqueHandle,
        source: OpaqueHandle,
        bytes: usize,
    ) -> Result<(), EngineError>;
    /// Waits for every copy made so far (`cudaStreamSynchronize` on
    /// `cudaStreamLegacy`).
    fn synchronize_copies(&self) -> Result<(), EngineError>;
    fn create_workspace(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, EngineError>;
    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), EngineError>;
    /// The device scratch cuTensorNet recommends for what `workspace` was
    /// last prepared for.
    fn workspace_size(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<i64, EngineError>;
    /// Lends `bytes` of the device allocation `scratch` to `workspace`.
    fn set_workspace(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        scratch: OpaqueHandle,
        bytes: i64,
    ) -> Result<(), EngineError>;
}
