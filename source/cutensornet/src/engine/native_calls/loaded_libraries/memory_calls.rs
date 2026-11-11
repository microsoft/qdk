// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `MemoryCalls` on the loaded libraries: device memory, copies and
//! cuTensorNet workspaces.

use crate::engine::{
    EngineError,
    native_calls::{
        OpaqueHandle,
        complex::{Complex64Abi, complex_bytes},
        memory_calls::MemoryCalls,
    },
};
use crate::libraries::{
    LoadedLibraries,
    cuda_runtime::{self, cudart_12},
    cutensornet::v2_13,
};
use std::ptr::NonNull;

impl MemoryCalls for LoadedLibraries {
    fn memory_info(&self) -> Result<(usize, usize), EngineError> {
        Ok(self.cuda_runtime.memory_info()?)
    }

    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, EngineError> {
        let mut allocation = std::ptr::null_mut();
        // SAFETY: `allocation` is a valid writable out-pointer, and
        // `cudaMalloc` takes any size by value: one it cannot serve is an
        // error status, and a null allocation is refused below.
        let status = unsafe { (self.cuda_runtime.functions.malloc)(&raw mut allocation, bytes) };
        self.check_cuda("cudaMalloc", status)?;
        NonNull::new(allocation).ok_or(EngineError::MissingNativeResource {
            operation: "cudaMalloc",
            resource: "device allocation",
        })
    }

    fn free(&self, allocation: OpaqueHandle) -> Result<(), EngineError> {
        // SAFETY: `NativeState`, which owns every device allocation, frees
        // each successful cudaMalloc allocation exactly once.
        let status = unsafe { (self.cuda_runtime.functions.free)(allocation.as_ptr()) };
        self.check_cuda("cudaFree", status)
    }

    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), EngineError> {
        let bytes = complex_bytes(source.len(), "complex tensor")?;
        // SAFETY: the retained destination allocation is at least `bytes`
        // long, and `source` is a live host slice: a copy from pageable host
        // memory returns once the slice has been staged.
        let status = unsafe {
            (self.cuda_runtime.functions.memcpy)(
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
        destination: OpaqueHandle,
        source: OpaqueHandle,
        bytes: usize,
    ) -> Result<(), EngineError> {
        // SAFETY: both allocations are live and at least `bytes` long, and
        // the stream that wrote `source` has been synchronized.
        let status = unsafe {
            (self.cuda_runtime.functions.memcpy)(
                destination.as_ptr(),
                source.as_ptr(),
                bytes,
                cudart_12::cudaMemcpyKind_cudaMemcpyDeviceToDevice,
            )
        };
        self.check_cuda("cudaMemcpy(D2D)", status)
    }

    fn synchronize_copies(&self) -> Result<(), EngineError> {
        let legacy = std::ptr::without_provenance_mut(cuda_runtime::STREAM_LEGACY);
        // SAFETY: `cudaStreamLegacy` is a valid stream for every CUDA
        // Runtime call that takes one.
        let status = unsafe { (self.cuda_runtime.functions.stream_synchronize)(legacy) };
        self.check_cuda("cudaStreamSynchronize(cudaStreamLegacy)", status)
    }

    fn create_workspace(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, EngineError> {
        let mut workspace = std::ptr::null_mut();
        // SAFETY: the handle is live and workspace is a writable out-pointer.
        let status = unsafe {
            (self.cutensornet.functions.create_workspace)(handle.as_ptr(), &raw mut workspace)
        };
        self.check_cutensornet("cutensornetCreateWorkspaceDescriptor", status)?;
        NonNull::new(workspace).ok_or(EngineError::MissingNativeResource {
            operation: "cutensornetCreateWorkspaceDescriptor",
            resource: "workspace descriptor",
        })
    }

    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), EngineError> {
        // SAFETY: the owner consumes this descriptor exactly once.
        let status = unsafe { (self.cutensornet.functions.destroy_workspace)(workspace.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyWorkspaceDescriptor", status)
    }

    fn workspace_size(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<i64, EngineError> {
        let preference = v2_13::cutensornetWorksizePref_t_CUTENSORNET_WORKSIZE_PREF_RECOMMENDED;
        let mut bytes = 0;
        // SAFETY: live, sized workspace; bytes is a writable int64_t.
        let status = unsafe {
            (self.cutensornet.functions.workspace_get_memory_size)(
                handle.as_ptr(),
                workspace.as_ptr(),
                preference,
                v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_DEVICE,
                v2_13::cutensornetWorkspaceKind_t_CUTENSORNET_WORKSPACE_SCRATCH,
                &raw mut bytes,
            )
        };
        self.check_cutensornet("cutensornetWorkspaceGetMemorySize", status)?;
        Ok(bytes)
    }

    fn set_workspace(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        scratch: OpaqueHandle,
        bytes: i64,
    ) -> Result<(), EngineError> {
        // SAFETY: the scratch is a live device allocation of at least
        // `bytes`, which its owner keeps until the descriptor is destroyed.
        let status = unsafe {
            (self.cutensornet.functions.workspace_set_memory)(
                handle.as_ptr(),
                workspace.as_ptr(),
                v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_DEVICE,
                v2_13::cutensornetWorkspaceKind_t_CUTENSORNET_WORKSPACE_SCRATCH,
                scratch.as_ptr(),
                bytes,
            )
        };
        self.check_cutensornet("cutensornetWorkspaceSetMemory", status)
    }
}
