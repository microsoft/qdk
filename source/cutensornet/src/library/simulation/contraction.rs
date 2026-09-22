//! Private cuTensorNet topology and optimizer metadata.
//!
//! A mode is a labeled tensor axis; its extent is its dimension. A binary path
//! selects two positions in the current operand list, removes them and appends
//! their intermediate result, preserving the other operands' order.
//! These positions are not the native IDs returned when appending input tensors.
//! This is the pinned SDK's contract; native qualification checks its behavior.
//!
//! Slicing partitions a mode into smaller extents and contracts each partition
//! separately. Only full coverage of internal modes at unit extent is supported
//! here: the slice count is the product of their original dimensions, or one
//! without slicing. Intermediate modes describe each path result, including the
//! final output. Numerical resources live in `execution`; the metadata is not
//! a portable contraction plan.

use super::{
    OpaqueHandle, SimulationError,
    error::combine_execution_and_cleanup,
    resources::{SessionApi, SessionResources},
};
use std::collections::{BTreeMap, BTreeSet};
use tensornet::{ContractionQuery, Indices};

#[path = "contraction/adapter.rs"]
pub(crate) mod adapter;
#[path = "contraction/execution.rs"]
pub(crate) mod execution;
#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
#[path = "contraction/qualification.rs"]
mod qualification;
#[cfg(test)]
#[path = "contraction/tests.rs"]
mod tests;

/// Tensor axes in original order, with checked native-width labels and extents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeTensor {
    pub(crate) modes: Vec<i32>,
    pub(crate) extents: Vec<i64>,
}

impl NativeTensor {
    fn from_indices(indices: &Indices) -> Result<Self, SimulationError> {
        native_count(indices.as_slice().len(), "tensor rank")?;
        let modes = indices
            .as_slice()
            .iter()
            .map(|axis| {
                i32::try_from(axis.id()).map_err(|_| invalid("mode label does not fit i32"))
            })
            .collect::<Result<_, _>>()?;
        let extents = indices
            .as_slice()
            .iter()
            .map(|axis| {
                i64::try_from(axis.dim()).map_err(|_| invalid("mode extent does not fit i64"))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { modes, extents })
    }
}

/// A mode label and its extent within each slice, not the number of slices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SlicedMode {
    pub(crate) mode: i32,
    pub(crate) extent: i64,
}

/// Owned copies of native metadata, not a portable contraction plan.
///
/// Remains usable after native resources close. Import/export validate complete
/// binary paths and full internal unit-extent slicing against the topology.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeMetadata {
    /// Positions in the shrinking operand list; remove both and append the result.
    pub(crate) path: Vec<[i32; 2]>,
    pub(crate) slicing: Vec<SlicedMode>,
    pub(crate) num_slices: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OptimizerSetting {
    HyperSamples,
    Threads,
    Seed,
    ReconfigurationIterations,
    DisableRankSimplification,
    DisableSlicing,
}

/// Explicit search controls; unlisted options retain the pinned SDK defaults.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OptimizerSettings {
    /// Optimizer workspace budget in bytes; neither an allocation nor total GPU memory.
    pub(crate) workspace_constraint: u64,
    pub(crate) hyper_samples: i32,
    pub(crate) threads: i32,
    pub(crate) seed: i32,
    pub(crate) reconfiguration_iterations: i32,
    pub(crate) disable_rank_simplification: bool,
    pub(crate) disable_slicing: bool,
}

impl OptimizerSettings {
    fn attributes(self) -> Result<[(OptimizerSetting, i32); 6], SimulationError> {
        if self.workspace_constraint == 0
            || self.hyper_samples < 0
            || self.threads <= 0
            || self.seed < 0
            || self.reconfiguration_iterations < 0
        {
            return Err(invalid("invalid optimizer settings"));
        }
        Ok([
            (OptimizerSetting::HyperSamples, self.hyper_samples),
            (OptimizerSetting::Threads, self.threads),
            (OptimizerSetting::Seed, self.seed),
            (
                OptimizerSetting::ReconfigurationIterations,
                self.reconfiguration_iterations,
            ),
            (
                OptimizerSetting::DisableRankSimplification,
                i32::from(self.disable_rank_simplification),
            ),
            (
                OptimizerSetting::DisableSlicing,
                i32::from(self.disable_slicing),
            ),
        ])
    }
}

/// Vendor-reported estimates, without an inferred FLOP or tensor-size convention.
#[derive(Clone, Copy, Debug)]
pub(crate) enum OptimizerEstimate {
    FlopCount,
    LargestTensor,
}

/// Injected cuTensorNet topology/metadata operations, not a shared backend API.
///
/// Callers keep handles live, associated with the same context and on its device.
/// Topology is complete before optimizer-info creation; reads require populated
/// info. Native handles and buffer sizes are private caller obligations, not
/// properties enforced by the raw handle type. Implementations copy input
/// metadata and retain no Rust buffer pointers after these calls.
pub(crate) trait ContractionApi {
    fn create_network(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_network(&self, network: OpaqueHandle) -> Result<(), SimulationError>;
    /// Copies ordered axes and returns an opaque native ID, not a path position.
    fn append_tensor(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        tensor: &NativeTensor,
    ) -> Result<i64, SimulationError>;
    fn set_output(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        modes: &[i32],
    ) -> Result<(), SimulationError>;
    fn set_compute_f64(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
    ) -> Result<(), SimulationError>;
    fn create_optimizer_config(
        &self,
        handle: OpaqueHandle,
    ) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_optimizer_config(&self, config: OpaqueHandle) -> Result<(), SimulationError>;
    fn configure_optimizer(
        &self,
        handle: OpaqueHandle,
        config: OpaqueHandle,
        setting: OptimizerSetting,
        value: i32,
    ) -> Result<(), SimulationError>;
    fn create_optimizer_info(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
    ) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_optimizer_info(&self, info: OpaqueHandle) -> Result<(), SimulationError>;
    /// Searches using the supplied byte budget and populates optimizer info.
    fn optimize(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        config: OpaqueHandle,
        workspace_constraint: u64,
        info: OpaqueHandle,
    ) -> Result<(), SimulationError>;
    /// Copies a complete positional path into info without searching or attaching.
    fn set_path(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        path: &[[i32; 2]],
    ) -> Result<(), SimulationError>;
    /// Copies slicing into info without searching; an empty slice disables slicing.
    fn set_slicing(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        slicing: &[SlicedMode],
    ) -> Result<(), SimulationError>;
    /// Attaches populated info to its matching network without optimizer search.
    fn attach_optimizer_info(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
    ) -> Result<(), SimulationError>;
    /// Fills caller storage sized to input count minus one; returns the native count.
    ///
    /// The caller checks the returned count before accepting the owned copy.
    fn read_path(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        path: &mut [[i32; 2]],
    ) -> Result<i32, SimulationError>;
    fn num_sliced_modes(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
    ) -> Result<i32, SimulationError>;
    /// Fills caller storage sized by `num_sliced_modes`; returns the native count.
    ///
    /// Info must not change between sizing and reading. The caller checks the
    /// returned count before accepting the owned copy.
    fn read_slicing(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        slicing: &mut [SlicedMode],
    ) -> Result<u32, SimulationError>;
    fn num_slices(&self, handle: OpaqueHandle, info: OpaqueHandle) -> Result<i64, SimulationError>;
    /// Fills one rank per path step, including the final output.
    ///
    /// Caller storage must contain input count minus one elements.
    fn intermediate_mode_counts(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        counts: &mut [i32],
    ) -> Result<(), SimulationError>;
    /// Fills flattened native mode labels in path order, including the output.
    ///
    /// Storage must match the sum of ranks from `intermediate_mode_counts` on
    /// unchanged info. No axis ordering within an intermediate is assumed.
    fn intermediate_modes(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        modes: &mut [i32],
    ) -> Result<(), SimulationError>;
    fn estimate(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        estimate: OptimizerEstimate,
    ) -> Result<f64, SimulationError>;
    fn create_slice_group_from_id_range(
        &self,
        handle: OpaqueHandle,
        start: i64,
        stop: i64,
        increment: i64,
    ) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_slice_group(&self, slice_group: OpaqueHandle) -> Result<(), SimulationError>;
}

/// Native-width copy of a validated query, preserving input and output-axis order.
///
/// Requires at least two inputs; the shared query owns connectivity/dimension
/// validation. This copy checks native ranges and the supported metadata subset,
/// not numerical buffers or contraction costs.
struct Topology {
    inputs: Vec<NativeTensor>,
    output: Vec<i32>,
    dimensions: BTreeMap<i32, i64>,
}

impl Topology {
    fn new(query: &ContractionQuery<'_>) -> Result<Self, SimulationError> {
        let count = query.network().nodes().len();
        native_count(count, "input tensor count")?;
        if count < 2 {
            return Err(invalid(
                "binary path metadata requires at least two input tensors",
            ));
        }
        let inputs = query
            .network()
            .nodes()
            .iter()
            .map(NativeTensor::from_indices)
            .collect::<Result<Vec<_>, _>>()?;
        let output = NativeTensor::from_indices(query.keep())?.modes;
        let dimensions = inputs
            .iter()
            .flat_map(|tensor| {
                tensor
                    .modes
                    .iter()
                    .copied()
                    .zip(tensor.extents.iter().copied())
            })
            .collect();
        Ok(Self {
            inputs,
            output,
            dimensions,
        })
    }

    fn validate(&self, metadata: &NativeMetadata) -> Result<(), SimulationError> {
        validate_path(self.inputs.len(), &metadata.path)?;
        native_count(metadata.slicing.len(), "sliced mode count")?;
        let mut seen = BTreeSet::new();
        let mut slices = 1_i64;
        for slice in &metadata.slicing {
            let Some(&dimension) = self.dimensions.get(&slice.mode) else {
                return Err(invalid("sliced mode is not in the network"));
            };
            if !seen.insert(slice.mode) {
                return Err(invalid("sliced modes must be unique"));
            }
            if self.output.contains(&slice.mode) {
                return Err(invalid("output slicing is not qualified"));
            }
            if slice.extent != 1 {
                return Err(invalid("only internal unit-extent slicing is qualified"));
            }
            slices =
                slices
                    .checked_mul(dimension)
                    .ok_or(SimulationError::ResourceSizeOverflow {
                        resource: "slice count",
                    })?;
        }
        if metadata.num_slices != slices {
            return Err(invalid(
                "slice count does not match full internal slice coverage",
            ));
        }
        Ok(())
    }
}

/// Owns a network and its optimizer objects under an exclusive session borrow.
///
/// The parent cannot close or be reused while this owner lives. Optimizer info
/// is created after topology; config is created only for explicit search.
/// Successful optimize/import enables reads; failed native mutation invalidates
/// readiness. Exported vectors borrow no native storage. Drop is fallback cleanup;
/// explicit close reports failures and attempts every acquired object's release.
pub(crate) struct ContractionResources<'session, Api: SessionApi + ContractionApi> {
    session: &'session mut SessionResources<Api>,
    topology: Topology,
    tensor_ids: Vec<i64>,
    network: Option<OpaqueHandle>,
    optimizer_config: Option<OpaqueHandle>,
    optimizer_info: Option<OpaqueHandle>,
    metadata_ready: bool,
}

impl<'session, Api: SessionApi + ContractionApi> ContractionResources<'session, Api> {
    pub(crate) fn new(
        session: &'session mut SessionResources<Api>,
        query: &ContractionQuery<'_>,
    ) -> Result<Self, SimulationError> {
        let topology = Topology::new(query)?;
        session.bind_device()?;
        let network = session.api().create_network(session.handle())?;
        let mut resources = Self {
            session,
            topology,
            tensor_ids: Vec::new(),
            network: Some(network),
            optimizer_config: None,
            optimizer_info: None,
            metadata_ready: false,
        };
        if let Err(error) = resources.initialize() {
            return combine_execution_and_cleanup(Err(error), resources.release());
        }
        Ok(resources)
    }

    fn initialize(&mut self) -> Result<(), SimulationError> {
        for tensor in &self.topology.inputs {
            let id =
                self.session
                    .api()
                    .append_tensor(self.session.handle(), self.network(), tensor)?;
            if self.tensor_ids.contains(&id) {
                return Err(unexpected("native tensor IDs are not unique"));
            }
            self.tensor_ids.push(id);
        }
        self.session.api().set_output(
            self.session.handle(),
            self.network(),
            &self.topology.output,
        )?;
        self.session
            .api()
            .set_compute_f64(self.session.handle(), self.network())?;
        self.optimizer_info = Some(
            self.session
                .api()
                .create_optimizer_info(self.session.handle(), self.network())?,
        );
        Ok(())
    }

    /// Native IDs in input order, independent of positional path operands.
    pub(crate) fn tensor_ids(&self) -> &[i64] {
        &self.tensor_ids
    }

    /// Performs native search with explicit settings; allocates no device workspace.
    pub(crate) fn optimize(&mut self, settings: OptimizerSettings) -> Result<(), SimulationError> {
        let attributes = settings.attributes()?;
        self.session.bind_device()?;
        self.metadata_ready = false;
        if self.optimizer_config.is_none() {
            self.optimizer_config = Some(
                self.session
                    .api()
                    .create_optimizer_config(self.session.handle())?,
            );
        }
        let config = self.optimizer_config.expect("optimizer config was created");
        for (setting, value) in attributes {
            self.session.api().configure_optimizer(
                self.session.handle(),
                config,
                setting,
                value,
            )?;
        }
        self.session.api().optimize(
            self.session.handle(),
            self.network(),
            config,
            settings.workspace_constraint,
            self.info(),
        )?;
        self.metadata_ready = true;
        Ok(())
    }

    /// Validates, copies and attaches supplied metadata; never invokes search.
    pub(crate) fn import(&mut self, metadata: &NativeMetadata) -> Result<(), SimulationError> {
        self.topology.validate(metadata)?;
        self.session.bind_device()?;
        self.metadata_ready = false;
        let api = self.session.api();
        api.set_path(self.session.handle(), self.info(), &metadata.path)?;
        api.set_slicing(self.session.handle(), self.info(), &metadata.slicing)?;
        api.attach_optimizer_info(self.session.handle(), self.network(), self.info())?;
        self.metadata_ready = true;
        Ok(())
    }

    /// Reads and validates owned metadata after a successful optimize or import.
    pub(crate) fn export(&mut self) -> Result<NativeMetadata, SimulationError> {
        self.ready()?;
        let api = self.session.api();
        let handle = self.session.handle();
        let info = self.info();
        let mut path = vec![[-1, -1]; self.topology.inputs.len() - 1];
        let count = api.read_path(handle, info, &mut path)?;
        if usize::try_from(count).ok() != Some(path.len()) {
            return Err(unexpected("native path length does not match the network"));
        }
        let count = api.num_sliced_modes(handle, info)?;
        let count = usize::try_from(count).map_err(|_| unexpected("negative sliced mode count"))?;
        if count > self.topology.dimensions.len() {
            return Err(unexpected(
                "native sliced mode count exceeds the network mode count",
            ));
        }
        let mut slicing = vec![
            SlicedMode {
                mode: -1,
                extent: 0
            };
            count
        ];
        let returned = api.read_slicing(handle, info, &mut slicing)?;
        if usize::try_from(returned).ok() != Some(count) {
            return Err(unexpected(
                "native sliced mode count changed during retrieval",
            ));
        }
        let metadata = NativeMetadata {
            path,
            slicing,
            num_slices: api.num_slices(handle, info)?,
        };
        self.topology
            .validate(&metadata)
            .map_err(|error| unexpected(error.to_string()))?;
        Ok(metadata)
    }

    /// Native structural observations. No search or locally synthesized substitute.
    pub(crate) fn intermediate_modes(&mut self) -> Result<Vec<Vec<i32>>, SimulationError> {
        self.ready()?;
        let api = self.session.api();
        let handle = self.session.handle();
        let info = self.info();
        let mut counts = vec![-1; self.topology.inputs.len() - 1];
        api.intermediate_mode_counts(handle, info, &mut counts)?;
        let counts = counts
            .into_iter()
            .map(|count| {
                usize::try_from(count).map_err(|_| unexpected("negative intermediate rank"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let total = counts.iter().try_fold(0_usize, |total, &count| {
            if count > self.topology.dimensions.len() {
                return Err(unexpected(
                    "intermediate rank exceeds the network mode count",
                ));
            }
            total
                .checked_add(count)
                .ok_or(SimulationError::ResourceSizeOverflow {
                    resource: "intermediate modes",
                })
        })?;
        let mut modes = vec![-1; total];
        api.intermediate_modes(handle, info, &mut modes)?;
        let mut offset = 0;
        let mut intermediates = Vec::with_capacity(counts.len());
        for count in counts {
            let modes = &modes[offset..offset + count];
            let unique: BTreeSet<_> = modes.iter().collect();
            if unique.len() != modes.len()
                || modes
                    .iter()
                    .any(|mode| !self.topology.dimensions.contains_key(mode))
            {
                return Err(unexpected(
                    "native intermediate contains duplicate or unknown modes",
                ));
            }
            intermediates.push(modes.to_vec());
            offset += count;
        }
        Ok(intermediates)
    }

    pub(crate) fn estimate(&mut self, estimate: OptimizerEstimate) -> Result<f64, SimulationError> {
        self.ready()?;
        let value = self
            .session
            .api()
            .estimate(self.session.handle(), self.info(), estimate)?;
        if !value.is_finite() || value < 0.0 {
            return Err(unexpected(
                "native optimizer estimate is negative or nonfinite",
            ));
        }
        Ok(value)
    }

    /// Consumes the owner and attempts all releases, preserving cleanup errors.
    ///
    /// Handles are taken even if native destruction fails; Drop does not retry.
    pub(crate) fn close(mut self) -> Result<(), SimulationError> {
        self.release()
    }

    fn ready(&self) -> Result<(), SimulationError> {
        if !self.metadata_ready {
            return Err(invalid("optimizer metadata has not been populated"));
        }
        self.session.bind_device()
    }

    fn network(&self) -> OpaqueHandle {
        self.network
            .expect("live contraction resources own their network")
    }

    fn info(&self) -> OpaqueHandle {
        self.optimizer_info
            .expect("initialized topology owns optimizer info")
    }

    fn release(&mut self) -> Result<(), SimulationError> {
        if self.network.is_none() {
            return Ok(());
        }
        let mut result = self.session.bind_device();
        let api = self.session.api();
        if let Some(info) = self.optimizer_info.take() {
            result = combine_execution_and_cleanup(result, api.destroy_optimizer_info(info));
        }
        if let Some(config) = self.optimizer_config.take() {
            result = combine_execution_and_cleanup(result, api.destroy_optimizer_config(config));
        }
        if let Some(network) = self.network.take() {
            result = combine_execution_and_cleanup(result, api.destroy_network(network));
        }
        result
    }
}

impl<Api: SessionApi + ContractionApi> Drop for ContractionResources<'_, Api> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

/// Native selection of slice IDs under an exclusive parent-session borrow.
///
/// Selects work, not sliced modes or their extents. Explicit close reports
/// destruction errors; Drop attempts release only if it has not been attempted.
pub(crate) struct SliceGroup<'session, Api: SessionApi + ContractionApi> {
    session: &'session mut SessionResources<Api>,
    slice_group: Option<OpaqueHandle>,
}

impl<'session, Api: SessionApi + ContractionApi> SliceGroup<'session, Api> {
    pub(crate) fn from_id_range(
        session: &'session mut SessionResources<Api>,
        start: i64,
        stop: i64,
        increment: i64,
    ) -> Result<Self, SimulationError> {
        if increment == 0 {
            return Err(invalid("slice identifier increment must be non-zero"));
        }
        session.bind_device()?;
        let slice_group = session.api().create_slice_group_from_id_range(
            session.handle(),
            start,
            stop,
            increment,
        )?;
        Ok(Self {
            session,
            slice_group: Some(slice_group),
        })
    }

    pub(crate) fn as_handle(&self) -> OpaqueHandle {
        self.slice_group
            .expect("a live slice group owns its native object")
    }

    pub(crate) fn close(mut self) -> Result<(), SimulationError> {
        self.release()
    }

    fn release(&mut self) -> Result<(), SimulationError> {
        self.slice_group.take().map_or(Ok(()), |group| {
            combine_execution_and_cleanup(
                self.session.bind_device(),
                self.session.api().destroy_slice_group(group),
            )
        })
    }
}

impl<Api: SessionApi + ContractionApi> Drop for SliceGroup<'_, Api> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

fn native_count(count: usize, resource: &'static str) -> Result<i32, SimulationError> {
    i32::try_from(count).map_err(|_| SimulationError::ResourceSizeOverflow { resource })
}

fn validate_path(inputs: usize, path: &[[i32; 2]]) -> Result<(), SimulationError> {
    native_count(inputs, "input tensor count")?;
    if inputs < 2 || path.len() != inputs - 1 {
        return Err(invalid(
            "a binary path must contain input count minus one pairs",
        ));
    }
    for (step, &[first, second]) in path.iter().enumerate() {
        let remaining = inputs - step;
        if first == second
            || usize::try_from(first).map_or(true, |value| value >= remaining)
            || usize::try_from(second).map_or(true, |value| value >= remaining)
        {
            return Err(invalid("path operands must be distinct live positions"));
        }
    }
    Ok(())
}

fn invalid(reason: &'static str) -> SimulationError {
    SimulationError::InvalidContractionConfiguration { reason }
}

fn unexpected(reason: impl Into<String>) -> SimulationError {
    SimulationError::InvalidNativeResult {
        reason: reason.into(),
    }
}
