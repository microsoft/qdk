//! Shared device storage and workspace operations, independent of State/MPS.

use super::{OpaqueHandle, SimulationError, ffi::Complex64Abi};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MemorySpace {
    Device,
    Host,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceKind {
    Scratch,
    Cache,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspacePreference {
    Minimum,
    Recommended,
}

/// Private injection seam. Callers own allocations, validate copy lengths and
/// retain attached memory through native use and descriptor destruction.
pub(crate) trait MemoryWorkspaceApi {
    fn memory_info(&self) -> Result<(usize, usize), SimulationError>;
    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, SimulationError>;
    fn free(&self, allocation: OpaqueHandle) -> Result<(), SimulationError>;
    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), SimulationError>;
    fn copy_from_device(
        &self,
        source: OpaqueHandle,
        destination: &mut [Complex64Abi],
    ) -> Result<(), SimulationError>;
    fn create_workspace(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), SimulationError>;
    fn workspace_memory_size(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        preference: WorkspacePreference,
        space: MemorySpace,
        kind: WorkspaceKind,
    ) -> Result<i64, SimulationError>;
    fn set_workspace_memory(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        space: MemorySpace,
        kind: WorkspaceKind,
        allocation: Option<OpaqueHandle>,
        bytes: i64,
    ) -> Result<(), SimulationError>;
}
