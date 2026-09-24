use super::*;
use crate::simulation::{
    contraction::execution::{
        ContractionExecutionApi, CuTensorNetExecutableContraction, HostScratch, InputId,
        TensorInput,
    },
    ffi::Complex64Abi,
    memory_workspace::{MemorySpace, MemoryWorkspaceApi, WorkspaceKind, WorkspacePreference},
};
use num_complex::Complex64;
use qdk_simulators::execution::{
    ContractionContext, ExecutableContraction, ExecutionLimits, InputMutability,
};
use std::collections::BTreeMap;
use tensornet::{ContractionPlan, ContractionStep, Operand};

#[derive(Default)]
pub(super) struct NumericalState {
    pub(super) allocations: BTreeMap<usize, AllocationState>,
    pub(super) workspaces: BTreeMap<usize, WorkspaceState>,
    pub(super) pending: BTreeMap<usize, BTreeSet<usize>>,
}

pub(super) struct AllocationState {
    bytes: usize,
    input: Option<Vec<Complex64Abi>>,
    output: Option<(usize, Vec<Complex64Abi>)>,
}

#[derive(Default)]
pub(super) struct WorkspaceState {
    pub(super) network: Option<usize>,
    pub(super) bindings: Vec<(MemorySpace, WorkspaceKind, Option<usize>, i64)>,
}

#[derive(Default)]
pub(super) struct NetworkNumericalState {
    pub(super) inputs: BTreeMap<i64, usize>,
    pub(super) output: Option<usize>,
    prepared: bool,
    pub(super) stream: Option<usize>,
}

#[derive(Default)]
pub(super) struct NumericalHistory {
    uploads: BTreeMap<usize, Vec<Complex64Abi>>,
    inputs: Vec<(i64, usize)>,
    workspace: Vec<(MemorySpace, WorkspaceKind, Option<usize>, i64)>,
}

pub(super) struct NumericalSettings {
    device_minimum: i64,
    host_minimum: i64,
    nonfinite_output: bool,
    input_dependent: bool,
}

impl Default for NumericalSettings {
    fn default() -> Self {
        Self {
            device_minimum: 512,
            host_minimum: 128,
            nonfinite_output: false,
            input_dependent: false,
        }
    }
}

fn api(
    failures: Vec<(&'static str, usize)>,
    numerical: NumericalSettings,
) -> Arc<TestDoubleContractionApi> {
    Arc::new(TestDoubleContractionApi {
        state: Mutex::new(State::default()),
        failures,
        corruption: Corruption::None,
        numerical,
        observations: NativeObservations {
            use_query_output: true,
            ..NativeObservations::default()
        },
    })
}

impl MemoryWorkspaceApi for TestDoubleContractionApi {
    fn memory_info(&self) -> Result<(usize, usize), SimulationError> {
        self.event("memory_info")?;
        let count = self
            .events()
            .iter()
            .filter(|&&event| event == "memory_info")
            .count();
        Ok(self.observations.memory[count - 1])
    }
    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, SimulationError> {
        assert!(bytes > 0);
        let allocation = self.create("allocate", ResourceKind::Allocation, None)?;
        self.state
            .lock()
            .expect("state")
            .numerical
            .allocations
            .insert(
                allocation.as_ptr() as usize,
                AllocationState {
                    bytes,
                    input: None,
                    output: None,
                },
            );
        Ok(allocation)
    }
    fn free(&self, allocation: OpaqueHandle) -> Result<(), SimulationError> {
        self.destroy("free", allocation)
    }
    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), SimulationError> {
        self.event("copy_to_device")?;
        let mut state = self.state.lock().expect("state");
        let id = state.check(destination, ResourceKind::Allocation);
        let allocation = state
            .numerical
            .allocations
            .get_mut(&id)
            .expect("live allocation");
        assert_eq!(allocation.bytes, size_of_val(source));
        allocation.input = Some(source.to_vec());
        state.history.numerical.uploads.insert(id, source.to_vec());
        Ok(())
    }
    fn copy_from_device(
        &self,
        source: OpaqueHandle,
        destination: &mut [Complex64Abi],
    ) -> Result<(), SimulationError> {
        self.event("copy_from_device")?;
        let state = self.state.lock().expect("state");
        let id = state.check(source, ResourceKind::Allocation);
        let allocation = &state.numerical.allocations[&id];
        let (stream, result) = allocation.output.as_ref().expect("contracted output");
        assert!(state.numerical.pending[stream].is_empty());
        assert_eq!(allocation.bytes, size_of_val(destination));
        destination.copy_from_slice(result);
        Ok(())
    }
    fn create_workspace(&self, parent: OpaqueHandle) -> Result<OpaqueHandle, SimulationError> {
        let workspace = self.create("create_workspace", ResourceKind::Workspace, Some(parent))?;
        self.state
            .lock()
            .expect("state")
            .numerical
            .workspaces
            .insert(workspace.as_ptr() as usize, WorkspaceState::default());
        Ok(workspace)
    }
    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), SimulationError> {
        self.destroy("destroy_workspace", workspace)
    }
    fn workspace_memory_size(
        &self,
        parent: OpaqueHandle,
        workspace: OpaqueHandle,
        preference: WorkspacePreference,
        space: MemorySpace,
        kind: WorkspaceKind,
    ) -> Result<i64, SimulationError> {
        self.event("workspace_memory_size")?;
        self.state
            .lock()
            .expect("state")
            .child(parent, workspace, ResourceKind::Workspace);
        let value = match (space, kind) {
            (_, WorkspaceKind::Cache) => 4096,
            (MemorySpace::Device, _) => self.numerical.device_minimum,
            (MemorySpace::Host, _) => self.numerical.host_minimum,
        };
        Ok(
            if preference == WorkspacePreference::Recommended
                && kind == WorkspaceKind::Scratch
                && value > 0
            {
                value * 2
            } else {
                value
            },
        )
    }
    fn set_workspace_memory(
        &self,
        parent: OpaqueHandle,
        workspace: OpaqueHandle,
        space: MemorySpace,
        kind: WorkspaceKind,
        allocation: Option<OpaqueHandle>,
        bytes: i64,
    ) -> Result<(), SimulationError> {
        self.event("set_workspace_memory")?;
        let address = allocation.map(|p| p.as_ptr() as usize);
        match kind {
            WorkspaceKind::Scratch => {
                assert!(allocation.is_some() && bytes > 0);
                if space == MemorySpace::Host {
                    assert_eq!(address.expect("host allocation") % 256, 0);
                }
            }
            WorkspaceKind::Cache => assert!(allocation.is_none() && bytes == 0),
        }
        let mut state = self.state.lock().expect("state");
        let id = state.child(parent, workspace, ResourceKind::Workspace);
        if space == MemorySpace::Device
            && let Some(allocation) = allocation
        {
            let allocation_id = state.check(allocation, ResourceKind::Allocation);
            assert_eq!(
                state.numerical.allocations[&allocation_id].bytes,
                usize::try_from(bytes).expect("size")
            );
        }
        let binding = (space, kind, address, bytes);
        state
            .numerical
            .workspaces
            .get_mut(&id)
            .expect("live workspace")
            .bindings
            .push(binding);
        state.history.numerical.workspace.push(binding);
        Ok(())
    }
}

impl ContractionExecutionApi for TestDoubleContractionApi {
    fn allocate_host_scratch(&self, bytes: usize) -> Result<HostScratch, SimulationError> {
        if self.event("allocate_host_scratch").is_err() {
            return Err(SimulationError::HostScratchAllocationFailed { bytes });
        }
        HostScratch::new(bytes)
    }

    fn compute_contraction_workspace(
        &self,
        parent: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        self.event("compute_contraction_workspace")?;
        let mut state = self.state.lock().expect("state");
        state.child(network, info, ResourceKind::Info);
        assert_eq!(
            state.network(parent, network).info,
            Some(info.as_ptr() as usize)
        );
        let id = state.child(parent, workspace, ResourceKind::Workspace);
        state
            .numerical
            .workspaces
            .get_mut(&id)
            .expect("live workspace")
            .network = Some(network.as_ptr() as usize);
        Ok(())
    }
    fn bind_input(
        &self,
        parent: OpaqueHandle,
        network: OpaqueHandle,
        tensor_id: i64,
        allocation: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        self.event("bind_input")?;
        let mut state = self.state.lock().expect("state");
        assert!(
            state
                .network(parent, network)
                .tensor_ids
                .contains(&tensor_id)
        );
        let allocation = state.check(allocation, ResourceKind::Allocation);
        assert!(state.numerical.allocations[&allocation].input.is_some());
        state
            .network_mut(parent, network)
            .numerical
            .inputs
            .insert(tensor_id, allocation);
        state.history.numerical.inputs.push((tensor_id, allocation));
        Ok(())
    }
    fn bind_output(
        &self,
        parent: OpaqueHandle,
        network: OpaqueHandle,
        allocation: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        self.event("bind_output")?;
        let mut state = self.state.lock().expect("state");
        let allocation = state.check(allocation, ResourceKind::Allocation);
        state.network_mut(parent, network).numerical.output = Some(allocation);
        Ok(())
    }
    fn prepare_contraction(
        &self,
        parent: OpaqueHandle,
        network: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        self.event("prepare_contraction")?;
        let mut state = self.state.lock().expect("state");
        let selected = state.network(parent, network);
        assert!(
            selected.numerical.inputs.is_empty(),
            "structural preparation has no inputs"
        );
        assert!(selected.numerical.output.is_some());
        assert!(
            !state.infos[&selected.info.expect("attached info")]
                .path
                .is_empty()
        );
        let workspace_id = state.child(parent, workspace, ResourceKind::Workspace);
        assert_eq!(
            state.numerical.workspaces[&workspace_id].network,
            Some(network.as_ptr() as usize)
        );
        state.network_mut(parent, network).numerical.prepared = true;
        Ok(())
    }
    fn contract(
        &self,
        parent: OpaqueHandle,
        network: OpaqueHandle,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), SimulationError> {
        let mut state = self.state.lock().expect("state");
        let stream_id = state.check(stream, ResourceKind::Stream);
        assert!(state.network(parent, network).numerical.prepared);
        assert_eq!(
            state.network(parent, network).numerical.inputs.len(),
            state.network(parent, network).tensors.len()
        );
        let workspace_id = state.child(parent, workspace, ResourceKind::Workspace);
        assert_eq!(
            state.numerical.workspaces[&workspace_id].network,
            Some(network.as_ptr() as usize)
        );
        let pending = state
            .numerical
            .pending
            .get_mut(&stream_id)
            .expect("live stream");
        assert!(pending.is_empty());
        pending.insert(network.as_ptr() as usize);
        state.network_mut(parent, network).numerical.stream = Some(stream_id);
        let output = state
            .network(parent, network)
            .numerical
            .output
            .expect("bound output");
        drop(state);
        self.event("contract")?;
        let mut state = self.state.lock().expect("state");
        let numerical_output = self
            .numerical
            .input_dependent
            .then(|| contract_small_network(&state, state.network(parent, network)));
        let allocation = state
            .numerical
            .allocations
            .get_mut(&output)
            .expect("live output");
        allocation.output = Some((
            stream_id,
            numerical_output.unwrap_or_else(|| {
                vec![
                    Complex64Abi::new(
                        if self.numerical.nonfinite_output {
                            f64::NAN
                        } else {
                            0.25
                        },
                        -0.125,
                    );
                    allocation.bytes / size_of::<Complex64Abi>()
                ]
            }),
        ));
        Ok(())
    }
}

/// A bounded direct sum oracle, independent of the adapter's selected schedule.
fn contract_small_network(state: &State, network: &NetworkState) -> Vec<Complex64Abi> {
    let dimensions: BTreeMap<_, _> = network
        .tensors
        .iter()
        .flat_map(|tensor| {
            tensor
                .modes
                .iter()
                .copied()
                .zip(tensor.extents.iter().copied())
        })
        .collect();
    let count: i64 = dimensions.values().product();
    assert!(count <= 4096, "analytical fixtures only");
    let output_modes = network.output.as_ref().expect("output modes");
    let output_count: i64 = output_modes.iter().map(|mode| dimensions[mode]).product();
    let mut output = vec![Complex64::new(0.0, 0.0); usize::try_from(output_count).expect("small")];
    for assignment in 0..count {
        let mut remaining = assignment;
        let coordinates: BTreeMap<_, _> = dimensions
            .iter()
            .map(|(&mode, &extent)| {
                let coordinate = remaining % extent;
                remaining /= extent;
                (mode, coordinate)
            })
            .collect();
        let offset = |modes: &[i32]| {
            let mut stride = 1;
            let mut offset = 0;
            for mode in modes {
                offset += coordinates[mode] * stride;
                stride *= dimensions[mode];
            }
            usize::try_from(offset).expect("small")
        };
        let product: Complex64 = network
            .tensors
            .iter()
            .zip(&network.tensor_ids)
            .map(|(tensor, id)| {
                let input = state.numerical.allocations[&network.numerical.inputs[id]]
                    .input
                    .as_ref()
                    .expect("uploaded");
                Complex64::from(input[offset(&tensor.modes)])
            })
            .product();
        output[offset(output_modes)] += product;
    }
    output
        .into_iter()
        .map(|v| Complex64Abi::new(v.re, v.im))
        .collect()
}

fn limits() -> ExecutionLimits {
    ExecutionLimits {
        device_scratch_bytes: Some(1024),
        host_scratch_bytes: Some(256),
    }
}

fn shared_chain() -> TensorNetwork {
    let axes: Vec<_> = [11, 23, 37, 53, 71]
        .into_iter()
        .map(|id| Index::new(id, 2).expect("axis"))
        .collect();
    TensorNetwork::new(
        axes.windows(2)
            .map(|w| Indices::new(w.to_vec()).expect("axes"))
            .collect(),
    )
    .expect("network")
}

fn shared_query(network: &TensorNetwork) -> ContractionQuery<'_> {
    ContractionQuery::new(
        network,
        Indices::new(vec![
            Index::new(11, 2).expect("axis"),
            Index::new(71, 2).expect("axis"),
        ])
        .expect("axes"),
    )
    .expect("query")
}

fn coefficients() -> Vec<Box<[Complex64]>> {
    vec![vec![Complex64::new(0.5, -0.25); 4].into_boxed_slice()]
}

fn selected_plan(query: &ContractionQuery<'_>) -> ContractionPlan {
    let axis = |id| {
        *query
            .network()
            .nodes()
            .iter()
            .flat_map(Indices::as_slice)
            .find(|axis| axis.id() == id)
            .expect("axis")
    };
    ContractionPlan::new(
        query,
        vec![
            ContractionStep::new(
                vec![Operand::Input(1), Operand::Input(2)],
                Indices::new(vec![axis(53), axis(23)]).expect("axes"),
            ),
            ContractionStep::new(
                vec![Operand::Input(0), Operand::Result(0)],
                Indices::new(
                    [53, 11]
                        .into_iter()
                        .filter(|&id| {
                            id == 53 || query.keep().as_slice().iter().any(|axis| axis.id() == id)
                        })
                        .map(axis)
                        .collect(),
                )
                .expect("axes"),
            ),
            ContractionStep::new(
                vec![Operand::Input(3), Operand::Result(1)],
                query.keep().clone(),
            ),
        ],
    )
    .expect("noncanonical logical axes")
}

pub(super) fn collapse_failure<R>(
    failure: qdk_simulators::execution::PreparationFailure<SimulationError, R>,
) -> SimulationError {
    combine_execution_and_cleanup::<()>(Err(failure.error), failure.cleanup.map_or(Ok(()), Err))
        .expect_err("primary failure")
}

fn prepare_numerical<'session>(
    session: &'session mut SessionResources<TestDoubleContractionApi>,
    query: &ContractionQuery<'_>,
    buffers: &[Box<[Complex64]>],
    bindings: &[usize],
    limits: ExecutionLimits,
) -> Result<
    (
        CuTensorNetExecutableContraction<'session, TestDoubleContractionApi>,
        Vec<InputId>,
    ),
    SimulationError,
> {
    let mut execution = session
        .prepare(query, &selected_plan(query), limits)
        .map_err(collapse_failure)?;
    let result = (|| {
        let ids = buffers
            .iter()
            .map(|values| {
                execution.register_input(
                    TensorInput {
                        dimensions: &[2, 2],
                        values,
                    },
                    InputMutability::Immutable,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        bindings
            .iter()
            .map(|&index| {
                ids.get(index)
                    .copied()
                    .ok_or_else(|| invalid("test input index out of range"))
            })
            .collect::<Result<Vec<_>, _>>()
    })();
    match result {
        Ok(inputs) => Ok((execution, inputs)),
        Err(error) => combine_execution_and_cleanup(Err(error), execution.close()),
    }
}

fn run_numerical<T>(
    api: Arc<TestDoubleContractionApi>,
    buffers: &[Box<[Complex64]>],
    bindings: &[usize],
    limits: ExecutionLimits,
    operation: impl FnOnce(
        &mut CuTensorNetExecutableContraction<'_, TestDoubleContractionApi>,
        &[InputId],
    ) -> Result<T, SimulationError>,
) -> Result<T, SimulationError> {
    let mut session = SessionResources::new(api, 0)?;
    let network = shared_chain();
    let result = (|| {
        let (mut execution, inputs) = prepare_numerical(
            &mut session,
            &shared_query(&network),
            buffers,
            bindings,
            limits,
        )?;
        let result = operation(&mut execution, &inputs);
        combine_execution_and_cleanup(result, execution.close())
    })();
    combine_execution_and_cleanup(result, session.close())
}

#[test]
fn successful_cleanup_allows_sequential_numerical_preparations() {
    let api = api(vec![], NumericalSettings::default());
    let mut session = SessionResources::new(api.clone(), 0).expect("session");
    let network = shared_chain();
    let scalar = ContractionQuery::new(&network, Indices::new(vec![]).expect("scalar axes"))
        .expect("scalar query");
    for (query, output_count) in [
        (shared_query(&network), 4),
        (scalar, 1),
        (shared_query(&network), 4),
    ] {
        let (mut execution, inputs) =
            prepare_numerical(&mut session, &query, &coefficients(), &[0; 4], limits())
                .expect("fresh preparation");
        assert_eq!(
            execution.execute(&inputs).expect("output").len(),
            output_count
        );
        execution.close().expect("successful executable cleanup");
    }
    session.close().expect("session cleanup");
    api.assert_released();
    assert_eq!(
        api.events()
            .iter()
            .filter(|&&event| event == "prepare_contraction")
            .count(),
        3
    );
}

#[test]
fn independent_live_owners_keep_their_outputs_and_cleanup_separate() {
    for cleanup_failures in [vec![], vec![("destroy_workspace", 1), ("free", 1)]] {
        let expect_cleanup_error = !cleanup_failures.is_empty();
        let api = api(cleanup_failures, NumericalSettings::default());
        let mut first_session = SessionResources::new(api.clone(), 0).expect("first session");
        let mut second_session = SessionResources::new(api.clone(), 0).expect("second session");
        let network = shared_chain();
        let scalar_query =
            ContractionQuery::new(&network, Indices::new(vec![]).expect("scalar axes"))
                .expect("scalar query");
        let (mut first, first_inputs) = prepare_numerical(
            &mut first_session,
            &shared_query(&network),
            &coefficients(),
            &[0; 4],
            limits(),
        )
        .expect("first executable");
        let (mut second, second_inputs) = prepare_numerical(
            &mut second_session,
            &scalar_query,
            &coefficients(),
            &[0; 4],
            limits(),
        )
        .expect("second executable");
        assert_eq!(first.execute(&first_inputs).expect("first output").len(), 4);
        let second_output = second.execute(&second_inputs).expect("second output");
        assert_eq!(second_output, vec![Complex64::new(0.25, -0.125)]);
        assert_eq!(first.close().is_err(), expect_cleanup_error);
        first_session.close().expect("first session cleanup");
        assert_eq!(
            second
                .execute(&second_inputs)
                .expect("surviving executable"),
            second_output
        );
        second.close().expect("second executable cleanup");
        second_session.close().expect("second session cleanup");
        api.assert_released();
    }
}

#[test]
fn a_failed_contraction_does_not_block_an_independent_stream() {
    let api = api(vec![("contract", 1)], NumericalSettings::default());
    let mut first_session = SessionResources::new(api.clone(), 0).expect("first session");
    let mut second_session = SessionResources::new(api.clone(), 0).expect("second session");
    let network = shared_chain();
    let query = shared_query(&network);
    let (mut first, first_inputs) = prepare_numerical(
        &mut first_session,
        &query,
        &coefficients(),
        &[0; 4],
        limits(),
    )
    .expect("first executable");
    let (mut second, second_inputs) = prepare_numerical(
        &mut second_session,
        &query,
        &coefficients(),
        &[0; 4],
        limits(),
    )
    .expect("second executable");
    assert!(first.execute(&first_inputs).is_err());
    assert_eq!(
        second
            .execute(&second_inputs)
            .expect("independent stream")
            .len(),
        4
    );
    second.close().expect("independent cleanup");
    second_session.close().expect("second session cleanup");
    first.close().expect("drain and close failed executable");
    first_session.close().expect("first session cleanup");
    api.assert_released();
}

#[test]
fn shared_buffers_native_ids_repeated_readback_and_owned_results() {
    let api = api(vec![], NumericalSettings::default());
    let output = run_numerical(
        api.clone(),
        &coefficients(),
        &[0; 4],
        limits(),
        |execution, inputs| {
            assert_eq!(execution.metadata()?, metadata());
            assert_eq!(execution.resources().common.selected_input_count, None);
            assert_eq!(execution.resources().common.selected_input_bytes, None);
            assert_eq!(execution.resources().common.resident_input_count, Some(1));
            assert_eq!(execution.resources().common.resident_input_bytes, Some(64));
            assert_eq!(execution.resources().common.output_bytes, Some(64));
            assert_eq!(execution.resources().common.owned_device_bytes, Some(640));
            let first = execution.execute(inputs)?;
            assert_eq!(execution.resources().common.selected_input_count, Some(1));
            assert_eq!(execution.resources().common.selected_input_bytes, Some(64));
            assert_eq!(first, execution.execute(inputs)?);
            Ok(first)
        },
    )
    .expect("execution");
    api.assert_released();
    assert_eq!(output, vec![Complex64::new(0.25, -0.125); 4]);
    let state = api.state.lock().expect("state");
    assert_eq!(state.history.numerical.uploads.len(), 1);
    assert_eq!(
        state
            .history
            .numerical
            .inputs
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [90, 7, 400, 12]
    );
    assert!(
        state
            .history
            .numerical
            .inputs
            .windows(2)
            .all(|w| w[0].1 == w[1].1)
    );
    assert!(!state.events.contains(&"optimize"));
    assert!(!state.events.contains(&"create_optimizer_config"));
    assert_eq!(
        state
            .events
            .iter()
            .filter(|&&event| event == "prepare_contraction")
            .count(),
        1
    );
}

#[test]
fn validates_bindings_and_values_before_uploading_or_binding() {
    for (buffers, bindings) in [
        (coefficients(), vec![0; 3]),
        (coefficients(), vec![1; 4]),
        (
            vec![vec![Complex64::new(1.0, 0.0); 3].into_boxed_slice()],
            vec![0; 4],
        ),
        (
            vec![vec![Complex64::new(f64::NAN, 0.0); 4].into_boxed_slice()],
            vec![0; 4],
        ),
    ] {
        let api = api(vec![], NumericalSettings::default());
        assert!(
            run_numerical(
                api.clone(),
                &buffers,
                &bindings,
                limits(),
                |execution, inputs| execution.execute(inputs)
            )
            .is_err()
        );
        api.assert_released();
        assert!(api.events().contains(&"prepare_contraction"));
        assert!(!api.events().contains(&"bind_input"));
        assert!(!api.events().contains(&"contract"));
    }
}

#[test]
fn rejects_missing_metadata_and_unqualified_numerical_slicing_before_allocation() {
    for selected in [
        None,
        Some(NativeMetadata {
            slicing: vec![SlicedMode {
                mode: 23,
                extent: 1,
            }],
            num_slices: 3,
            ..metadata()
        }),
    ] {
        let api = api(vec![], NumericalSettings::default());
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        let network = chain();
        let mut resources =
            ContractionResources::new(&mut session, &query(&network)).expect("network");
        if let Some(selected) = selected {
            resources
                .import(&selected)
                .expect("supported metadata subset");
        }
        assert!(matches!(
            CuTensorNetExecutableContraction::prepare(
                resources,
                super::adapter::supplied_plan(&query(&network)),
                limits()
            )
            .map_err(|failure| failure.error),
            Err(SimulationError::InvalidContractionConfiguration { .. }
                | SimulationError::UnsupportedContraction { .. })
        ));
        session.close().expect("session cleanup");
        api.assert_released();
        assert!(!api.events().contains(&"create_workspace"));
        assert!(!api.events().contains(&"allocate"));
    }
}

#[test]
fn retains_unselected_registered_buffers_and_keeps_distinct_bindings() {
    let api = api(vec![], NumericalSettings::default());
    let buffers = vec![
        vec![Complex64::new(0.5, -0.25); 4].into_boxed_slice(),
        vec![Complex64::new(-0.5, 0.25); 4].into_boxed_slice(),
        vec![Complex64::new(0.0, 1.0); 4].into_boxed_slice(),
    ];
    run_numerical(
        api.clone(),
        &buffers,
        &[0, 1, 0, 1],
        limits(),
        |execution, inputs| {
            execution.execute(inputs)?;
            assert_eq!(execution.resources().common.selected_input_count, Some(2));
            assert_eq!(execution.resources().common.selected_input_bytes, Some(128));
            assert_eq!(execution.resources().common.resident_input_count, Some(3));
            assert_eq!(execution.resources().common.resident_input_bytes, Some(192));
            Ok(())
        },
    )
    .expect("all explicitly registered buffers");
    api.assert_released();
    let state = api.state.lock().expect("state");
    assert_eq!(state.history.numerical.uploads.len(), 3);
    let inputs = &state.history.numerical.inputs;
    assert_eq!(inputs[0].1, inputs[2].1);
    assert_eq!(inputs[1].1, inputs[3].1);
    assert_ne!(inputs[0].1, inputs[1].1);
}

#[test]
fn honors_exact_workspace_limits_and_distinguishes_zero_scratch_from_disabled_cache() {
    for (device, host) in [(512, 128), (0, 0)] {
        let api = api(
            vec![],
            NumericalSettings {
                device_minimum: device,
                host_minimum: host,
                nonfinite_output: false,
                ..NumericalSettings::default()
            },
        );
        run_numerical(
            api.clone(),
            &coefficients(),
            &[0; 4],
            ExecutionLimits {
                device_scratch_bytes: Some(usize::try_from(device.max(256)).expect("size")),
                host_scratch_bytes: Some(usize::try_from(host).expect("size")),
            },
            |execution, inputs| {
                assert_eq!(
                    execution.resources().common.device_scratch_minimum,
                    Some(usize::try_from(device).expect("size"))
                );
                assert_eq!(
                    execution.resources().common.device_scratch_allocated,
                    Some(usize::try_from(device.max(256)).expect("size"))
                );
                assert_eq!(
                    execution.resources().common.host_scratch_allocated,
                    Some(usize::try_from(host).expect("size"))
                );
                execution.execute(inputs)
            },
        )
        .expect("exact bound");
        api.assert_released();
        let state = api.state.lock().expect("state");
        assert_eq!(
            state
                .history
                .numerical
                .workspace
                .iter()
                .filter(|(_, kind, _, _)| *kind == WorkspaceKind::Cache)
                .count(),
            2
        );
        assert_eq!(
            state
                .history
                .numerical
                .workspace
                .iter()
                .filter(|(space, kind, _, _)| *space == MemorySpace::Host
                    && *kind == WorkspaceKind::Scratch)
                .count(),
            usize::from(host > 0)
        );
    }
    for (device, host) in [(511, 128), (512, 127)] {
        let api = api(vec![], NumericalSettings::default());
        assert!(matches!(
            run_numerical(
                api.clone(),
                &coefficients(),
                &[0; 4],
                ExecutionLimits {
                    device_scratch_bytes: Some(device),
                    host_scratch_bytes: Some(host)
                },
                |_, _| Ok(())
            ),
            Err(SimulationError::WorkspaceLimitExceeded { .. })
        ));
        api.assert_released();
        assert!(!api.events().contains(&"allocate"));
    }
}

#[test]
fn observed_4x4_workspace_requirement_is_rejected_before_allocation() {
    // The native unsliced 4x4 run reported this minimum before rejecting 64 MiB.
    let api = api(
        vec![],
        NumericalSettings {
            device_minimum: 2_186_281_216,
            host_minimum: 0,
            nonfinite_output: false,
            ..NumericalSettings::default()
        },
    );
    assert!(matches!(
        run_numerical::<()>(
            api.clone(),
            &coefficients(),
            &[0; 4],
            ExecutionLimits {
                device_scratch_bytes: Some(67_108_864),
                host_scratch_bytes: Some(1_048_576),
            },
            |_, _| panic!("over-budget preparation must not reach execution"),
        ),
        Err(SimulationError::WorkspaceLimitExceeded {
            required: 2_186_281_216,
            maximum: 67_108_864,
        })
    ));
    api.assert_released();
    let events = api.events();
    assert!(events.contains(&"compute_contraction_workspace"));
    assert!(!events.contains(&"allocate"));
    assert!(!events.contains(&"prepare_contraction"));
    assert!(!events.contains(&"contract"));
}

#[test]
fn absent_workspace_limits_allocate_and_report_the_native_minimum() {
    let api = api(
        vec![],
        NumericalSettings {
            device_minimum: 2_186_281_216,
            host_minimum: 128,
            nonfinite_output: false,
            ..NumericalSettings::default()
        },
    );
    run_numerical(
        api.clone(),
        &coefficients(),
        &[0; 4],
        ExecutionLimits {
            device_scratch_bytes: None,
            host_scratch_bytes: None,
        },
        |execution, inputs| {
            assert_eq!(
                execution.resources().common.device_scratch_allocated,
                Some(2_186_281_216)
            );
            assert_eq!(
                execution.resources().common.host_scratch_allocated,
                Some(128)
            );
            assert_eq!(
                execution.resources().common.owned_device_bytes,
                Some(2_186_281_216 + 64 + 64)
            );
            execution.execute(inputs)
        },
    )
    .expect("no policy ceiling");
    api.assert_released();
}

#[test]
#[cfg(target_pointer_width = "64")]
fn overnight_policy_enforces_device_ceiling_independently_of_host_scratch() {
    const CEILING: usize = 32 * 1024 * 1024 * 1024;
    for required in [CEILING, CEILING + 256] {
        let api = api(
            vec![],
            NumericalSettings {
                device_minimum: i64::try_from(required).expect("size"),
                host_minimum: 1_048_832,
                nonfinite_output: false,
                ..NumericalSettings::default()
            },
        );
        let result = run_numerical(
            api.clone(),
            &coefficients(),
            &[0; 4],
            ExecutionLimits {
                device_scratch_bytes: Some(CEILING),
                host_scratch_bytes: None,
            },
            |execution, inputs| {
                assert_eq!(
                    execution.resources().common.device_scratch_allocated,
                    Some(CEILING)
                );
                assert_eq!(
                    execution.resources().common.host_scratch_allocated,
                    Some(1_048_832)
                );
                execution.execute(inputs)
            },
        );
        if required == CEILING {
            result.expect("inclusive device limit and no host policy ceiling");
        } else {
            assert!(matches!(
                result,
                Err(SimulationError::WorkspaceLimitExceeded { required: value, maximum })
                    if value == required && maximum == CEILING
            ));
            assert!(!api.events().contains(&"allocate"));
        }
        api.assert_released();
    }
}

#[test]
fn native_negative_workspace_and_nonfinite_readback_are_errors() {
    for settings in [
        NumericalSettings {
            device_minimum: -1,
            ..NumericalSettings::default()
        },
        NumericalSettings {
            host_minimum: -1,
            ..NumericalSettings::default()
        },
        NumericalSettings {
            nonfinite_output: true,
            ..NumericalSettings::default()
        },
    ] {
        let api = api(vec![], settings);
        assert!(matches!(
            run_numerical(
                api.clone(),
                &coefficients(),
                &[0; 4],
                limits(),
                |execution, inputs| execution.execute(inputs)
            ),
            Err(SimulationError::InvalidNativeResult { .. })
        ));
        api.assert_released();
    }
}

#[test]
fn every_numerical_boundary_reports_failures_and_releases_resources() {
    let mut points = vec![
        ("create_workspace", 1),
        ("compute_contraction_workspace", 1),
        ("copy_to_device", 1),
        ("bind_output", 1),
        ("prepare_contraction", 1),
        ("contract", 1),
        ("synchronize_stream", 1),
        ("copy_from_device", 1),
    ];
    points.extend((1..=6).map(|n| ("workspace_memory_size", n)));
    points.extend((1..=3).map(|n| ("allocate", n)));
    points.extend((1..=4).map(|n| ("set_workspace_memory", n)));
    points.extend((1..=4).map(|n| ("bind_input", n)));
    for point in points {
        let api = api(vec![point], NumericalSettings::default());
        let error = run_numerical(
            api.clone(),
            &coefficients(),
            &[0; 4],
            limits(),
            |execution, inputs| execution.execute(inputs),
        )
        .expect_err("injected failure");
        assert!(error.to_string().contains(point.0), "{point:?}: {error}");
        api.assert_released();
    }
}

#[test]
fn execution_failure_invalidates_reuse_and_preserves_cleanup_failures() {
    let api = api(
        vec![("contract", 1), ("destroy_workspace", 1), ("free", 1)],
        NumericalSettings::default(),
    );
    let error = run_numerical(
        api.clone(),
        &coefficients(),
        &[0; 4],
        limits(),
        |execution, inputs| {
            let result = execution.execute(inputs);
            assert!(matches!(
                execution.execute(inputs),
                Err(SimulationError::UnusableContraction)
            ));
            assert!(matches!(
                execution.metadata(),
                Err(SimulationError::UnusableContraction)
            ));
            assert!(matches!(
                execution.intermediate_modes(),
                Err(SimulationError::UnusableContraction)
            ));
            result
        },
    )
    .expect_err("execution and cleanup");
    for operation in ["contract", "destroy_workspace", "free"] {
        assert!(error.to_string().contains(operation));
    }
    api.assert_released();
}

#[test]
fn preparation_failure_preserves_cleanup_errors() {
    let api = api(
        vec![
            ("prepare_contraction", 1),
            ("destroy_workspace", 1),
            ("free", 1),
        ],
        NumericalSettings::default(),
    );
    let error = run_numerical(api.clone(), &coefficients(), &[0; 4], limits(), |_, _| {
        Ok(())
    })
    .expect_err("preparation and cleanup");
    for operation in ["prepare_contraction", "destroy_workspace", "free"] {
        assert!(error.to_string().contains(operation));
    }
    api.assert_released();
}

#[test]
fn drop_and_explicit_cleanup_failures_attempt_every_release_once() {
    for failure in [
        ("synchronize_stream", 2),
        ("destroy_workspace", 1),
        ("destroy_network", 1),
        ("free", 1),
        ("free", 2),
        ("free", 3),
    ] {
        let api = api(vec![failure], NumericalSettings::default());
        assert!(
            run_numerical(
                api.clone(),
                &coefficients(),
                &[0; 4],
                limits(),
                |execution, inputs| execution.execute(inputs)
            )
            .is_err()
        );
        api.assert_released();
        assert_eq!(
            api.events()
                .iter()
                .filter(|&&event| event == "free")
                .count(),
            3
        );
    }
    let api = api(vec![], NumericalSettings::default());
    {
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        let network = shared_chain();
        let (mut execution, inputs) = prepare_numerical(
            &mut session,
            &shared_query(&network),
            &coefficients(),
            &[0; 4],
            limits(),
        )
        .expect("prepare");
        execution.execute(&inputs).expect("contract");
    }
    api.assert_released();
    assert_eq!(
        api.events()
            .iter()
            .filter(|&&event| event == "destroy_workspace")
            .count(),
        1
    );
}

fn calls(api: &TestDoubleContractionApi, name: &str) -> usize {
    api.events().iter().filter(|&&event| event == name).count()
}

fn matrix(values: &[Complex64]) -> TensorInput<'_> {
    TensorInput {
        dimensions: &[2, 2],
        values,
    }
}

fn real(values: &[f64]) -> Vec<Complex64> {
    values
        .iter()
        .map(|&value| Complex64::new(value, 0.0))
        .collect()
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one prepare/reuse/close lifecycle including independent output evidence"
)]
fn resident_candidates_diverge_rejoin_and_replace_without_repreparation_or_reupload() {
    let api = api(
        vec![],
        NumericalSettings {
            input_dependent: true,
            ..NumericalSettings::default()
        },
    );
    let mut session = SessionResources::new(api.clone(), 0).expect("session");
    let mut execution = {
        let network = shared_chain();
        let query = shared_query(&network);
        let selected = selected_plan(&query);
        let executable = session
            .prepare(&query, &selected, limits())
            .expect("structural prepare");
        assert_eq!(executable.plan(), &selected);
        executable
    };
    assert_eq!(calls(&api, "copy_to_device"), 0);
    assert_eq!(calls(&api, "bind_input"), 0);
    let identity = execution
        .register_input(matrix(&real(&[1., 0., 0., 1.])), InputMutability::Immutable)
        .expect("identity");
    let pauli_x = execution
        .register_input(matrix(&real(&[0., 1., 1., 0.])), InputMutability::Immutable)
        .expect("X");
    let mutable = execution
        .register_input(matrix(&real(&[2., 0., 0., 3.])), InputMutability::Mutable)
        .expect("general nonunitary");
    let phase = [
        Complex64::new(1., 0.),
        Complex64::new(0., 0.),
        Complex64::new(0., 0.),
        Complex64::new(0., 1.),
    ];
    let phase_id = execution
        .register_input(matrix(&phase), InputMutability::Immutable)
        .expect("S candidate");
    let allocations = calls(&api, "allocate");
    let a = execution
        .execute(&[identity, identity, mutable, identity])
        .expect("A");
    assert_eq!(a, real(&[2., 0., 0., 3.]));
    let b = execution
        .execute(&[pauli_x, identity, mutable, identity])
        .expect("B");
    assert_eq!(b, real(&[0., 2., 3., 0.]));
    assert_eq!(
        execution
            .execute(&[pauli_x, pauli_x, mutable, identity])
            .expect("rejoin"),
        a
    );
    assert_eq!(calls(&api, "copy_to_device"), 4);
    let replacement = [
        Complex64::new(2., 0.),
        Complex64::new(0., 0.),
        Complex64::new(0., 1.),
        Complex64::new(3., 0.),
    ];
    execution
        .replace_input(mutable, matrix(&replacement))
        .expect("same capacity replacement");
    let c = execution
        .execute(&[pauli_x, identity, mutable, identity])
        .expect("C");
    assert_eq!(
        c,
        [
            Complex64::new(0., 0.),
            Complex64::new(2., 0.),
            Complex64::new(3., 0.),
            Complex64::new(0., 1.)
        ]
    );
    assert_eq!(
        execution
            .execute(&[identity, identity, mutable, identity])
            .expect("only one slot changed"),
        replacement
    );
    assert_eq!(
        execution
            .execute(&[identity, identity, mutable, mutable])
            .expect("all users see replacement"),
        [
            Complex64::new(4., 0.),
            Complex64::new(0., 0.),
            Complex64::new(0., 5.),
            Complex64::new(9., 0.)
        ]
    );
    assert_eq!(
        execution
            .execute(&[phase_id, identity, identity, identity])
            .expect("non-Pauli candidate"),
        phase
    );
    let bindings = calls(&api, "bind_input");
    assert_eq!(
        execution
            .execute(&[phase_id, identity, identity, identity])
            .expect("warm selection"),
        phase
    );
    assert_eq!(calls(&api, "bind_input"), bindings);
    assert_eq!(calls(&api, "allocate"), allocations);
    assert_eq!(calls(&api, "copy_to_device"), 5);
    assert_eq!(calls(&api, "prepare_contraction"), 1);
    assert_eq!(calls(&api, "optimize"), 0);
    assert_eq!(calls(&api, "create_optimizer_config"), 0);
    let report = execution.resources().as_ref();
    assert_eq!(report.resident_input_count, Some(4));
    assert_eq!(report.resident_input_bytes, Some(256));
    assert_eq!(report.selected_input_count, Some(2));
    assert_eq!(report.selected_input_bytes, Some(128));
    assert_eq!(report.owned_device_bytes, Some(512 + 64 + 256));
    execution.close().expect("close");
    session.close().expect("session close");
    assert_eq!(a, real(&[2., 0., 0., 3.]));
    assert_eq!(b, real(&[0., 2., 3., 0.]));
    assert_eq!(c[3], Complex64::new(0., 1.));
    api.assert_released();
}

#[test]
fn optimizer_selected_plan_and_reordered_output_use_the_same_context_path() {
    use crate::simulation::contraction::adapter::{
        CuTensorNetContractionOptimizer, CuTensorNetContractionOptimizerSettings,
    };
    use qdk_simulators::execution::{ContractionOptimizer, PlanningConstraints};
    for optimized in [false, true] {
        let api = api(
            vec![],
            NumericalSettings {
                input_dependent: true,
                ..NumericalSettings::default()
            },
        );
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        let network = shared_chain();
        let query = ContractionQuery::new(
            &network,
            Indices::new(vec![
                Index::new(71, 2).expect("axis"),
                Index::new(11, 2).expect("axis"),
            ])
            .expect("axes"),
        )
        .expect("query");
        let plan = if optimized {
            CuTensorNetContractionOptimizer::new(&mut session)
                .optimize(
                    &query,
                    PlanningConstraints {
                        workspace_bytes: Some(1024),
                    },
                    CuTensorNetContractionOptimizerSettings {
                        hyper_samples: 1,
                        threads: 1,
                        seed: 17,
                        reconfiguration_iterations: 0,
                        disable_rank_simplification: true,
                    },
                )
                .expect("selected")
                .0
        } else {
            selected_plan(&query)
        };
        let searches = calls(&api, "optimize");
        let configs = calls(&api, "create_optimizer_config");
        let mut execution = session.prepare(&query, &plan, limits()).expect("prepare");
        assert_eq!(execution.plan(), &plan);
        let i = execution
            .register_input(matrix(&real(&[1., 0., 0., 1.])), InputMutability::Immutable)
            .expect("I");
        let r1 = execution
            .register_input(matrix(&real(&[0., 0., 1., 0.])), InputMutability::Immutable)
            .expect("nonunitary reset branch");
        assert_eq!(
            execution.execute(&[i, r1, i, i]).expect("ordered output"),
            real(&[0., 1., 0., 0.])
        );
        assert_eq!(calls(&api, "optimize"), searches);
        assert_eq!(calls(&api, "create_optimizer_config"), configs);
        execution.close().expect("close");
        session.close().expect("session");
        api.assert_released();
    }
}

fn assert_poisoned(
    execution: &mut CuTensorNetExecutableContraction<'_, TestDoubleContractionApi>,
    api: &TestDoubleContractionApi,
    id: InputId,
) {
    let report = execution.resources().clone();
    let before = api.events();
    assert!(matches!(
        execution.execute(&[id; 4]),
        Err(SimulationError::UnusableContraction)
    ));
    assert!(matches!(
        execution.register_input(matrix(&real(&[1.; 4])), InputMutability::Mutable),
        Err(SimulationError::UnusableContraction)
    ));
    assert!(matches!(
        execution.replace_input(id, matrix(&real(&[1.; 4]))),
        Err(SimulationError::UnusableContraction)
    ));
    assert_eq!(execution.resources(), &report);
    assert_eq!(api.events(), before);
}

#[test]
fn all_input_validation_failures_poison_before_native_side_effects() {
    // Each rejection gets a fresh owner; no recovery exception for preflight.
    for case in 0..11 {
        let api = api(vec![], NumericalSettings::default());
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        let mut foreign_session = SessionResources::new(api.clone(), 0).expect("other session");
        let network = shared_chain();
        let query = shared_query(&network);
        let plan = selected_plan(&query);
        let mut execution = session.prepare(&query, &plan, limits()).expect("prepare");
        let mut other = foreign_session
            .prepare(&query, &plan, limits())
            .expect("other");
        let foreign = other
            .register_input(matrix(&real(&[1.; 4])), InputMutability::Mutable)
            .expect("foreign");
        let id = execution
            .register_input(matrix(&real(&[1.; 4])), InputMutability::Mutable)
            .expect("mutable");
        let immutable = execution
            .register_input(matrix(&real(&[1.; 4])), InputMutability::Immutable)
            .expect("immutable");
        let wrong_shape = execution
            .register_input(
                TensorInput {
                    dimensions: &[4],
                    values: &real(&[1.; 4]),
                },
                InputMutability::Immutable,
            )
            .expect("valid but incompatible");
        let wrong_order = execution
            .register_input(
                TensorInput {
                    dimensions: &[1, 4],
                    values: &real(&[1.; 4]),
                },
                InputMutability::Immutable,
            )
            .expect("same rank and byte count, different ordered dimensions");
        execution.execute(&[id; 4]).expect("previous selection");
        let previous = execution.resources().clone();
        let before = api.events();
        let result = match case {
            0 => execution
                .register_input(matrix(&real(&[1.; 3])), InputMutability::Immutable)
                .map(|_| ()),
            1 => execution
                .register_input(matrix(&real(&[f64::NAN; 4])), InputMutability::Immutable)
                .map(|_| ()),
            2 => execution
                .register_input(
                    TensorInput {
                        dimensions: &[0],
                        values: &[],
                    },
                    InputMutability::Immutable,
                )
                .map(|_| ()),
            3 => execution.replace_input(
                id,
                TensorInput {
                    dimensions: &[4],
                    values: &real(&[1.; 4]),
                },
            ),
            4 => execution.replace_input(immutable, matrix(&real(&[1.; 4]))),
            5 => execution.replace_input(id, matrix(&real(&[f64::INFINITY; 4]))),
            6 => execution.replace_input(foreign, matrix(&real(&[1.; 4]))),
            7 => execution.execute(&[id; 3]).map(|_| ()),
            8 => execution.execute(&[id, id, id, foreign]).map(|_| ()),
            9 => execution.execute(&[id, id, id, wrong_shape]).map(|_| ()),
            10 => execution.execute(&[id, id, id, wrong_order]).map(|_| ()),
            _ => unreachable!(),
        };
        assert!(
            matches!(
                result,
                Err(SimulationError::InvalidContractionConfiguration { .. })
            ),
            "case {case}"
        );
        assert_eq!(
            api.events(),
            before,
            "case {case}: validation before any native work"
        );
        assert_eq!(execution.resources(), &previous);
        assert_poisoned(&mut execution, &api, id);
        execution.close().expect("failed owner close");
        other.close().expect("other close");
        session.close().expect("session close");
        foreign_session.close().expect("foreign close");
        api.assert_released();
    }
}

#[test]
fn registration_upload_and_partial_binding_failures_retain_acquisitions_and_selection_evidence() {
    for point in [("allocate", 4), ("copy_to_device", 2), ("bind_input", 3)] {
        let api = api(vec![point], NumericalSettings::default());
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        let network = shared_chain();
        let query = shared_query(&network);
        let mut execution = session
            .prepare(&query, &selected_plan(&query), limits())
            .expect("prepare");
        let id = execution
            .register_input(matrix(&real(&[1.; 4])), InputMutability::Mutable)
            .expect("prior input");
        let registration =
            execution.register_input(matrix(&real(&[1.; 4])), InputMutability::Mutable);
        if point.0 == "bind_input" {
            registration.expect("registered");
            assert!(execution.execute(&[id; 4]).is_err());
            assert_eq!(calls(&api, "bind_input"), 3);
            assert_eq!(calls(&api, "contract"), 0);
            assert_eq!(execution.resources().common.selected_input_count, Some(1));
            assert_eq!(execution.resources().common.selected_input_bytes, Some(64));
        } else {
            assert!(registration.is_err());
            assert_eq!(execution.resources().common.selected_input_count, None);
        }
        let acquired = 1 + usize::from(point.0 != "allocate");
        assert_eq!(
            execution.resources().common.resident_input_count,
            Some(acquired)
        );
        assert_eq!(
            execution.resources().common.resident_input_bytes,
            Some(64 * acquired)
        );
        assert_eq!(
            execution.resources().common.owned_device_bytes,
            Some(576 + 64 * acquired)
        );
        assert_poisoned(&mut execution, &api, id);
        execution.close().expect("close");
        session.close().expect("session");
        api.assert_released();
    }
}

#[test]
fn failed_replacement_keeps_prior_output_and_prohibits_consuming_partial_values() {
    let api = api(
        vec![("copy_to_device", 2)],
        NumericalSettings {
            input_dependent: true,
            ..NumericalSettings::default()
        },
    );
    let mut session = SessionResources::new(api.clone(), 0).expect("session");
    let network = shared_chain();
    let query = shared_query(&network);
    let mut execution = session
        .prepare(&query, &selected_plan(&query), limits())
        .expect("prepare");
    let id = execution
        .register_input(matrix(&real(&[1., 0., 0., 1.])), InputMutability::Mutable)
        .expect("input");
    let output = execution.execute(&[id; 4]).expect("first");
    let report = execution.resources().clone();
    assert!(
        execution
            .replace_input(id, matrix(&real(&[2.; 4])))
            .is_err()
    );
    assert_eq!(execution.resources(), &report);
    assert_eq!(calls(&api, "allocate"), 3);
    assert_poisoned(&mut execution, &api, id);
    execution.close().expect("close");
    session.close().expect("session");
    assert_eq!(output, real(&[1., 0., 0., 1.]));
    api.assert_released();
}

#[test]
fn preparation_records_each_observation_and_acquisition_before_the_next_failure() {
    let mut points = vec![
        ("create_workspace", 1),
        ("compute_contraction_workspace", 1),
        ("allocate_host_scratch", 1),
        ("bind_output", 1),
        ("prepare_contraction", 1),
    ];
    points.extend((1..=6).map(|n| ("workspace_memory_size", n)));
    points.extend((1..=2).map(|n| ("allocate", n)));
    points.extend((1..=4).map(|n| ("set_workspace_memory", n)));
    for point in points {
        let api = api(
            vec![point, ("destroy_network", 1)],
            NumericalSettings::default(),
        );
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        let network = shared_chain();
        let query = shared_query(&network);
        let failure = session
            .prepare(&query, &selected_plan(&query), limits())
            .err()
            .expect("failure");
        let report = &failure.partial;
        let observed = match point {
            ("create_workspace" | "compute_contraction_workspace", _) => 0,
            ("workspace_memory_size", n) => n - 1,
            _ => 6,
        };
        for (index, (actual, expected)) in [
            (report.common.device_scratch_minimum, 512),
            (report.common.device_scratch_recommended, 1024),
            (report.common.host_scratch_minimum, 128),
            (report.common.host_scratch_recommended, 256),
            (report.device_cache_recommended, 4096),
            (report.host_cache_recommended, 4096),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(actual, (index < observed).then_some(expected), "{point:?}");
        }
        let device = match point {
            ("create_workspace" | "compute_contraction_workspace" | "workspace_memory_size", _)
            | ("allocate", 1) => 0,
            _ => 512,
        };
        let host = if device == 0 || point.0 == "allocate_host_scratch" {
            0
        } else {
            128
        };
        let output = usize::from(matches!(point.0, "bind_output" | "prepare_contraction")) * 64;
        assert_eq!(report.common.output_bytes, Some(64), "{point:?}");
        assert_eq!(
            report.common.device_scratch_allocated,
            Some(device),
            "{point:?}"
        );
        assert_eq!(
            report.common.host_scratch_allocated,
            Some(host),
            "{point:?}"
        );
        assert_eq!(
            report.common.owned_device_bytes,
            Some(device + output),
            "{point:?}"
        );
        assert_eq!(report.common.resident_input_count, Some(0));
        assert!(
            failure
                .cleanup
                .expect("separate cleanup")
                .to_string()
                .contains("destroy_network")
        );
        if point.0 == "allocate_host_scratch" {
            assert!(matches!(
                failure.error,
                SimulationError::HostScratchAllocationFailed { bytes: 128 }
            ));
        } else {
            assert!(failure.error.to_string().contains(point.0));
        }
        session.close().expect("session");
        api.assert_released();
    }
}

#[test]
fn earliest_topology_and_import_failures_keep_primary_and_cleanup_separate() {
    let mut points = vec![
        ("set_device", 2),
        ("create_network", 1),
        ("set_output", 1),
        ("set_compute_f64", 1),
        ("create_optimizer_info", 1),
        ("set_path", 1),
        ("set_slicing", 1),
        ("attach_optimizer_info", 1),
        ("read_path", 1),
        ("num_sliced_modes", 1),
        ("read_slicing", 1),
        ("num_slices", 1),
        ("intermediate_mode_counts", 1),
        ("intermediate_modes", 1),
    ];
    points.extend((1..=4).map(|n| ("append_tensor", n)));
    for point in points {
        let api = api(
            vec![point, ("destroy_network", 1)],
            NumericalSettings::default(),
        );
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        let network = shared_chain();
        let query = shared_query(&network);
        let failure = session
            .prepare(&query, &selected_plan(&query), limits())
            .err()
            .expect("failure");
        assert!(
            matches!(failure.error, SimulationError::NativeCallFailed { .. }),
            "{point:?}: {}",
            failure.error
        );
        assert!(failure.error.to_string().contains(point.0));
        assert_eq!(
            failure.cleanup.is_some(),
            !matches!(point.0, "set_device" | "create_network")
        );
        assert_eq!(failure.partial.common.owned_device_bytes, Some(0));
        assert_eq!(failure.partial.common.output_bytes, None);
        assert_eq!(failure.partial.common.device_scratch_minimum, None);
        session.close().expect("session");
        api.assert_released();
    }
}

#[test]
fn scratch_ceiling_failures_retain_requirements_but_never_project_allocations() {
    for limits in [
        ExecutionLimits {
            device_scratch_bytes: Some(0),
            host_scratch_bytes: None,
        },
        ExecutionLimits {
            device_scratch_bytes: Some(511),
            host_scratch_bytes: None,
        },
        ExecutionLimits {
            device_scratch_bytes: None,
            host_scratch_bytes: Some(0),
        },
        ExecutionLimits {
            device_scratch_bytes: None,
            host_scratch_bytes: Some(127),
        },
    ] {
        let api = api(vec![], NumericalSettings::default());
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        let network = shared_chain();
        let query = shared_query(&network);
        let failure = session
            .prepare(&query, &selected_plan(&query), limits)
            .err()
            .expect("limit");
        assert!(matches!(
            failure.error,
            SimulationError::WorkspaceLimitExceeded { .. }
        ));
        assert_eq!(failure.partial.common.device_scratch_minimum, Some(512));
        assert_eq!(failure.partial.common.host_scratch_minimum, Some(128));
        assert_eq!(failure.partial.device_cache_recommended, Some(4096));
        assert_eq!(failure.partial.common.device_scratch_allocated, Some(0));
        assert_eq!(failure.partial.common.host_scratch_allocated, Some(0));
        assert_eq!(failure.partial.common.owned_device_bytes, Some(0));
        assert_eq!(calls(&api, "allocate"), 0);
        session.close().expect("session");
        api.assert_released();
    }
}

#[test]
fn retained_identity_cannot_alias_a_later_executable_on_the_same_session() {
    let api = api(vec![], NumericalSettings::default());
    let mut session = SessionResources::new(api.clone(), 0).expect("session");
    let network = shared_chain();
    let query = shared_query(&network);
    let plan = selected_plan(&query);
    let mut first = session.prepare(&query, &plan, limits()).expect("first");
    let stale = first
        .register_input(matrix(&real(&[1.; 4])), InputMutability::Mutable)
        .expect("input");
    first.close().expect("close first");
    let mut second = session.prepare(&query, &plan, limits()).expect("second");
    let current = second
        .register_input(matrix(&real(&[1.; 4])), InputMutability::Mutable)
        .expect("input");
    assert!(second.execute(&[stale; 4]).is_err());
    assert_eq!(second.resources().common.selected_input_bytes, None);
    assert_eq!(calls(&api, "bind_input"), 0);
    assert_poisoned(&mut second, &api, current);
    second.close().expect("close second");
    session.close().expect("close session");
    api.assert_released();
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "complete two-mode analytical fixture and reuse assertions"
)]
fn general_multiqubit_tensors_reuse_the_same_numerical_api() {
    let api = Arc::new(TestDoubleContractionApi {
        state: Mutex::new(State::default()),
        failures: vec![],
        corruption: Corruption::None,
        numerical: NumericalSettings {
            input_dependent: true,
            ..NumericalSettings::default()
        },
        observations: NativeObservations {
            selected: NativeMetadata {
                path: vec![[0, 1]],
                slicing: vec![],
                num_slices: 1,
            },
            use_query_output: true,
            ..NativeObservations::default()
        },
    });
    let axes = |ids: &[u32]| {
        Indices::new(
            ids.iter()
                .map(|&id| Index::new(id, 2).expect("axis"))
                .collect(),
        )
        .expect("axes")
    };
    let network = TensorNetwork::new(vec![axes(&[0, 1, 2, 3]), axes(&[2, 3])])
        .expect("two-mode operator and input");
    let query = ContractionQuery::new(&network, axes(&[0, 1])).expect("query");
    let plan = ContractionPlan::new(
        &query,
        vec![ContractionStep::new(
            vec![Operand::Input(0), Operand::Input(1)],
            query.keep().clone(),
        )],
    )
    .expect("plan");
    let mut session = SessionResources::new(api.clone(), 0).expect("session");
    let mut execution = session.prepare(&query, &plan, limits()).expect("prepare");
    let mut nonunitary = real(&[0.; 16]);
    nonunitary[0] = Complex64::new(0.5, 0.);
    nonunitary[15] = Complex64::new(0.25, 0.);
    let mut joint = real(&[0.; 16]);
    for column in 0..4 {
        joint[4 * column + (column ^ 3)] = Complex64::new(1., 0.);
    }
    let filter = execution
        .register_input(
            TensorInput {
                dimensions: &[2; 4],
                values: &nonunitary,
            },
            InputMutability::Immutable,
        )
        .expect("general filter");
    let joint = execution
        .register_input(
            TensorInput {
                dimensions: &[2; 4],
                values: &joint,
            },
            InputMutability::Immutable,
        )
        .expect("joint operator");
    let input = [
        Complex64::new(1., 0.),
        Complex64::new(0., 0.),
        Complex64::new(0., 0.),
        Complex64::new(0., 1.),
    ];
    let input = execution
        .register_input(matrix(&input), InputMutability::Immutable)
        .expect("input");
    let expected = [
        Complex64::new(0.5, 0.),
        Complex64::new(0., 0.),
        Complex64::new(0., 0.),
        Complex64::new(0., 0.25),
    ];
    assert_eq!(
        execution.execute(&[filter, input]).expect("filter"),
        expected
    );
    assert_eq!(
        execution.execute(&[joint, input]).expect("joint"),
        [
            Complex64::new(0., 1.),
            Complex64::new(0., 0.),
            Complex64::new(0., 0.),
            Complex64::new(1., 0.)
        ]
    );
    assert_eq!(
        execution.execute(&[filter, input]).expect("reuse"),
        expected
    );
    assert_eq!(calls(&api, "copy_to_device"), 3);
    assert_eq!(calls(&api, "prepare_contraction"), 1);
    execution.close().expect("close");
    session.close().expect("session");
    api.assert_released();
}
