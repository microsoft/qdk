//! Device storage and workspace descriptors used by the engine, independent
//! of the state and its policy.
//!
//! [`MemoryWorkspaceApi`] is a private injection seam: `library::engine`
//! implements it with cuTensorNet and CUDA, and host tests with test doubles.
//! Only device scratch at the recommended size is modeled, because that is all
//! the MPS uses. Callers own every allocation, validate copy lengths and keep
//! attached memory alive until native use and descriptor destruction end.

use super::{EngineError, OpaqueHandle, ffi::Complex64Abi};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MemorySpace {
    Device,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceKind {
    Scratch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspacePreference {
    Recommended,
}

/// Private injection seam. Callers own allocations, validate copy lengths and
/// retain attached memory through native use and descriptor destruction.
pub(crate) trait MemoryWorkspaceApi {
    fn memory_info(&self) -> Result<(usize, usize), EngineError>;
    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, EngineError>;
    fn free(&self, allocation: OpaqueHandle) -> Result<(), EngineError>;
    /// Synchronous upload; no host pointer is retained after return.
    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), EngineError>;
    fn create_workspace(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, EngineError>;
    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), EngineError>;
    fn workspace_memory_size(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        preference: WorkspacePreference,
        space: MemorySpace,
        kind: WorkspaceKind,
    ) -> Result<i64, EngineError>;
    fn set_workspace_memory(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        space: MemorySpace,
        kind: WorkspaceKind,
        allocation: Option<OpaqueHandle>,
        bytes: i64,
    ) -> Result<(), EngineError>;
}
