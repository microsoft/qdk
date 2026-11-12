// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `MemoryCalls` of the fake libraries: device buffers, copies and
//! workspaces.

use super::{FakeLibraries, Object, WORKSPACE_BYTES, failed, id, to_usize};
use crate::engine::{
    EngineError,
    native_calls::{OpaqueHandle, complex::Complex64Abi, memory_calls::MemoryCalls},
};

impl MemoryCalls for FakeLibraries {
    fn memory_info(&self) -> Result<(usize, usize), EngineError> {
        let inner = self.call("memory_info")?;
        Ok((inner.free_memory.unwrap_or(1 << 40), 1 << 40))
    }

    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, EngineError> {
        let elements = bytes.div_ceil(size_of::<Complex64Abi>());
        Ok(self
            .call("allocate")?
            .create(Object::Buffer(vec![Complex64Abi::default(); elements])))
    }

    fn free(&self, allocation: OpaqueHandle) -> Result<(), EngineError> {
        let mut inner = self.call("free")?;
        if inner.stream_busy {
            return Err(failed("free", "the stream may still be using the buffer"));
        }
        if inner.copy_sources.contains(&id(allocation)) {
            return Err(failed("free", "a copy may still be reading the buffer"));
        }
        if inner.objects.values().any(|object| {
            matches!(
                object,
                Object::Workspace { scratch: Some((scratch, _)) } if *scratch == id(allocation)
            )
        }) {
            return Err(failed("free", "the buffer is attached to a live workspace"));
        }
        inner.mps_tensors.remove(&id(allocation));
        inner.destroy(allocation, "free", |o| matches!(o, Object::Buffer(_)))
    }

    fn copy_on_device(
        &self,
        destination: OpaqueHandle,
        source: OpaqueHandle,
        bytes: usize,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("copy_on_device")?;
        let elements = bytes / size_of::<Complex64Abi>();
        let source_buffer = inner.buffer(id(source), "copy_on_device")?;
        let whole = elements == source_buffer.len();
        let copied = source_buffer
            .get(..elements)
            .ok_or_else(|| failed("copy_on_device", "the source is too small"))?
            .to_vec();
        match inner.objects.get_mut(&id(destination)) {
            Some(Object::Buffer(buffer)) if buffer.len() >= elements => {
                buffer[..elements].copy_from_slice(&copied);
            }
            _ => return Err(failed("copy_on_device", "no such buffer of that size")),
        }
        // Only a copy of a whole tensor holds the computed MPS's site.
        match inner.mps_tensors.get(&id(source)).copied() {
            Some(tag) if whole => inner.mps_tensors.insert(id(destination), tag),
            _ => inner.mps_tensors.remove(&id(destination)),
        };
        inner.copies_running = true;
        inner.copy_destinations.push(id(destination));
        inner.copy_sources.push(id(source));
        Ok(())
    }

    fn synchronize_copies(&self) -> Result<(), EngineError> {
        let mut inner = self.call("synchronize_copies")?;
        inner.copies_running = false;
        inner.copy_sources.clear();
        Ok(())
    }

    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), EngineError> {
        let mut inner = self.call("copy_to_device")?;
        match inner.objects.get_mut(&id(destination)) {
            Some(Object::Buffer(buffer)) if buffer.len() >= source.len() => {
                buffer[..source.len()].copy_from_slice(source);
            }
            _ => return Err(failed("copy_to_device", "no such buffer of that size")),
        }
        inner.copies_running = true;
        Ok(())
    }

    fn create_workspace(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, EngineError> {
        self.call("create_workspace")?.create_on(
            handle,
            "create_workspace",
            Object::Workspace { scratch: None },
        )
    }

    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_workspace")?
            .destroy(workspace, "destroy_workspace", |o| {
                matches!(o, Object::Workspace { .. })
            })
    }

    fn workspace_size(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<i64, EngineError> {
        let inner = self.call("workspace_size")?;
        inner.require(workspace, "workspace_size", |o| {
            matches!(o, Object::Workspace { .. })
        })?;
        inner.owned(handle, workspace, "workspace_size")?;
        Ok(inner.workspace_bytes.unwrap_or(WORKSPACE_BYTES))
    }

    fn set_workspace(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        scratch: OpaqueHandle,
        bytes: i64,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("set_workspace")?;
        inner.owned(handle, workspace, "set_workspace")?;
        let allocation = id(scratch);
        if size_of_val(inner.buffer(allocation, "set_workspace")?) < to_usize(bytes)? {
            return Err(failed(
                "set_workspace",
                "the scratch is smaller than its size",
            ));
        }
        match inner.objects.get_mut(&id(workspace)) {
            Some(Object::Workspace { scratch }) => {
                *scratch = Some((allocation, bytes));
            }
            _ => return Err(failed("set_workspace", "no such workspace")),
        }
        if !inner.scratch.contains(&allocation) {
            inner.scratch.push(allocation);
        }
        Ok(())
    }
}
