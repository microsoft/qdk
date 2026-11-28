// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `MemoryCalls` on the loaded libraries: device memory, copies and
//! cuTensorNet workspaces, whose four slots map to cuTensorNet's memspace
//! and workspace kind.

use crate::engine::CudaAlgebraError;
use crate::engine::native_calls::{
    NativePointer,
    complex::{Complex64Abi, complex_bytes},
    memory_calls::{
        DeviceBuffer, MemoryCalls, WorkspaceDescriptor, WorkspaceMemory, WorkspaceSlot,
    },
    session_calls::CuTensorNetHandle,
};
use crate::libraries::{
    LoadedLibraries,
    cuda_runtime::{self, cudart_12},
    cutensornet::v2_13,
};

use super::created;

impl MemoryCalls for LoadedLibraries {
    fn memory_info(&self) -> Result<(usize, usize), CudaAlgebraError> {
        let mut free = 0;
        let mut total = 0;
        // SAFETY: both arguments are valid writable `size_t` out-pointers.
        let status =
            unsafe { (self.cuda_runtime.functions.cudaMemGetInfo)(&raw mut free, &raw mut total) };
        self.check_cuda("cudaMemGetInfo", status)?;
        Ok((free, total))
    }

    fn allocate(&self, bytes: usize) -> Result<DeviceBuffer, CudaAlgebraError> {
        let mut allocation = std::ptr::null_mut();
        // SAFETY: `allocation` is a valid writable out-pointer, and
        // `cudaMalloc` takes any size by value: one it cannot serve is an
        // error status, and a null allocation is refused below.
        let status =
            unsafe { (self.cuda_runtime.functions.cudaMalloc)(&raw mut allocation, bytes) };
        self.check_cuda("cudaMalloc", status)?;
        created(allocation, "cudaMalloc", "device allocation")
    }

    fn free(&self, allocation: DeviceBuffer) -> Result<(), CudaAlgebraError> {
        // SAFETY: the CUDA algebra, which allocated it, frees each
        // successful cudaMalloc allocation exactly once.
        let status = unsafe { (self.cuda_runtime.functions.cudaFree)(allocation.as_ptr()) };
        self.check_cuda("cudaFree", status)
    }

    fn copy_to_device(
        &self,
        destination: DeviceBuffer,
        source: &[Complex64Abi],
    ) -> Result<(), CudaAlgebraError> {
        let bytes = complex_bytes(source.len(), "complex tensor")?;
        // SAFETY: the retained destination allocation is at least `bytes`
        // long, and `source` is a live host slice: a copy from pageable host
        // memory returns once the slice has been staged.
        let status = unsafe {
            (self.cuda_runtime.functions.cudaMemcpy)(
                destination.as_ptr(),
                source.as_ptr().cast(),
                bytes,
                cudart_12::cudaMemcpyKind_cudaMemcpyHostToDevice,
            )
        };
        self.check_cuda("cudaMemcpy(H2D)", status)
    }

    fn copy_on_device(
        &self,
        destination: DeviceBuffer,
        source: DeviceBuffer,
        bytes: usize,
    ) -> Result<(), CudaAlgebraError> {
        // SAFETY: both allocations are live and at least `bytes` long, and
        // the stream that wrote `source` has been synchronized.
        let status = unsafe {
            (self.cuda_runtime.functions.cudaMemcpy)(
                destination.as_ptr(),
                source.as_ptr(),
                bytes,
                cudart_12::cudaMemcpyKind_cudaMemcpyDeviceToDevice,
            )
        };
        self.check_cuda("cudaMemcpy(D2D)", status)
    }

    fn synchronize_copies(&self) -> Result<(), CudaAlgebraError> {
        let legacy = std::ptr::without_provenance_mut(cuda_runtime::STREAM_LEGACY);
        // SAFETY: `cudaStreamLegacy` is a valid stream for every CUDA
        // Runtime call that takes one.
        let status = unsafe { (self.cuda_runtime.functions.cudaStreamSynchronize)(legacy) };
        self.check_cuda("cudaStreamSynchronize(cudaStreamLegacy)", status)
    }

    fn copy_to_host(
        &self,
        destination: &mut [Complex64Abi],
        source: DeviceBuffer,
    ) -> Result<(), CudaAlgebraError> {
        let bytes = complex_bytes(destination.len(), "complex tensor")?;
        // SAFETY: the source allocation is live and at least `bytes` long,
        // its writer has been synchronized, and `destination` is a live,
        // writable host slice of `bytes`: a copy to pageable host memory
        // returns once it has finished.
        let status = unsafe {
            (self.cuda_runtime.functions.cudaMemcpy)(
                destination.as_mut_ptr().cast(),
                source.as_ptr(),
                bytes,
                cudart_12::cudaMemcpyKind_cudaMemcpyDeviceToHost,
            )
        };
        self.check_cuda("cudaMemcpy(D2H)", status)
    }

    fn create_workspace(
        &self,
        handle: CuTensorNetHandle,
    ) -> Result<WorkspaceDescriptor, CudaAlgebraError> {
        let mut workspace = std::ptr::null_mut();
        // SAFETY: the handle is live and workspace is a writable out-pointer.
        let status = unsafe {
            (self
                .cutensornet
                .functions
                .cutensornetCreateWorkspaceDescriptor)(
                handle.as_ptr(), &raw mut workspace
            )
        };
        self.check_cutensornet("cutensornetCreateWorkspaceDescriptor", status)?;
        created(
            workspace,
            "cutensornetCreateWorkspaceDescriptor",
            "workspace descriptor",
        )
    }

    fn destroy_workspace(&self, workspace: WorkspaceDescriptor) -> Result<(), CudaAlgebraError> {
        // SAFETY: the owner consumes this descriptor exactly once.
        let status = unsafe {
            (self
                .cutensornet
                .functions
                .cutensornetDestroyWorkspaceDescriptor)(workspace.as_ptr())
        };
        self.check_cutensornet("cutensornetDestroyWorkspaceDescriptor", status)
    }

    fn workspace_size(
        &self,
        handle: CuTensorNetHandle,
        workspace: WorkspaceDescriptor,
        slot: WorkspaceSlot,
    ) -> Result<i64, CudaAlgebraError> {
        let preference = v2_13::cutensornetWorksizePref_t_CUTENSORNET_WORKSIZE_PREF_RECOMMENDED;
        let (memspace, kind) = native_slot(slot);
        let mut bytes = 0;
        // SAFETY: live, sized workspace; bytes is a writable int64_t.
        let status = unsafe {
            (self.cutensornet.functions.cutensornetWorkspaceGetMemorySize)(
                handle.as_ptr(),
                workspace.as_ptr(),
                preference,
                memspace,
                kind,
                &raw mut bytes,
            )
        };
        self.check_cutensornet("cutensornetWorkspaceGetMemorySize", status)?;
        Ok(bytes)
    }

    fn set_workspace(
        &self,
        handle: CuTensorNetHandle,
        workspace: WorkspaceDescriptor,
        slot: WorkspaceSlot,
        memory: WorkspaceMemory,
        bytes: i64,
    ) -> Result<(), CudaAlgebraError> {
        let (memspace, kind) = native_slot(slot);
        // SAFETY: `memory` is live memory of the slot's memspace, at least
        // `bytes` long, which its owner keeps until the descriptor is
        // destroyed or lent other memory for the slot.
        let status = unsafe {
            (self.cutensornet.functions.cutensornetWorkspaceSetMemory)(
                handle.as_ptr(),
                workspace.as_ptr(),
                memspace,
                kind,
                memory.as_ptr(),
                bytes,
            )
        };
        self.check_cutensornet("cutensornetWorkspaceSetMemory", status)
    }
}

/// cuTensorNet's memspace and workspace kind for `slot`.
fn native_slot(
    slot: WorkspaceSlot,
) -> (
    v2_13::cutensornetMemspace_t,
    v2_13::cutensornetWorkspaceKind_t,
) {
    match slot {
        WorkspaceSlot::DeviceScratch => (
            v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_DEVICE,
            v2_13::cutensornetWorkspaceKind_t_CUTENSORNET_WORKSPACE_SCRATCH,
        ),
        WorkspaceSlot::DeviceCache => (
            v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_DEVICE,
            v2_13::cutensornetWorkspaceKind_t_CUTENSORNET_WORKSPACE_CACHE,
        ),
        WorkspaceSlot::HostScratch => (
            v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_HOST,
            v2_13::cutensornetWorkspaceKind_t_CUTENSORNET_WORKSPACE_SCRATCH,
        ),
        WorkspaceSlot::HostCache => (
            v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_HOST,
            v2_13::cutensornetWorkspaceKind_t_CUTENSORNET_WORKSPACE_CACHE,
        ),
    }
}
