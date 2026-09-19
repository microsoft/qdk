//! Numerical execution of selected native metadata. Preparation never searches.

use super::{ContractionApi, ContractionResources, NativeMetadata, invalid, unexpected};
use crate::simulation::{
    OpaqueHandle, SimulationError, Stream,
    error::combine_execution_and_cleanup,
    ffi::Complex64Abi,
    memory_workspace::{MemorySpace, MemoryWorkspaceApi, WorkspaceKind, WorkspacePreference},
    resources::SessionApi,
};
use num_complex::Complex64;
use std::{alloc::Layout, ptr::NonNull};

#[cfg(test)]
#[path = "execution/qualification.rs"]
mod qualification;

/// Private native calls; buffers and workspace stay owned through synchronization
/// and network destruction. Null strides select the shared column-major layout.
pub(crate) trait ContractionExecutionApi:
    ContractionApi + SessionApi + MemoryWorkspaceApi
{
    fn compute_contraction_workspace(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), SimulationError>;
    fn bind_input(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        tensor_id: i64,
        allocation: OpaqueHandle,
    ) -> Result<(), SimulationError>;
    fn bind_output(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        allocation: OpaqueHandle,
    ) -> Result<(), SimulationError>;
    fn prepare_contraction(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), SimulationError>;
    /// Replaces output and contracts all slices; no user-managed slice groups.
    fn contract(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), SimulationError>;
}

/// `None` omits a scratch policy ceiling, not native allocation failure checks.
/// Neither field limits total process or device memory.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WorkspaceLimits {
    pub(crate) device_scratch: Option<usize>,
    pub(crate) host_scratch: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ExecutionMemory {
    pub(crate) coefficient_bytes: usize,
    pub(crate) unique_buffers: usize,
    pub(crate) output_bytes: usize,
    pub(crate) device_scratch_minimum: usize,
    pub(crate) device_scratch_recommended: usize,
    pub(crate) host_scratch_minimum: usize,
    pub(crate) host_scratch_recommended: usize,
    pub(crate) device_cache_recommended: usize,
    pub(crate) host_cache_recommended: usize,
    pub(crate) device_scratch_allocated: usize,
    pub(crate) host_scratch_allocated: usize,
    pub(crate) owned_device_bytes: usize,
}

/// Owns the topology together with every pointer attached to it. The consuming
/// transition prevents metadata mutation after kernel/workspace preparation.
pub(crate) struct ContractionExecution<'session, Api: ContractionExecutionApi> {
    resources: Option<ContractionResources<'session, Api>>,
    workspace: Option<OpaqueHandle>,
    allocations: Vec<OpaqueHandle>,
    host_scratch: Option<HostScratch>,
    output: Option<OpaqueHandle>,
    report: ExecutionMemory,
    usable: bool,
}

impl<'session, Api: ContractionExecutionApi> ContractionExecution<'session, Api> {
    pub(crate) fn prepare(
        resources: ContractionResources<'session, Api>,
        buffers: &[Box<[Complex64]>],
        bindings: &[usize],
        limits: WorkspaceLimits,
    ) -> Result<Self, SimulationError> {
        let mut execution = Self {
            resources: Some(resources),
            workspace: None,
            allocations: Vec::new(),
            host_scratch: None,
            output: None,
            report: ExecutionMemory::default(),
            usable: false,
        };
        if let Err(error) = execution.initialize(buffers, bindings, limits) {
            return combine_execution_and_cleanup(Err(error), execution.release());
        }
        execution.usable = true;
        Ok(execution)
    }

    pub(crate) fn memory(&self) -> &ExecutionMemory {
        &self.report
    }

    pub(crate) fn metadata(&mut self) -> Result<NativeMetadata, SimulationError> {
        self.ensure_usable()?;
        self.resources.as_mut().expect("live execution").export()
    }

    pub(crate) fn intermediate_modes(&mut self) -> Result<Vec<Vec<i32>>, SimulationError> {
        self.ensure_usable()?;
        self.resources
            .as_mut()
            .expect("live execution")
            .intermediate_modes()
    }

    /// Returns an owned host copy. After an execution failure, no further native
    /// inspection or execution is permitted; close synchronizes queued work.
    pub(crate) fn contract(&mut self) -> Result<Vec<Complex64>, SimulationError> {
        self.ensure_usable()?;
        self.usable = false;
        let resources = self.resources();
        resources.session.bind_device()?;
        let api = resources.session.api();
        let mut output =
            vec![Complex64Abi::default(); self.report.output_bytes / size_of::<Complex64Abi>()];
        api.contract(
            resources.session.handle(),
            resources.network(),
            self.workspace(),
            resources.session.stream(),
        )?;
        api.synchronize_stream(resources.session.stream())?;
        api.copy_from_device(self.output.expect("prepared output"), &mut output)?;
        let output: Vec<Complex64> = output.into_iter().map(Into::into).collect();
        if output
            .iter()
            .any(|value| !value.re.is_finite() || !value.im.is_finite())
        {
            return Err(unexpected("contraction returned nonfinite amplitudes"));
        }
        self.usable = true;
        Ok(output)
    }

    pub(crate) fn close(mut self) -> Result<(), SimulationError> {
        self.release()
    }

    fn ensure_usable(&self) -> Result<(), SimulationError> {
        if self.usable {
            Ok(())
        } else {
            Err(invalid("contraction execution is not usable after failure"))
        }
    }

    fn resources(&self) -> &ContractionResources<'session, Api> {
        self.resources.as_ref().expect("live execution")
    }

    fn workspace(&self) -> OpaqueHandle {
        self.workspace
            .expect("workspace created before preparation")
    }

    fn initialize(
        &mut self,
        buffers: &[Box<[Complex64]>],
        bindings: &[usize],
        limits: WorkspaceLimits,
    ) -> Result<(), SimulationError> {
        self.resources().ready()?;
        // Export validates the supported path/slicing subset even after search.
        let metadata = self.resources.as_mut().expect("live execution").export()?;
        if !metadata.slicing.is_empty() {
            return Err(invalid("numerical slicing is not qualified"));
        }
        let used = self.validate_buffers(buffers, bindings)?;
        let resources = self.resources();
        self.workspace = Some(
            resources
                .session
                .api()
                .create_workspace(resources.session.handle())?,
        );
        self.prepare_workspace(limits)?;
        let mut device_buffers = vec![None; buffers.len()];
        for (id, used) in used.into_iter().enumerate() {
            if used {
                let values: Vec<_> = buffers[id]
                    .iter()
                    .map(|value| Complex64Abi::new(value.re, value.im))
                    .collect();
                let allocation = self.allocate(bytes(values.len())?)?;
                self.resources()
                    .session
                    .api()
                    .copy_to_device(allocation, &values)?;
                device_buffers[id] = Some(allocation);
            }
        }
        self.output = Some(self.allocate(self.report.output_bytes)?);
        let resources = self.resources();
        let api = resources.session.api();
        for (&tensor_id, &buffer_id) in resources.tensor_ids.iter().zip(bindings) {
            api.bind_input(
                resources.session.handle(),
                resources.network(),
                tensor_id,
                device_buffers[buffer_id].expect("referenced buffer uploaded"),
            )?;
        }
        api.bind_output(
            resources.session.handle(),
            resources.network(),
            self.output.expect("allocated output"),
        )?;
        api.prepare_contraction(
            resources.session.handle(),
            resources.network(),
            self.workspace(),
        )?;
        Ok(())
    }

    fn validate_buffers(
        &mut self,
        buffers: &[Box<[Complex64]>],
        bindings: &[usize],
    ) -> Result<Vec<bool>, SimulationError> {
        let topology = &self.resources().topology;
        if bindings.len() != topology.inputs.len() {
            return Err(invalid("one buffer binding is required per input node"));
        }
        let mut used = vec![false; buffers.len()];
        for (tensor, &id) in topology.inputs.iter().zip(bindings) {
            let buffer = buffers
                .get(id)
                .ok_or_else(|| invalid("input buffer ID is out of range"))?;
            let elements = tensor.extents.iter().try_fold(1_usize, |n, &extent| {
                n.checked_mul(usize::try_from(extent).map_err(|_| overflow())?)
                    .ok_or_else(overflow)
            })?;
            if buffer.len() != elements {
                return Err(invalid(
                    "coefficient count does not match input tensor shape",
                ));
            }
            if buffer
                .iter()
                .any(|v| !v.re.is_finite() || !v.im.is_finite())
            {
                return Err(invalid("input coefficients must be finite"));
            }
            used[id] = true;
        }
        let elements = topology.output.iter().try_fold(1_usize, |n, mode| {
            n.checked_mul(usize::try_from(topology.dimensions[mode]).map_err(|_| overflow())?)
                .ok_or_else(overflow)
        })?;
        self.report.output_bytes = bytes(elements)?;
        for (buffer, &used) in buffers.iter().zip(&used) {
            if used {
                self.report.coefficient_bytes = self
                    .report
                    .coefficient_bytes
                    .checked_add(bytes(buffer.len())?)
                    .ok_or_else(overflow)?;
                self.report.unique_buffers += 1;
            }
        }
        self.report.owned_device_bytes = self
            .report
            .coefficient_bytes
            .checked_add(self.report.output_bytes)
            .ok_or_else(overflow)?;
        Ok(used)
    }

    fn prepare_workspace(&mut self, limits: WorkspaceLimits) -> Result<(), SimulationError> {
        let resources = self.resources();
        let api = resources.session.api();
        let handle = resources.session.handle();
        let workspace = self.workspace();
        api.compute_contraction_workspace(
            handle,
            resources.network(),
            resources.info(),
            workspace,
        )?;
        let size = |preference, space, kind| {
            let value = api.workspace_memory_size(handle, workspace, preference, space, kind)?;
            usize::try_from(value)
                .map_err(|_| unexpected("negative or unaddressable workspace size"))
        };
        let device_min = size(
            WorkspacePreference::Minimum,
            MemorySpace::Device,
            WorkspaceKind::Scratch,
        )?;
        let device_rec = size(
            WorkspacePreference::Recommended,
            MemorySpace::Device,
            WorkspaceKind::Scratch,
        )?;
        let host_min = size(
            WorkspacePreference::Minimum,
            MemorySpace::Host,
            WorkspaceKind::Scratch,
        )?;
        let host_rec = size(
            WorkspacePreference::Recommended,
            MemorySpace::Host,
            WorkspaceKind::Scratch,
        )?;
        let device_cache = size(
            WorkspacePreference::Recommended,
            MemorySpace::Device,
            WorkspaceKind::Cache,
        )?;
        let host_cache = size(
            WorkspacePreference::Recommended,
            MemorySpace::Host,
            WorkspaceKind::Cache,
        )?;
        if device_rec < device_min || host_rec < host_min {
            return Err(unexpected(
                "workspace recommendation is smaller than its minimum",
            ));
        }
        let device_bytes = device_min.max(256);
        check_limit(device_bytes, limits.device_scratch)?;
        check_limit(host_min, limits.host_scratch)?;
        self.report.device_scratch_minimum = device_min;
        self.report.device_scratch_recommended = device_rec;
        self.report.host_scratch_minimum = host_min;
        self.report.host_scratch_recommended = host_rec;
        self.report.device_cache_recommended = device_cache;
        self.report.host_cache_recommended = host_cache;
        self.report.owned_device_bytes = self
            .report
            .owned_device_bytes
            .checked_add(device_bytes)
            .ok_or_else(overflow)?;
        let device = self.allocate(device_bytes)?;
        self.report.device_scratch_allocated = device_bytes;
        if host_min > 0 {
            self.host_scratch = Some(HostScratch::new(host_min)?);
            self.report.host_scratch_allocated = host_min;
        }
        let api = self.resources().session.api();
        api.set_workspace_memory(
            handle,
            workspace,
            MemorySpace::Device,
            WorkspaceKind::Scratch,
            Some(device),
            native_bytes(device_bytes)?,
        )?;
        if let Some(host) = &self.host_scratch {
            api.set_workspace_memory(
                handle,
                workspace,
                MemorySpace::Host,
                WorkspaceKind::Scratch,
                Some(host.pointer),
                native_bytes(host_min)?,
            )?;
        }
        for space in [MemorySpace::Device, MemorySpace::Host] {
            api.set_workspace_memory(handle, workspace, space, WorkspaceKind::Cache, None, 0)?;
        }
        Ok(())
    }

    fn allocate(&mut self, bytes: usize) -> Result<OpaqueHandle, SimulationError> {
        let allocation = self.resources().session.api().allocate(bytes)?;
        self.allocations.push(allocation);
        Ok(allocation)
    }

    fn release(&mut self) -> Result<(), SimulationError> {
        let Some(mut resources) = self.resources.take() else {
            return Ok(());
        };
        self.usable = false;
        let mut result = resources.session.bind_device();
        result = combine_execution_and_cleanup(
            result,
            resources
                .session
                .api()
                .synchronize_stream(resources.session.stream()),
        );
        if let Some(workspace) = self.workspace.take() {
            result = combine_execution_and_cleanup(
                result,
                resources.session.api().destroy_workspace(workspace),
            );
        }
        result = combine_execution_and_cleanup(result, resources.release());
        while let Some(allocation) = self.allocations.pop() {
            result =
                combine_execution_and_cleanup(result, resources.session.api().free(allocation));
        }
        self.host_scratch = None;
        self.output = None;
        result
    }
}

impl<Api: ContractionExecutionApi> Drop for ContractionExecution<'_, Api> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

struct HostScratch {
    pointer: OpaqueHandle,
    layout: Layout,
}

impl HostScratch {
    fn new(bytes: usize) -> Result<Self, SimulationError> {
        let layout = Layout::from_size_align(bytes, 256).map_err(|_| overflow())?;
        // SAFETY: positive size, checked layout; this owner deallocates once.
        let pointer = NonNull::new(unsafe { std::alloc::alloc(layout) })
            .ok_or(SimulationError::HostScratchAllocationFailed { bytes })?;
        Ok(Self {
            pointer: pointer.cast(),
            layout,
        })
    }
}

impl Drop for HostScratch {
    fn drop(&mut self) {
        // SAFETY: same allocation/layout, no remaining native references.
        unsafe { std::alloc::dealloc(self.pointer.as_ptr().cast(), self.layout) };
    }
}

fn overflow() -> SimulationError {
    SimulationError::ResourceSizeOverflow {
        resource: "contraction buffer",
    }
}

fn bytes(elements: usize) -> Result<usize, SimulationError> {
    elements
        .checked_mul(size_of::<Complex64Abi>())
        .ok_or_else(overflow)
}

fn native_bytes(bytes: usize) -> Result<i64, SimulationError> {
    i64::try_from(bytes).map_err(|_| overflow())
}

fn check_limit(required: usize, maximum: Option<usize>) -> Result<(), SimulationError> {
    if let Some(maximum) = maximum
        && required > maximum
    {
        Err(SimulationError::WorkspaceLimitExceeded { required, maximum })
    } else {
        Ok(())
    }
}
