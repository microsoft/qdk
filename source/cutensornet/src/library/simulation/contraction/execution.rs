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
use qdk_simulators::execution::{
    ExecutableContraction, ExecutionLimits, InputMutability, PreparationFailure, ResourceReport,
};
use std::{
    alloc::Layout,
    collections::BTreeSet,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};
use tensornet::ContractionPlan;

#[cfg(test)]
#[path = "execution/qualification.rs"]
mod qualification;

/// Private native calls; buffers and workspace stay owned through synchronization
/// and network destruction. Null strides select the shared column-major layout.
pub(crate) trait ContractionExecutionApi:
    ContractionApi + SessionApi + MemoryWorkspaceApi
{
    fn allocate_host_scratch(&self, bytes: usize) -> Result<HostScratch, SimulationError>;
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CuTensorNetResourceReport {
    pub(crate) common: ResourceReport,
    pub(crate) device_cache_recommended: Option<usize>,
    pub(crate) host_cache_recommended: Option<usize>,
}

impl Default for CuTensorNetResourceReport {
    fn default() -> Self {
        Self {
            common: ResourceReport {
                resident_input_bytes: Some(0),
                resident_input_count: Some(0),
                device_scratch_allocated: Some(0),
                host_scratch_allocated: Some(0),
                owned_device_bytes: Some(0),
                ..ResourceReport::default()
            },
            device_cache_recommended: None,
            host_cache_recommended: None,
        }
    }
}

impl AsRef<ResourceReport> for CuTensorNetResourceReport {
    fn as_ref(&self) -> &ResourceReport {
        &self.common
    }
}

/// A contiguous column-major tensor (first axis fastest). Wire labels are not
/// part of a payload; ordered dimensions are, even when byte counts match.
#[derive(Clone, Copy)]
pub(crate) struct TensorInput<'a> {
    pub(crate) dimensions: &'a [usize],
    pub(crate) values: &'a [Complex64],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InputId {
    owner: usize,
    index: usize,
}

struct ResidentInput {
    dimensions: Vec<usize>,
    allocation: OpaqueHandle,
    bytes: usize,
    mutability: InputMutability,
}

/// Owns all pointers attached to the topology and exclusively borrows Session.
pub(crate) struct CuTensorNetExecutableContraction<'session, Api: ContractionExecutionApi> {
    resources: Option<ContractionResources<'session, Api>>,
    plan: ContractionPlan,
    workspace: Option<OpaqueHandle>,
    allocations: Vec<OpaqueHandle>,
    host_scratch: Option<HostScratch>,
    output: Option<OpaqueHandle>,
    report: CuTensorNetResourceReport,
    owner: usize,
    inputs: Vec<ResidentInput>,
    bindings: Vec<Option<InputId>>,
    usable: bool,
}

impl<'session, Api: ContractionExecutionApi> CuTensorNetExecutableContraction<'session, Api> {
    #[allow(
        clippy::result_large_err,
        reason = "preparation evidence is returned by value"
    )]
    pub(crate) fn prepare(
        resources: ContractionResources<'session, Api>,
        plan: ContractionPlan,
        limits: ExecutionLimits,
    ) -> Result<Self, PreparationFailure<SimulationError, CuTensorNetResourceReport>> {
        let bindings = vec![None; resources.topology.inputs.len()];
        let mut execution = Self {
            resources: Some(resources),
            plan,
            workspace: None,
            allocations: Vec::new(),
            host_scratch: None,
            output: None,
            report: CuTensorNetResourceReport::default(),
            owner: 0,
            inputs: Vec::new(),
            bindings,
            usable: false,
        };
        if let Err(error) = execution.initialize(limits) {
            let cleanup = execution.release().err();
            return Err(PreparationFailure {
                partial: execution.report.clone(),
                error,
                cleanup,
            });
        }
        execution.usable = true;
        Ok(execution)
    }

    pub(crate) fn plan(&self) -> &ContractionPlan {
        &self.plan
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

    fn ensure_usable(&self) -> Result<(), SimulationError> {
        if self.usable {
            Ok(())
        } else {
            Err(SimulationError::UnusableContraction)
        }
    }

    fn native(&self) -> &ContractionResources<'session, Api> {
        self.resources.as_ref().expect("live execution")
    }

    fn workspace(&self) -> OpaqueHandle {
        self.workspace
            .expect("workspace created before preparation")
    }

    fn initialize(&mut self, limits: ExecutionLimits) -> Result<(), SimulationError> {
        static NEXT_OWNER: AtomicUsize = AtomicUsize::new(1);
        self.owner = NEXT_OWNER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| overflow())?;
        self.native().ready()?;
        let metadata = self.resources.as_mut().expect("live execution").export()?;
        if !metadata.slicing.is_empty() {
            return Err(SimulationError::UnsupportedContraction {
                reason: "numerical slicing is not qualified",
            });
        }
        let topology = &self.native().topology;
        let elements = topology.output.iter().try_fold(1_usize, |n, mode| {
            n.checked_mul(usize::try_from(topology.dimensions[mode]).map_err(|_| overflow())?)
                .ok_or_else(overflow)
        })?;
        let output_bytes = bytes(elements)?;
        self.report.common.output_bytes = Some(output_bytes);
        self.workspace = Some(
            self.native()
                .session
                .api()
                .create_workspace(self.native().session.handle())?,
        );
        self.prepare_workspace(limits)?;
        self.output = Some(self.allocate(output_bytes)?);
        let resources = self.native();
        let api = resources.session.api();
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

    fn input(&self, id: InputId) -> Result<&ResidentInput, SimulationError> {
        if id.owner != self.owner {
            return Err(invalid("input identity belongs to a different executable"));
        }
        self.inputs
            .get(id.index)
            .ok_or_else(|| invalid("invalid input identity"))
    }

    fn prepare_workspace(&mut self, limits: ExecutionLimits) -> Result<(), SimulationError> {
        let resources = self.native();
        let handle = resources.session.handle();
        let workspace = self.workspace();
        resources.session.api().compute_contraction_workspace(
            handle,
            resources.network(),
            resources.info(),
            workspace,
        )?;
        self.report.common.device_scratch_minimum = Some(self.workspace_size(
            WorkspacePreference::Minimum,
            MemorySpace::Device,
            WorkspaceKind::Scratch,
        )?);
        self.report.common.device_scratch_recommended = Some(self.workspace_size(
            WorkspacePreference::Recommended,
            MemorySpace::Device,
            WorkspaceKind::Scratch,
        )?);
        self.report.common.host_scratch_minimum = Some(self.workspace_size(
            WorkspacePreference::Minimum,
            MemorySpace::Host,
            WorkspaceKind::Scratch,
        )?);
        self.report.common.host_scratch_recommended = Some(self.workspace_size(
            WorkspacePreference::Recommended,
            MemorySpace::Host,
            WorkspaceKind::Scratch,
        )?);
        self.report.device_cache_recommended = Some(self.workspace_size(
            WorkspacePreference::Recommended,
            MemorySpace::Device,
            WorkspaceKind::Cache,
        )?);
        self.report.host_cache_recommended = Some(self.workspace_size(
            WorkspacePreference::Recommended,
            MemorySpace::Host,
            WorkspaceKind::Cache,
        )?);
        let report = &self.report.common;
        let device_min = report.device_scratch_minimum.expect("observed");
        let host_min = report.host_scratch_minimum.expect("observed");
        if report.device_scratch_recommended.expect("observed") < device_min
            || report.host_scratch_recommended.expect("observed") < host_min
        {
            return Err(unexpected(
                "workspace recommendation is smaller than its minimum",
            ));
        }
        let device_bytes = device_min.max(256);
        check_limit(device_bytes, limits.device_scratch_bytes)?;
        check_limit(host_min, limits.host_scratch_bytes)?;
        let native_device_bytes = native_bytes(device_bytes)?;
        let native_host_bytes = native_bytes(host_min)?;
        let device = self.allocate(device_bytes)?;
        self.report.common.device_scratch_allocated = Some(device_bytes);
        if host_min > 0 {
            self.host_scratch = Some(
                self.native()
                    .session
                    .api()
                    .allocate_host_scratch(host_min)?,
            );
            self.report.common.host_scratch_allocated = Some(host_min);
        }
        let api = self.native().session.api();
        api.set_workspace_memory(
            handle,
            workspace,
            MemorySpace::Device,
            WorkspaceKind::Scratch,
            Some(device),
            native_device_bytes,
        )?;
        if let Some(host) = &self.host_scratch {
            api.set_workspace_memory(
                handle,
                workspace,
                MemorySpace::Host,
                WorkspaceKind::Scratch,
                Some(host.pointer),
                native_host_bytes,
            )?;
        }
        for space in [MemorySpace::Device, MemorySpace::Host] {
            api.set_workspace_memory(handle, workspace, space, WorkspaceKind::Cache, None, 0)?;
        }
        Ok(())
    }

    fn workspace_size(
        &self,
        preference: WorkspacePreference,
        space: MemorySpace,
        kind: WorkspaceKind,
    ) -> Result<usize, SimulationError> {
        let value = self.native().session.api().workspace_memory_size(
            self.native().session.handle(),
            self.workspace(),
            preference,
            space,
            kind,
        )?;
        usize::try_from(value).map_err(|_| unexpected("negative or unaddressable workspace size"))
    }

    fn allocate(&mut self, bytes: usize) -> Result<OpaqueHandle, SimulationError> {
        let total = self
            .report
            .common
            .owned_device_bytes
            .expect("known allocations")
            .checked_add(bytes)
            .ok_or_else(overflow)?;
        let allocation = self.native().session.api().allocate(bytes)?;
        self.allocations.push(allocation);
        self.report.common.owned_device_bytes = Some(total);
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
        self.inputs.clear();
        self.host_scratch = None;
        self.output = None;
        result
    }
}

impl<Api: ContractionExecutionApi> ExecutableContraction
    for CuTensorNetExecutableContraction<'_, Api>
{
    type Input<'a> = TensorInput<'a>;
    type InputId = InputId;
    type Report = CuTensorNetResourceReport;
    type Output = Vec<Complex64>;
    type Error = SimulationError;

    fn register_input(
        &mut self,
        input: TensorInput<'_>,
        mutability: InputMutability,
    ) -> Result<InputId, SimulationError> {
        self.ensure_usable()?;
        self.usable = false;
        let values = validate_input(input)?;
        let bytes = bytes(values.len())?;
        let resident_bytes = self
            .report
            .common
            .resident_input_bytes
            .expect("known inputs")
            .checked_add(bytes)
            .ok_or_else(overflow)?;
        let resident_count = self.inputs.len().checked_add(1).ok_or_else(overflow)?;
        let dimensions = input.dimensions.to_vec();
        self.native().session.bind_device()?;
        let allocation = self.allocate(bytes)?;
        let id = InputId {
            owner: self.owner,
            index: self.inputs.len(),
        };
        self.inputs.push(ResidentInput {
            dimensions,
            allocation,
            bytes,
            mutability,
        });
        self.report.common.resident_input_bytes = Some(resident_bytes);
        self.report.common.resident_input_count = Some(resident_count);
        self.native()
            .session
            .api()
            .copy_to_device(allocation, &values)?;
        self.usable = true;
        Ok(id)
    }

    fn replace_input(
        &mut self,
        id: InputId,
        input: TensorInput<'_>,
    ) -> Result<(), SimulationError> {
        self.ensure_usable()?;
        self.usable = false;
        let resident = self.input(id)?;
        if resident.mutability != InputMutability::Mutable {
            return Err(invalid("immutable input cannot be replaced"));
        }
        if resident.dimensions != input.dimensions {
            return Err(invalid("replacement must preserve ordered dimensions"));
        }
        let values = validate_input(input)?;
        let allocation = resident.allocation;
        self.native().session.bind_device()?;
        // Every successful execute synchronized; failed executions cannot reach here.
        self.native()
            .session
            .api()
            .copy_to_device(allocation, &values)?;
        self.usable = true;
        Ok(())
    }

    fn execute(&mut self, inputs: &[InputId]) -> Result<Vec<Complex64>, SimulationError> {
        self.ensure_usable()?;
        self.usable = false;
        if inputs.len() != self.bindings.len() {
            return Err(invalid("one input identity is required per input slot"));
        }
        let mut selected = BTreeSet::new();
        let mut selected_bytes = 0_usize;
        for (tensor, &id) in self.native().topology.inputs.iter().zip(inputs) {
            let input = self.input(id)?;
            if tensor.extents.len() != input.dimensions.len()
                || tensor
                    .extents
                    .iter()
                    .zip(&input.dimensions)
                    .any(|(&extent, &dim)| usize::try_from(extent) != Ok(dim))
            {
                return Err(invalid("input ordered dimensions do not match the slot"));
            }
            if selected.insert(id.index) {
                selected_bytes = selected_bytes
                    .checked_add(input.bytes)
                    .ok_or_else(overflow)?;
            }
        }
        self.report.common.selected_input_bytes = Some(selected_bytes);
        self.report.common.selected_input_count = Some(selected.len());
        self.native().session.bind_device()?;
        for (slot, &id) in inputs.iter().enumerate() {
            if self.bindings[slot] != Some(id) {
                let resources = self.native();
                resources.session.api().bind_input(
                    resources.session.handle(),
                    resources.network(),
                    resources.tensor_ids[slot],
                    self.input(id)?.allocation,
                )?;
                self.bindings[slot] = Some(id);
            }
        }
        let resources = self.native();
        let api = resources.session.api();
        let mut output = vec![
            Complex64Abi::default();
            self.report.common.output_bytes.expect("prepared output")
                / size_of::<Complex64Abi>()
        ];
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

    fn resources(&self) -> &CuTensorNetResourceReport {
        &self.report
    }

    fn close(mut self) -> Result<(), SimulationError> {
        self.release()
    }
}

impl<Api: ContractionExecutionApi> Drop for CuTensorNetExecutableContraction<'_, Api> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

pub(crate) struct HostScratch {
    pointer: OpaqueHandle,
    layout: Layout,
}

impl HostScratch {
    pub(crate) fn new(bytes: usize) -> Result<Self, SimulationError> {
        if bytes == 0 {
            return Err(invalid("host scratch allocation must be positive"));
        }
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

fn validate_input(input: TensorInput<'_>) -> Result<Vec<Complex64Abi>, SimulationError> {
    let elements = input.dimensions.iter().try_fold(1_usize, |n, &dimension| {
        if dimension == 0 {
            return Err(invalid("input dimensions must be positive"));
        }
        n.checked_mul(dimension).ok_or_else(overflow)
    })?;
    if elements != input.values.len() {
        return Err(invalid(
            "coefficient count does not match input tensor shape",
        ));
    }
    bytes(elements)?;
    if input
        .values
        .iter()
        .any(|v| !v.re.is_finite() || !v.im.is_finite())
    {
        return Err(invalid("input coefficients must be finite"));
    }
    Ok(input
        .values
        .iter()
        .map(|v| Complex64Abi::new(v.re, v.im))
        .collect())
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
