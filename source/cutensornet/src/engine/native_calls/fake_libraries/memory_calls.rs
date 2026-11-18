// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `MemoryCalls` of the fake libraries: device buffers, copies and
//! workspaces, whose slots ask for what the fake was told
//! (`FakeLibraries::set_workspace_bytes` and the setters beside it).

use super::{FakeLibraries, Object, failed, id, is_device, pointer, slot_index, to_usize};
use crate::engine::CudaAlgebraError;
use crate::engine::native_calls::{
    OpaqueHandle,
    complex::{Complex64Abi, complex_bytes},
    memory_calls::{MemoryCalls, WorkspaceSlot},
};

impl MemoryCalls for FakeLibraries {
    fn memory_info(&self) -> Result<(usize, usize), CudaAlgebraError> {
        let inner = self.call("memory_info")?;
        Ok((inner.free_memory.unwrap_or(1 << 40), 1 << 40))
    }

    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, CudaAlgebraError> {
        let elements = bytes.div_ceil(size_of::<Complex64Abi>());
        let mut inner = self.call("allocate")?;
        if let Some((left, aliased)) = inner.aliased_allocation.take() {
            if left == 1 {
                return Ok(pointer(aliased));
            }
            inner.aliased_allocation = Some((left - 1, aliased));
        }
        Ok(inner.create(Object::Buffer(vec![Complex64Abi::default(); elements])))
    }

    fn free(&self, allocation: OpaqueHandle) -> Result<(), CudaAlgebraError> {
        let mut inner = self.call("free")?;
        if inner.any_stream_busy() {
            return Err(failed("free", "a stream may still be using the buffer"));
        }
        if inner.copy_sources.contains(&id(allocation)) {
            return Err(failed("free", "a copy may still be reading the buffer"));
        }
        if inner.objects.values().any(|object| match object {
            Object::Workspace { lent, .. } => super::WORKSPACE_SLOTS.iter().any(|&slot| {
                is_device(slot)
                    && lent[slot_index(slot)].is_some_and(|(memory, _)| memory == id(allocation))
            }),
            _ => false,
        }) {
            return Err(failed("free", "the buffer is attached to a live workspace"));
        }
        inner.destroy(allocation, "free", |o| matches!(o, Object::Buffer(_)))
    }

    fn copy_on_device(
        &self,
        destination: OpaqueHandle,
        source: OpaqueHandle,
        bytes: usize,
    ) -> Result<(), CudaAlgebraError> {
        let mut inner = self.call("copy_on_device")?;
        if inner.write_pending(id(source)) || inner.write_pending(id(destination)) {
            return Err(failed(
                "copy_on_device",
                "the stream writing the source or the destination is not synchronized",
            ));
        }
        let elements = bytes / size_of::<Complex64Abi>();
        let copied = inner
            .buffer(id(source), "copy_on_device")?
            .get(..elements)
            .ok_or_else(|| failed("copy_on_device", "the source is too small"))?
            .to_vec();
        match inner.objects.get_mut(&id(destination)) {
            Some(Object::Buffer(buffer)) if buffer.len() >= elements => {
                buffer[..elements].copy_from_slice(&copied);
            }
            _ => return Err(failed("copy_on_device", "no such buffer of that size")),
        }
        inner.copies_running = true;
        inner.copy_sources.push(id(source));
        Ok(())
    }

    fn synchronize_copies(&self) -> Result<(), CudaAlgebraError> {
        let mut inner = self.call("synchronize_copies")?;
        inner.copies_running = false;
        inner.copy_sources.clear();
        Ok(())
    }

    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), CudaAlgebraError> {
        // The loaded libraries refuse an empty copy before any native call.
        complex_bytes(source.len(), "complex tensor")?;
        let mut inner = self.call("copy_to_device")?;
        if inner.write_pending(id(destination)) {
            return Err(failed(
                "copy_to_device",
                "the stream writing the destination is not synchronized",
            ));
        }
        match inner.objects.get_mut(&id(destination)) {
            Some(Object::Buffer(buffer)) if buffer.len() >= source.len() => {
                buffer[..source.len()].copy_from_slice(source);
            }
            _ => return Err(failed("copy_to_device", "no such buffer of that size")),
        }
        inner.copies_running = true;
        Ok(())
    }

    fn copy_to_host(
        &self,
        destination: &mut [Complex64Abi],
        source: OpaqueHandle,
    ) -> Result<(), CudaAlgebraError> {
        // The loaded libraries refuse an empty copy before any native call.
        complex_bytes(destination.len(), "complex tensor")?;
        let inner = self.call("copy_to_host")?;
        if inner.write_pending(id(source)) {
            return Err(failed(
                "copy_to_host",
                "the stream writing the buffer is not synchronized",
            ));
        }
        let copied = inner
            .buffer(id(source), "copy_to_host")?
            .get(..destination.len())
            .ok_or_else(|| failed("copy_to_host", "the source is too small"))?;
        destination.copy_from_slice(copied);
        Ok(())
    }

    fn create_workspace(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, CudaAlgebraError> {
        self.call("create_workspace")?.create_on(
            handle,
            "create_workspace",
            Object::Workspace {
                lent: [None; 4],
                sized_for: None,
            },
        )
    }

    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), CudaAlgebraError> {
        self.call("destroy_workspace")?
            .destroy(workspace, "destroy_workspace", |o| {
                matches!(o, Object::Workspace { .. })
            })
    }

    fn workspace_size(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        slot: WorkspaceSlot,
    ) -> Result<i64, CudaAlgebraError> {
        let inner = self.call("workspace_size")?;
        inner.require(workspace, "workspace_size", |o| {
            matches!(o, Object::Workspace { .. })
        })?;
        inner.owned(handle, workspace, "workspace_size")?;
        Ok(inner.recommended_bytes(slot))
    }

    /// Refuses memory, of any size, for a slot that asks for none. The
    /// evidence is narrower: on an A100, cuTensorNet refuses a zero-size
    /// workspace with a pointer (status 7, an invalid workspace).
    fn set_workspace(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        slot: WorkspaceSlot,
        memory: OpaqueHandle,
        bytes: i64,
    ) -> Result<(), CudaAlgebraError> {
        let mut inner = self.call("set_workspace")?;
        inner.owned(handle, workspace, "set_workspace")?;
        if inner.recommended_bytes(slot) <= 0 {
            return Err(failed("set_workspace", "the slot asks for no memory"));
        }
        let memory = id(memory);
        if is_device(slot) && size_of_val(inner.buffer(memory, "set_workspace")?) < to_usize(bytes)?
        {
            return Err(failed(
                "set_workspace",
                "the scratch is smaller than its size",
            ));
        }
        match inner.objects.get_mut(&id(workspace)) {
            Some(Object::Workspace { lent, .. }) => {
                lent[slot_index(slot)] = Some((memory, bytes));
            }
            _ => return Err(failed("set_workspace", "no such workspace")),
        }
        if slot == WorkspaceSlot::DeviceScratch && !inner.scratch.contains(&memory) {
            inner.scratch.push(memory);
        }
        Ok(())
    }
}
