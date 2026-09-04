#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use super::query::BaseQueryResult;
use super::{
    Circuit, Gate, OpaqueHandle, SimulationError, SimulationResult, Stream, branch,
    circuit::{
        ExecutionReport, StatePhaseTimings, StateReadout, WorkspaceReport, contract_open_mps,
    },
    ffi::Complex64Abi,
    policy::ExecutionPolicy,
    query::{
        AdjacentZQuery, B2_EXPECTATION_HYPER_SAMPLES, QueryPhaseTimings, QueryResult,
        normalize_expectation,
    },
    sampler::{FullBitstringSamples, PreparedSampler, SamplerApi, SamplerContext, SamplingRequest},
};
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use crate::library::Session;
use num_complex::Complex64;
use qdk_simulators::QubitID;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::sync::Arc;
use std::{f64::consts::FRAC_1_SQRT_2, mem::size_of, time::Instant};

#[cfg(test)]
#[path = "replay/tests.rs"]
mod tests;

pub(crate) struct MpsTarget {
    extents: Vec<Box<[i64]>>,
    extent_pointers: Box<[*const i64]>,
    output_elements: Vec<usize>,
}

impl MpsTarget {
    fn new(qubit_count: usize, bond_cap: i64) -> Result<Self, SimulationError> {
        // cuTensorNet requires at least two sites for MPS finalization
        // (bindings/v2_13.rs:338). This crate implements only MPS and has one
        // unconditional finalize_mps call below, so it cannot represent one site.
        if qubit_count < 2 {
            return Err(SimulationError::InvalidCircuit {
                reason: "MPS simulation requires at least two qubits; use type=\"cpu\" for single-qubit circuits".to_string(),
            });
        }
        let bonds = (0..qubit_count - 1)
            .map(|cut| target_bond_extent(qubit_count, cut, bond_cap))
            .collect::<Result<Vec<_>, _>>()?;
        let mut extents = Vec::with_capacity(qubit_count);
        for site in 0..qubit_count {
            let shape = if site == 0 {
                vec![2, bonds[0]]
            } else if site + 1 == qubit_count {
                vec![bonds[site - 1], 2]
            } else {
                vec![bonds[site - 1], 2, bonds[site]]
            };
            extents.push(shape.into_boxed_slice());
        }
        let output_elements = extents
            .iter()
            .map(|shape| checked_element_count(shape, "MPS output"))
            .collect::<Result<Vec<_>, _>>()?;
        let extent_pointers = extents
            .iter()
            .map(|shape| shape.as_ptr())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(Self {
            extents,
            extent_pointers,
            output_elements,
        })
    }

    pub(crate) fn extent_pointers(&self) -> &[*const i64] {
        &self.extent_pointers
    }
}

pub(crate) struct OutputMetadata {
    pub(crate) extents: Vec<Box<[i64]>>,
    pub(crate) strides: Vec<Box<[i64]>>,
}

impl OutputMetadata {
    fn new(target: &MpsTarget) -> Self {
        let extents = target
            .extents
            .iter()
            .map(|shape| vec![0; shape.len()].into_boxed_slice())
            .collect();
        let strides = target
            .extents
            .iter()
            .map(|shape| vec![0; shape.len()].into_boxed_slice())
            .collect();
        Self { extents, strides }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StateF64Attribute {
    SvdAbsoluteCutoff,
    SvdRelativeCutoff,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StateU32Configuration {
    SvdAlgorithmGesvd,
    MpsGaugeSimple,
}

pub(crate) trait ReplayApi {
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
    fn create_state(
        &self,
        handle: OpaqueHandle,
        mode_extents: &[i64],
    ) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_state(&self, state: OpaqueHandle) -> Result<(), SimulationError>;
    fn apply_tensor_operator(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        modes: &[i32],
        tensor: OpaqueHandle,
        unitary: bool,
    ) -> Result<(), SimulationError>;
    fn finalize_mps(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        target: &MpsTarget,
    ) -> Result<(), SimulationError>;
    fn capture_mps(&self, handle: OpaqueHandle, state: OpaqueHandle)
    -> Result<(), SimulationError>;
    fn configure_state_f64(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        attribute: StateF64Attribute,
        value: f64,
    ) -> Result<(), SimulationError>;
    fn configure_state_u32(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        configuration: StateU32Configuration,
    ) -> Result<(), SimulationError>;
    fn create_workspace(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), SimulationError>;
    fn prepare_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), SimulationError>;
    fn workspace_size(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<i64, SimulationError>;
    fn set_workspace(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        allocation: OpaqueHandle,
        bytes: i64,
    ) -> Result<(), SimulationError>;
    fn compute_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        workspace: OpaqueHandle,
        metadata: &mut OutputMetadata,
        outputs: &mut [OpaqueHandle],
        stream: Stream,
    ) -> Result<(), SimulationError>;
    fn synchronize_stream(&self, stream: Stream) -> Result<(), SimulationError>;
    fn create_network_operator(
        &self,
        handle: OpaqueHandle,
        mode_extents: &[i64],
    ) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_network_operator(&self, operator: OpaqueHandle) -> Result<(), SimulationError>;
    fn append_product(
        &self,
        handle: OpaqueHandle,
        operator: OpaqueHandle,
        coefficient: Complex64,
        factor_modes: &[Box<[i32]>],
        factor_tensors: &[OpaqueHandle],
    ) -> Result<(), SimulationError>;
    fn create_expectation(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        operator: OpaqueHandle,
    ) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_expectation(&self, expectation: OpaqueHandle) -> Result<(), SimulationError>;
    fn configure_expectation_hyper_samples(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), SimulationError>;
    fn prepare_expectation(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), SimulationError>;
    fn compute_expectation(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(Complex64, Complex64), SimulationError>;
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl Session {
    pub(crate) fn sample(
        &mut self,
        circuit: &Circuit,
        sampled_qubits: &[QubitID],
        request: SamplingRequest,
    ) -> Result<Box<[i64]>, SimulationError> {
        let mut replay = Replay::new(
            self.api(),
            self.handle(),
            self.stream(),
            circuit,
            self.policy(),
        )?;
        let execution = replay.sample_qubits(circuit, sampled_qubits, request);
        let cleanup = replay.close();
        combine_execution_and_cleanup(execution, cleanup)
    }

    #[cfg(test)]
    pub(super) fn simulate_with_branch(
        &mut self,
        initial_circuit: &Circuit,
        branch_request: branch::BranchRequest,
        continuation_circuit: &Circuit,
        query: &AdjacentZQuery,
    ) -> Result<branch::BranchSimulationResult, SimulationError> {
        let overall_started = Instant::now();
        let mut replay = Replay::new(
            self.api(),
            self.handle(),
            self.stream(),
            initial_circuit,
            self.policy(),
        )?;
        let execution =
            replay.execute_branch(initial_circuit, branch_request, continuation_circuit, query);
        let cleanup_started = Instant::now();
        let cleanup = replay.close();
        let cleanup_seconds = cleanup_started.elapsed().as_secs_f64();
        let mut result = combine_execution_and_cleanup(execution, cleanup)?;
        let (free_after_cleanup_bytes, _) = self.api().memory_info()?;
        result
            .initial_state
            .report
            .workspace
            .free_after_cleanup_bytes = free_after_cleanup_bytes;
        result
            .post_projection_state
            .report
            .workspace
            .free_after_cleanup_bytes = free_after_cleanup_bytes;
        result
            .continuation_state
            .report
            .workspace
            .free_after_cleanup_bytes = free_after_cleanup_bytes;
        result.query.workspace.free_after_cleanup_bytes = free_after_cleanup_bytes;
        result.report.timings.cleanup_seconds = cleanup_seconds;
        result.report.timings.total_wall_seconds = overall_started.elapsed().as_secs_f64();
        Ok(result)
    }

    fn simulate(
        &mut self,
        circuit: &Circuit,
        readout: StateReadout,
    ) -> Result<SimulationResult, SimulationError> {
        let mut replay = Replay::new(
            self.api(),
            self.handle(),
            self.stream(),
            circuit,
            self.policy(),
        )?;
        let execution = replay.execute(circuit, readout);
        let cleanup = replay.close();
        let mut result = combine_execution_and_cleanup(execution, cleanup)?;
        let (free_after_cleanup_bytes, _) = replay.api.memory_info()?;
        result.report.workspace.free_after_cleanup_bytes = free_after_cleanup_bytes;
        Ok(result)
    }

    fn simulate_and_query(
        &mut self,
        circuit: &Circuit,
        query: &AdjacentZQuery,
    ) -> Result<BaseQueryResult, SimulationError> {
        let through_query_started = Instant::now();
        let mut replay = Replay::new(
            self.api(),
            self.handle(),
            self.stream(),
            circuit,
            self.policy(),
        )?;
        let execution = (|| {
            let state = replay.execute(circuit, StateReadout::MetadataOnly)?;
            let query = replay.execute_query(query)?;
            Ok((state, query))
        })();
        let through_query_completion_seconds = through_query_started.elapsed().as_secs_f64();
        let cleanup_started = Instant::now();
        let cleanup = replay.close();
        let replay_cleanup_seconds = cleanup_started.elapsed().as_secs_f64();
        let (mut state, mut query) = combine_execution_and_cleanup(execution, cleanup)?;
        let (free_after_cleanup_bytes, _) = replay.api.memory_info()?;
        state.report.workspace.free_after_cleanup_bytes = free_after_cleanup_bytes;
        query.workspace.free_after_cleanup_bytes = free_after_cleanup_bytes;
        Ok(BaseQueryResult {
            state,
            query,
            through_query_completion_seconds,
            replay_cleanup_seconds,
        })
    }
}

struct Replay<'api, Api: ReplayApi + ?Sized> {
    api: &'api Api,
    handle: OpaqueHandle,
    stream: Stream,
    state: Option<OpaqueHandle>,
    workspace: Option<OpaqueHandle>,
    query_workspace: Option<OpaqueHandle>,
    network_operator: Option<OpaqueHandle>,
    expectation: Option<OpaqueHandle>,
    allocations: Vec<OpaqueHandle>,
    state_extents: Box<[i64]>,
    operator_modes: Vec<Box<[i32]>>,
    target: MpsTarget,
    policy: ExecutionPolicy,
    closed: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct OwnedOperator {
    modes: Box<[i32]>,
    matrix: Box<[Complex64Abi]>,
}

impl OwnedOperator {
    fn new(modes: Vec<i32>, matrix: Vec<Complex64Abi>) -> Result<Self, SimulationError> {
        if !(1..=2).contains(&modes.len()) {
            return Err(invalid_operator("operator must act on one or two modes"));
        }
        if modes.iter().any(|mode| *mode < 0) {
            return Err(invalid_operator("operator modes must be nonnegative"));
        }
        if modes.len() == 2 && modes[0] == modes[1] {
            return Err(invalid_operator("two-site operator modes must differ"));
        }
        let expected_elements = 1_usize << (2 * modes.len());
        if matrix.len() != expected_elements {
            return Err(invalid_operator(
                "operator matrix shape does not match its arity",
            ));
        }
        Ok(Self {
            modes: modes.into_boxed_slice(),
            matrix: matrix.into_boxed_slice(),
        })
    }
}

fn invalid_operator(reason: &'static str) -> SimulationError {
    SimulationError::InvalidCircuit {
        reason: reason.to_string(),
    }
}

impl<'api, Api: ReplayApi + ?Sized> Replay<'api, Api> {
    fn new(
        api: &'api Api,
        handle: OpaqueHandle,
        stream: Stream,
        circuit: &Circuit,
        policy: ExecutionPolicy,
    ) -> Result<Self, SimulationError> {
        let policy = policy.validate()?;
        let qubit_count = usize::try_from(circuit.qubit_count()).map_err(|_| {
            SimulationError::ResourceSizeOverflow {
                resource: "state mode count",
            }
        })?;
        let target = MpsTarget::new(qubit_count, policy.bond_cap)?;
        let state_extents = vec![2; qubit_count].into_boxed_slice();
        let state = api.create_state(handle, state_extents.as_ref())?;
        let mut replay = Self {
            api,
            handle,
            stream,
            state: Some(state),
            workspace: None,
            query_workspace: None,
            network_operator: None,
            expectation: None,
            allocations: Vec::new(),
            state_extents,
            operator_modes: Vec::new(),
            target,
            policy,
            closed: false,
        };
        match api.create_workspace(handle) {
            Ok(workspace) => replay.workspace = Some(workspace),
            Err(error) => {
                return combine_execution_and_cleanup(Err(error), replay.close());
            }
        }
        Ok(replay)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the ordered state lifecycle is one failure and timing transaction"
    )]
    fn execute_branch(
        &mut self,
        initial_circuit: &Circuit,
        branch_request: branch::BranchRequest,
        continuation_circuit: &Circuit,
        query: &AdjacentZQuery,
    ) -> Result<branch::BranchSimulationResult, SimulationError> {
        use branch::{BranchPhaseTimings, BranchReport, BranchSimulationResult, SelectedBranch};

        let mut timings = BranchPhaseTimings::default();
        let phase_started = Instant::now();
        let initial_state = self.execute(initial_circuit, StateReadout::FullAmplitudes)?;
        let initial_wall_seconds = phase_started.elapsed().as_secs_f64();
        timings.first_barrier_synchronization_seconds =
            initial_state.report.timings.synchronization_seconds;
        timings.initial_execution_seconds =
            (initial_wall_seconds - timings.first_barrier_synchronization_seconds).max(0.0);

        let phase_started = Instant::now();
        self.capture_current_state()?;
        timings.first_capture_seconds = phase_started.elapsed().as_secs_f64();

        let phase_started = Instant::now();
        let (masses, mass_synchronization_seconds) =
            compute_branch_masses(self, branch_request.mode)?;
        timings.mass_computation_seconds =
            (phase_started.elapsed().as_secs_f64() - mass_synchronization_seconds).max(0.0);
        timings.mass_synchronization_seconds = mass_synchronization_seconds;

        let selected_mass = match branch_request.selected {
            SelectedBranch::Zero => masses.q0,
            SelectedBranch::One => masses.q1,
        };
        if selected_mass <= 0.0 {
            return Err(SimulationError::InvalidCircuit {
                reason: "cannot project onto zero-mass branch before mutation".to_string(),
            });
        }
        let probability = masses.probability(branch_request.selected)?;
        let log_probability = masses.log_probability(branch_request.selected)?;

        let phase_started = Instant::now();
        apply_projection(
            self,
            branch_request.mode,
            branch_request.selected,
            selected_mass,
        )?;
        timings.projection_registration_seconds = phase_started.elapsed().as_secs_f64();

        let post_projection_state = self.materialize_current_state(
            StateReadout::FullAmplitudes,
            StatePhaseTimings::default(),
        )?;
        timings.projection_preparation_compute_seconds =
            preparation_compute_seconds(&post_projection_state.report.timings);
        timings.projection_barrier_synchronization_seconds =
            post_projection_state.report.timings.synchronization_seconds;

        let phase_started = Instant::now();
        self.capture_current_state()?;
        timings.projection_capture_seconds = phase_started.elapsed().as_secs_f64();

        let phase_started = Instant::now();
        let mut continuation_timings = StatePhaseTimings::default();
        for gate in continuation_circuit.gates() {
            self.register_gate(*gate, &mut continuation_timings)?;
        }
        timings.continuation_registration_seconds = phase_started.elapsed().as_secs_f64();
        let continuation_state =
            self.materialize_current_state(StateReadout::FullAmplitudes, continuation_timings)?;
        timings.continuation_preparation_compute_seconds =
            preparation_compute_seconds(&continuation_state.report.timings);
        timings.continuation_barrier_synchronization_seconds =
            continuation_state.report.timings.synchronization_seconds;

        let phase_started = Instant::now();
        let query_result = self.execute_query(query)?;
        timings.query_seconds = phase_started.elapsed().as_secs_f64();
        Ok(BranchSimulationResult {
            initial_state,
            post_projection_state,
            continuation_state,
            query: query_result,
            report: BranchReport {
                request: branch_request,
                masses,
                probability,
                log_probability,
                timings,
            },
        })
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the ordered state lifecycle is one failure and timing transaction"
    )]
    fn execute(
        &mut self,
        circuit: &Circuit,
        readout: StateReadout,
    ) -> Result<SimulationResult, SimulationError> {
        let mut timings = StatePhaseTimings::default();
        self.finalize_initial(circuit, &mut timings)?;
        self.materialize_current_state(readout, timings)
    }

    fn sample_full_bitstrings(
        &mut self,
        circuit: &Circuit,
        request: SamplingRequest,
    ) -> Result<FullBitstringSamples, SimulationError>
    where
        Api: SamplerApi,
    {
        let sampled_qubits = (0..self.state_extents.len()).collect::<Vec<_>>();
        let output = self.sample_qubits(circuit, &sampled_qubits, request)?;
        FullBitstringSamples::new(sampled_qubits.len(), output)
    }

    fn sample_qubits(
        &mut self,
        circuit: &Circuit,
        sampled_qubits: &[QubitID],
        request: SamplingRequest,
    ) -> Result<Box<[i64]>, SimulationError>
    where
        Api: SamplerApi,
    {
        self.finalize_initial(circuit, &mut StatePhaseTimings::default())?;
        drop(
            self.materialize_current_state(
                StateReadout::MetadataOnly,
                StatePhaseTimings::default(),
            )?,
        );
        let modes_to_sample = sampled_qubits
            .iter()
            .map(|&qubit| {
                let qubit = u32::try_from(qubit).map_err(|_| SimulationError::InvalidCircuit {
                    reason: format!("qubit {qubit} does not fit the native mode identifier"),
                })?;
                if qubit >= circuit.qubit_count() {
                    return Err(SimulationError::InvalidCircuit {
                        reason: format!(
                            "qubit {qubit} is outside a {}-qubit circuit",
                            circuit.qubit_count()
                        ),
                    });
                }
                mode_id(qubit)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (free_before_bytes, _) = self.api.memory_info()?;
        let workspace = self.workspace();
        let mut sampler = PreparedSampler::new(
            self.api,
            SamplerContext {
                handle: self.handle,
                state: self.state(),
                workspace,
                stream: self.stream,
                maximum_workspace_bytes: self.policy.maximum_workspace_bytes,
            },
            &modes_to_sample,
            &request,
        )?;
        let execution = (|| {
            let workspace_bytes = self.api.workspace_size(self.handle, workspace)?;
            if workspace_bytes <= 0 {
                return Err(SimulationError::InvalidNativeResult {
                    reason: format!("recommended sampler workspace size is {workspace_bytes}"),
                });
            }
            let workspace_size = usize::try_from(workspace_bytes).map_err(|_| {
                SimulationError::ResourceSizeOverflow {
                    resource: "sampler device workspace",
                }
            })?;
            validate_workspace_size(
                workspace_size,
                self.policy.maximum_workspace_bytes,
                free_before_bytes,
            )?;
            let scratch = self.allocate_bytes(workspace_size, "sampler device workspace")?;
            if !(scratch.as_ptr() as usize).is_multiple_of(256) {
                return Err(SimulationError::InvalidNativeResult {
                    reason: "cudaMalloc returned sampler workspace below 256-byte alignment"
                        .to_string(),
                });
            }
            self.api
                .set_workspace(self.handle, workspace, scratch, workspace_bytes)?;
            let output = sampler.sample(&request, workspace, self.stream)?;
            Ok(output)
        })();
        combine_execution_and_cleanup(execution, sampler.close())
    }

    fn finalize_initial(
        &mut self,
        circuit: &Circuit,
        timings: &mut StatePhaseTimings,
    ) -> Result<(), SimulationError> {
        for gate in circuit.gates() {
            self.register_gate(*gate, timings)?;
        }
        let state = self.state();
        let phase_started = Instant::now();
        self.api.finalize_mps(self.handle, state, &self.target)?;
        self.configure(state)?;
        timings.finalization_configuration_seconds = phase_started.elapsed().as_secs_f64();
        Ok(())
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the ordered state materialization is one failure and timing transaction"
    )]
    fn materialize_current_state(
        &mut self,
        readout: StateReadout,
        mut timings: StatePhaseTimings,
    ) -> Result<SimulationResult, SimulationError> {
        let state = self.state();
        let phase_started = Instant::now();
        let output_elements = self.target.output_elements.clone();
        let mut outputs = Vec::with_capacity(output_elements.len());
        for elements in output_elements {
            outputs.push(self.allocate_complex(elements, "MPS output")?);
        }
        timings.workspace_allocation_attachment_seconds += phase_started.elapsed().as_secs_f64();
        let phase_started = Instant::now();
        let (free_before_bytes, total_bytes) = self.api.memory_info()?;
        let workspace = self.workspace();
        self.api.prepare_state(
            self.handle,
            state,
            self.policy.maximum_workspace_bytes,
            workspace,
            self.stream,
        )?;
        let workspace_bytes = self.api.workspace_size(self.handle, workspace)?;
        if workspace_bytes <= 0 {
            return Err(SimulationError::InvalidNativeResult {
                reason: format!("recommended workspace size is {workspace_bytes}"),
            });
        }
        let workspace_size = usize::try_from(workspace_bytes).map_err(|_| {
            SimulationError::ResourceSizeOverflow {
                resource: "device workspace",
            }
        })?;
        validate_workspace_size(
            workspace_size,
            self.policy.maximum_workspace_bytes,
            free_before_bytes,
        )?;
        timings.preparation_workspace_sizing_seconds = phase_started.elapsed().as_secs_f64();
        let phase_started = Instant::now();
        let scratch = self.allocate_bytes(workspace_size, "device workspace")?;
        if !(scratch.as_ptr() as usize).is_multiple_of(256) {
            return Err(SimulationError::InvalidNativeResult {
                reason: "cudaMalloc returned workspace below 256-byte alignment".to_string(),
            });
        }
        self.api
            .set_workspace(self.handle, workspace, scratch, workspace_bytes)?;
        timings.workspace_allocation_attachment_seconds += phase_started.elapsed().as_secs_f64();

        let mut metadata = OutputMetadata::new(&self.target);
        let phase_started = Instant::now();
        self.api.compute_state(
            self.handle,
            state,
            workspace,
            &mut metadata,
            &mut outputs,
            self.stream,
        )?;
        timings.state_compute_call_seconds = phase_started.elapsed().as_secs_f64();
        let phase_started = Instant::now();
        self.api.synchronize_stream(self.stream)?;
        timings.synchronization_seconds = phase_started.elapsed().as_secs_f64();

        let phase_started = Instant::now();
        let extents = convert_layout("extent", &metadata.extents)?;
        let target_extents = convert_layout("target extent", &self.target.extents)?;
        let transferred = if readout == StateReadout::FullAmplitudes {
            let mut host_outputs = Vec::with_capacity(outputs.len());
            for (output, elements) in outputs.iter().zip(&self.target.output_elements) {
                let mut host = vec![Complex64Abi::default(); *elements];
                self.api.copy_from_device(*output, &mut host)?;
                host_outputs.push(host.into_iter().map(Complex64::from).collect::<Vec<_>>());
            }
            let strides = convert_layout("stride", &metadata.strides)?;
            Some((host_outputs, strides))
        } else {
            None
        };
        timings.output_metadata_transfer_seconds = phase_started.elapsed().as_secs_f64();
        let phase_started = Instant::now();
        validate_realized_extents(&extents, &self.target.extents)?;
        let maximum_bond = maximum_bond(&extents)?;
        let amplitudes = transferred
            .map(|(host_outputs, strides)| contract_open_mps(&host_outputs, &extents, &strides))
            .transpose()?;
        timings.host_validation_seconds = phase_started.elapsed().as_secs_f64();
        Ok(SimulationResult::new(
            amplitudes,
            ExecutionReport {
                policy: self.policy,
                target_extents,
                realized_extents: extents,
                maximum_bond,
                workspace: WorkspaceReport {
                    total_bytes,
                    free_before_bytes,
                    requested_maximum_bytes: self.policy.maximum_workspace_bytes,
                    native_recommended_bytes: workspace_size,
                    allocated_bytes: workspace_size,
                    free_after_cleanup_bytes: 0,
                },
                timings,
            },
        ))
    }

    fn capture_current_state(&self) -> Result<(), SimulationError> {
        self.api.capture_mps(self.handle, self.state())
    }

    fn register_gate(
        &mut self,
        gate: Gate,
        timings: &mut StatePhaseTimings,
    ) -> Result<(), SimulationError> {
        let phase_started = Instant::now();
        let operator = fixture_operator(gate)?;
        timings.matrix_construction_seconds += phase_started.elapsed().as_secs_f64();
        self.register_operator(operator, timings)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the separate Query lifecycle is one failure and timing transaction"
    )]
    fn execute_query(&mut self, query: &AdjacentZQuery) -> Result<QueryResult, SimulationError> {
        let mut timings = QueryPhaseTimings::default();
        if usize::try_from(query.width).ok() != Some(self.state_extents.len()) {
            return Err(SimulationError::InvalidCircuit {
                reason: "Query width does not match the native state".to_string(),
            });
        }
        if self.network_operator.is_some()
            || self.expectation.is_some()
            || self.query_workspace.is_some()
        {
            return Err(SimulationError::InvalidCircuit {
                reason: "this replay already owns a Query lifecycle".to_string(),
            });
        }

        let phase_started = Instant::now();
        let operator = self
            .api
            .create_network_operator(self.handle, self.state_extents.as_ref())?;
        self.network_operator = Some(operator);
        let z_matrix = [
            Complex64Abi::new(1.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(-1.0, 0.0),
        ];
        let z_tensor = self.allocate_complex(z_matrix.len(), "Query Z tensor")?;
        self.api.copy_to_device(z_tensor, &z_matrix)?;
        for [left, right] in &query.terms {
            let factor_modes = vec![
                vec![mode_id(*left)?].into_boxed_slice(),
                vec![mode_id(*right)?].into_boxed_slice(),
            ];
            self.api.append_product(
                self.handle,
                operator,
                Complex64::new(1.0, 0.0),
                &factor_modes,
                &[z_tensor, z_tensor],
            )?;
        }

        let expectation = self
            .api
            .create_expectation(self.handle, self.state(), operator)?;
        self.expectation = Some(expectation);
        self.api.configure_expectation_hyper_samples(
            self.handle,
            expectation,
            B2_EXPECTATION_HYPER_SAMPLES,
        )?;
        let query_workspace = self.api.create_workspace(self.handle)?;
        self.query_workspace = Some(query_workspace);
        timings.construction_seconds = phase_started.elapsed().as_secs_f64();

        let phase_started = Instant::now();
        let (free_before_bytes, total_bytes) = self.api.memory_info()?;
        self.api.prepare_expectation(
            self.handle,
            expectation,
            self.policy.maximum_workspace_bytes,
            query_workspace,
            self.stream,
        )?;
        let workspace_bytes = self.api.workspace_size(self.handle, query_workspace)?;
        if workspace_bytes < 0 {
            return Err(SimulationError::InvalidNativeResult {
                reason: format!("recommended Query workspace size is {workspace_bytes}"),
            });
        }
        let workspace_size = usize::try_from(workspace_bytes).map_err(|_| {
            SimulationError::ResourceSizeOverflow {
                resource: "Query device workspace",
            }
        })?;
        validate_workspace_size(
            workspace_size,
            self.policy.maximum_workspace_bytes,
            free_before_bytes,
        )?;
        timings.preparation_path_planning_seconds = phase_started.elapsed().as_secs_f64();

        let phase_started = Instant::now();
        if workspace_size > 0 {
            let scratch = self.allocate_bytes(workspace_size, "Query device workspace")?;
            if !(scratch.as_ptr() as usize).is_multiple_of(256) {
                return Err(SimulationError::InvalidNativeResult {
                    reason: "cudaMalloc returned Query workspace below 256-byte alignment"
                        .to_string(),
                });
            }
            self.api
                .set_workspace(self.handle, query_workspace, scratch, workspace_bytes)?;
        }
        timings.workspace_allocation_attachment_seconds = phase_started.elapsed().as_secs_f64();

        let phase_started = Instant::now();
        let (raw_expectation, squared_norm) =
            self.api
                .compute_expectation(self.handle, expectation, query_workspace, self.stream)?;
        timings.compute_call_seconds = phase_started.elapsed().as_secs_f64();

        let phase_started = Instant::now();
        self.api.synchronize_stream(self.stream)?;
        timings.synchronization_seconds = phase_started.elapsed().as_secs_f64();

        let phase_started = Instant::now();
        let mut result = normalize_expectation(raw_expectation, squared_norm)?;
        result.workspace = WorkspaceReport {
            total_bytes,
            free_before_bytes,
            requested_maximum_bytes: self.policy.maximum_workspace_bytes,
            native_recommended_bytes: workspace_size,
            allocated_bytes: workspace_size,
            free_after_cleanup_bytes: 0,
        };
        timings.output_validation_seconds = phase_started.elapsed().as_secs_f64();
        result.timings = timings;
        Ok(result)
    }

    fn execute_projector_expectation(
        &mut self,
        mode: i32,
        diagonal: [f64; 2],
    ) -> Result<(Complex64, Complex64, f64), SimulationError> {
        if self.network_operator.is_some()
            || self.expectation.is_some()
            || self.query_workspace.is_some()
        {
            return Err(SimulationError::InvalidCircuit {
                reason: "this replay already owns a Query lifecycle".to_string(),
            });
        }
        let execution = (|| {
            let matrix = [
                Complex64Abi::new(diagonal[0], 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(diagonal[1], 0.0),
            ];
            let tensor = self.allocate_complex(matrix.len(), "branch projector")?;
            self.api.copy_to_device(tensor, &matrix)?;
            let operator = self
                .api
                .create_network_operator(self.handle, self.state_extents.as_ref())?;
            self.network_operator = Some(operator);
            self.api.append_product(
                self.handle,
                operator,
                Complex64::new(1.0, 0.0),
                &[vec![mode].into_boxed_slice()],
                &[tensor],
            )?;
            let expectation = self
                .api
                .create_expectation(self.handle, self.state(), operator)?;
            self.expectation = Some(expectation);
            self.api.configure_expectation_hyper_samples(
                self.handle,
                expectation,
                B2_EXPECTATION_HYPER_SAMPLES,
            )?;
            let workspace = self.api.create_workspace(self.handle)?;
            self.query_workspace = Some(workspace);
            let (free_before_bytes, _) = self.api.memory_info()?;
            self.api.prepare_expectation(
                self.handle,
                expectation,
                self.policy.maximum_workspace_bytes,
                workspace,
                self.stream,
            )?;
            let workspace_bytes = self.api.workspace_size(self.handle, workspace)?;
            if workspace_bytes < 0 {
                return Err(SimulationError::InvalidNativeResult {
                    reason: format!("recommended property workspace size is {workspace_bytes}"),
                });
            }
            let workspace_size = usize::try_from(workspace_bytes).map_err(|_| {
                SimulationError::ResourceSizeOverflow {
                    resource: "property device workspace",
                }
            })?;
            validate_workspace_size(
                workspace_size,
                self.policy.maximum_workspace_bytes,
                free_before_bytes,
            )?;
            if workspace_size > 0 {
                let scratch = self.allocate_bytes(workspace_size, "property device workspace")?;
                if !(scratch.as_ptr() as usize).is_multiple_of(256) {
                    return Err(SimulationError::InvalidNativeResult {
                        reason: "cudaMalloc returned property workspace below 256-byte alignment"
                            .to_string(),
                    });
                }
                self.api
                    .set_workspace(self.handle, workspace, scratch, workspace_bytes)?;
            }
            let values =
                self.api
                    .compute_expectation(self.handle, expectation, workspace, self.stream)?;
            let synchronization_started = Instant::now();
            self.api.synchronize_stream(self.stream)?;
            Ok((
                values.0,
                values.1,
                synchronization_started.elapsed().as_secs_f64(),
            ))
        })();
        let cleanup = self.close_query_lifecycle();
        combine_execution_and_cleanup(execution, cleanup)
    }

    fn register_operator(
        &mut self,
        operator: OwnedOperator,
        timings: &mut StatePhaseTimings,
    ) -> Result<(), SimulationError> {
        let phase_started = Instant::now();
        let tensor = self.allocate_complex(operator.matrix.len(), "operator tensor")?;
        self.api.copy_to_device(tensor, &operator.matrix)?;
        timings.upload_seconds += phase_started.elapsed().as_secs_f64();
        self.operator_modes.push(operator.modes);
        let modes = self
            .operator_modes
            .last()
            .expect("registered operator modes were just retained");
        let phase_started = Instant::now();
        let result = self
            .api
            .apply_tensor_operator(self.handle, self.state(), modes, tensor, true);
        timings.operator_registration_seconds += phase_started.elapsed().as_secs_f64();
        result
    }

    fn configure(&self, state: OpaqueHandle) -> Result<(), SimulationError> {
        self.api.configure_state_f64(
            self.handle,
            state,
            StateF64Attribute::SvdAbsoluteCutoff,
            self.policy.absolute_cutoff,
        )?;
        self.api.configure_state_f64(
            self.handle,
            state,
            StateF64Attribute::SvdRelativeCutoff,
            self.policy.relative_cutoff,
        )?;
        self.api.configure_state_u32(
            self.handle,
            state,
            StateU32Configuration::SvdAlgorithmGesvd,
        )?;
        self.api
            .configure_state_u32(self.handle, state, StateU32Configuration::MpsGaugeSimple)
    }

    fn allocate_complex(
        &mut self,
        elements: usize,
        resource: &'static str,
    ) -> Result<OpaqueHandle, SimulationError> {
        let bytes = elements
            .checked_mul(size_of::<Complex64Abi>())
            .ok_or(SimulationError::ResourceSizeOverflow { resource })?;
        self.allocate_bytes(bytes, resource)
    }

    fn allocate_bytes(
        &mut self,
        bytes: usize,
        resource: &'static str,
    ) -> Result<OpaqueHandle, SimulationError> {
        if bytes == 0 {
            return Err(SimulationError::InvalidNativeResult {
                reason: format!("{resource} requires zero bytes"),
            });
        }
        let allocation = self.api.allocate(bytes)?;
        self.allocations.push(allocation);
        Ok(allocation)
    }

    fn state(&self) -> OpaqueHandle {
        self.state.expect("a live replay always owns its state")
    }

    fn workspace(&self) -> OpaqueHandle {
        self.workspace
            .expect("a live replay always owns its workspace descriptor")
    }

    fn close(&mut self) -> Result<(), SimulationError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let mut first_error = self.api.synchronize_stream(self.stream).err();
        if let Err(error) = self.close_query_lifecycle()
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Some(workspace) = self.workspace.take()
            && let Err(error) = self.api.destroy_workspace(workspace)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Some(state) = self.state.take()
            && let Err(error) = self.api.destroy_state(state)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        while let Some(allocation) = self.allocations.pop() {
            if let Err(error) = self.api.free(allocation)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn close_query_lifecycle(&mut self) -> Result<(), SimulationError> {
        let mut first_error = None;
        if let Some(workspace) = self.query_workspace.take()
            && let Err(error) = self.api.destroy_workspace(workspace)
        {
            first_error = Some(error);
        }
        if let Some(expectation) = self.expectation.take()
            && let Err(error) = self.api.destroy_expectation(expectation)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Some(operator) = self.network_operator.take()
            && let Err(error) = self.api.destroy_network_operator(operator)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        first_error.map_or(Ok(()), Err)
    }
}

fn compute_branch_masses<Api: ReplayApi + ?Sized>(
    replay: &mut Replay<'_, Api>,
    mode: u32,
) -> Result<(branch::BranchMasses, f64), SimulationError> {
    let mode_id = mode_id(mode)?;
    let (raw_p0, norm_p0, sync_p0) = replay.execute_projector_expectation(mode_id, [1.0, 0.0])?;
    let (raw_p1, norm_p1, sync_p1) = replay.execute_projector_expectation(mode_id, [0.0, 1.0])?;
    let masses = branch::BranchMasses::from_expectations(raw_p0, norm_p0, raw_p1, norm_p1)?;
    Ok((masses, sync_p0 + sync_p1))
}

fn apply_projection<Api: ReplayApi + ?Sized>(
    replay: &mut Replay<'_, Api>,
    mode: u32,
    selected: branch::SelectedBranch,
    selected_mass: f64,
) -> Result<(), SimulationError> {
    if !selected_mass.is_finite() || selected_mass <= 0.0 {
        return Err(SimulationError::InvalidCircuit {
            reason: "cannot project onto a nonpositive or non-finite branch mass".to_string(),
        });
    }
    let mode_id = mode_id(mode)?;
    let scale = 1.0 / selected_mass.sqrt();
    let projector = match selected {
        branch::SelectedBranch::Zero => vec![
            Complex64Abi::new(scale, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
        ],
        branch::SelectedBranch::One => vec![
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(scale, 0.0),
        ],
    };
    let tensor = replay.allocate_complex(projector.len(), "projection operator")?;
    replay.api.copy_to_device(tensor, &projector)?;
    replay
        .api
        .apply_tensor_operator(replay.handle, replay.state(), &[mode_id], tensor, false)
}

fn preparation_compute_seconds(timings: &StatePhaseTimings) -> f64 {
    timings.preparation_workspace_sizing_seconds
        + timings.workspace_allocation_attachment_seconds
        + timings.state_compute_call_seconds
}

fn fixture_operator(gate: Gate) -> Result<OwnedOperator, SimulationError> {
    let (modes, matrix) = match gate {
        Gate::X { target } => (
            vec![mode_id(target)?],
            vec![
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(1.0, 0.0),
                Complex64Abi::new(1.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
            ],
        ),
        Gate::H { target } => (
            vec![mode_id(target)?],
            vec![
                Complex64Abi::new(FRAC_1_SQRT_2, 0.0),
                Complex64Abi::new(FRAC_1_SQRT_2, 0.0),
                Complex64Abi::new(FRAC_1_SQRT_2, 0.0),
                Complex64Abi::new(-FRAC_1_SQRT_2, 0.0),
            ],
        ),
        Gate::Rx { theta, target } => {
            let (sine, cosine) = (theta / 2.0).sin_cos();
            (
                vec![mode_id(target)?],
                vec![
                    Complex64Abi::new(cosine, 0.0),
                    Complex64Abi::new(0.0, -sine),
                    Complex64Abi::new(0.0, -sine),
                    Complex64Abi::new(cosine, 0.0),
                ],
            )
        }
        Gate::Rz { theta, target } => {
            let (sine, cosine) = (theta / 2.0).sin_cos();
            (
                vec![mode_id(target)?],
                vec![
                    Complex64Abi::new(cosine, -sine),
                    Complex64Abi::new(0.0, 0.0),
                    Complex64Abi::new(0.0, 0.0),
                    Complex64Abi::new(cosine, sine),
                ],
            )
        }
        Gate::Cnot { control, target } => (
            vec![mode_id(control)?, mode_id(target)?],
            vec![
                Complex64Abi::new(1.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(1.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(1.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
                Complex64Abi::new(1.0, 0.0),
                Complex64Abi::new(0.0, 0.0),
            ],
        ),
    };
    OwnedOperator::new(modes, matrix)
}

impl<Api: ReplayApi + ?Sized> Drop for Replay<'_, Api> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn convert_layout(label: &str, values: &[Box<[i64]>]) -> Result<Vec<Vec<usize>>, SimulationError> {
    let mut converted = Vec::with_capacity(values.len());
    for shape in values {
        let mut converted_shape = Vec::with_capacity(shape.len());
        for value in shape {
            if *value <= 0 {
                return Err(SimulationError::InvalidNativeResult {
                    reason: format!("MPS {label} is not positive: {values:?}"),
                });
            }
            converted_shape.push(usize::try_from(*value).map_err(|_| {
                SimulationError::InvalidNativeResult {
                    reason: format!("MPS {label} does not fit usize: {values:?}"),
                }
            })?);
        }
        converted.push(converted_shape);
    }
    Ok(converted)
}

fn validate_realized_extents(
    realized: &[Vec<usize>],
    target: &[Box<[i64]>],
) -> Result<(), SimulationError> {
    if realized.len() != target.len() {
        return Err(invalid_native_extents(
            "realized site count differs from target",
        ));
    }
    for (site, (realized_shape, target_shape)) in realized.iter().zip(target).enumerate() {
        if realized_shape.len() != target_shape.len() {
            return Err(invalid_native_extents(
                "realized tensor rank differs from target",
            ));
        }
        for (realized_extent, target_extent) in realized_shape.iter().zip(target_shape.iter()) {
            let target_extent = usize::try_from(*target_extent)
                .map_err(|_| invalid_native_extents("target extent does not fit usize"))?;
            if *realized_extent > target_extent {
                return Err(invalid_native_extents(
                    "realized extent exceeds target capacity",
                ));
            }
        }
        let physical_mode = usize::from(site != 0);
        if realized_shape[physical_mode] != 2 {
            return Err(invalid_native_extents(
                "realized physical extent is not two",
            ));
        }
        if site > 0 && realized[site - 1][realized[site - 1].len() - 1] != realized_shape[0] {
            return Err(invalid_native_extents(
                "adjacent realized bond extents differ",
            ));
        }
    }
    Ok(())
}

fn maximum_bond(realized: &[Vec<usize>]) -> Result<usize, SimulationError> {
    if matches!(realized, [shape] if shape.as_slice() == [2]) {
        return Ok(1);
    }
    realized
        .iter()
        .take(realized.len().saturating_sub(1))
        .map(|shape| shape.last().copied())
        .collect::<Option<Vec<_>>>()
        .and_then(|bonds| bonds.into_iter().max())
        .ok_or_else(|| invalid_native_extents("realized MPS contains no bond"))
}

fn invalid_native_extents(reason: &'static str) -> SimulationError {
    SimulationError::InvalidNativeResult {
        reason: reason.to_string(),
    }
}

fn target_bond_extent(
    qubit_count: usize,
    cut: usize,
    requested_cap: i64,
) -> Result<i64, SimulationError> {
    if requested_cap <= 0 {
        return Err(SimulationError::InvalidCircuit {
            reason: "invalid MPS bond request".to_string(),
        });
    }
    let Some(left_mode_count) = cut.checked_add(1) else {
        return Err(SimulationError::InvalidCircuit {
            reason: "invalid MPS bond request".to_string(),
        });
    };
    let Some(right_mode_count) = qubit_count
        .checked_sub(left_mode_count)
        .filter(|count| *count > 0)
    else {
        return Err(SimulationError::InvalidCircuit {
            reason: "invalid MPS bond request".to_string(),
        });
    };
    let left = saturating_power_of_two(left_mode_count);
    let right = saturating_power_of_two(right_mode_count);
    Ok(requested_cap.min(left).min(right))
}

fn saturating_power_of_two(exponent: usize) -> i64 {
    let first_overflowing_exponent =
        usize::try_from(i64::BITS - 1).expect("the i64 bit width should fit usize");
    if exponent >= first_overflowing_exponent {
        i64::MAX
    } else {
        1_i64 << exponent
    }
}

fn validate_workspace_size(
    required: usize,
    policy_maximum: usize,
    free_bytes: usize,
) -> Result<(), SimulationError> {
    if required > policy_maximum {
        return Err(SimulationError::WorkspaceLimitExceeded {
            required,
            maximum: policy_maximum,
        });
    }
    if required > free_bytes {
        return Err(SimulationError::WorkspaceLimitExceeded {
            required,
            maximum: free_bytes,
        });
    }
    Ok(())
}

fn checked_element_count(shape: &[i64], resource: &'static str) -> Result<usize, SimulationError> {
    shape.iter().try_fold(1_usize, |elements, extent| {
        if *extent <= 0 {
            return Err(SimulationError::InvalidNativeResult {
                reason: format!("{resource} extent must be positive: {shape:?}"),
            });
        }
        let extent = usize::try_from(*extent)
            .map_err(|_| SimulationError::ResourceSizeOverflow { resource })?;
        elements
            .checked_mul(extent)
            .ok_or(SimulationError::ResourceSizeOverflow { resource })
    })
}

fn mode_id(qubit: u32) -> Result<i32, SimulationError> {
    i32::try_from(qubit).map_err(|_| SimulationError::InvalidCircuit {
        reason: format!("qubit {qubit} does not fit the native mode identifier"),
    })
}

fn combine_execution_and_cleanup<T>(
    execution: Result<T, SimulationError>,
    cleanup: Result<(), SimulationError>,
) -> Result<T, SimulationError> {
    match (execution, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(execution), Err(cleanup)) => Err(SimulationError::ExecutionAndCleanupFailed {
            execution: Box::new(execution),
            cleanup: Box::new(cleanup),
        }),
    }
}

fn circuit_with_gates(qubit_count: u32, gates: &[Gate]) -> Circuit {
    let mut circuit = Circuit::new(qubit_count).expect("qualification width should be valid");
    for gate in gates {
        circuit
            .push(*gate)
            .expect("qualification gate should be valid");
    }
    circuit
}

fn maximum_amplitude_error(actual: &[Complex64], expected: &[Complex64]) -> f64 {
    assert_eq!(actual.len(), expected.len());
    actual
        .iter()
        .zip(expected)
        .map(|(actual, expected)| (*actual - expected).norm())
        .fold(0.0_f64, f64::max)
}

fn basis_state(width: usize, index: usize) -> Vec<Complex64> {
    let length = 1_usize
        .checked_shl(u32::try_from(width).expect("small qualification width should fit u32"))
        .expect("small qualification state should fit usize");
    let mut state = vec![Complex64::new(0.0, 0.0); length];
    state[index] = Complex64::new(1.0, 0.0);
    state
}

fn b0_qualification_cases() -> Vec<(&'static str, Circuit, Vec<Complex64>)> {
    let mut cases = b0_basis_order_cases();
    cases.extend(b0_rotation_cases());
    cases
}

fn b0_basis_order_cases() -> Vec<(&'static str, Circuit, Vec<Complex64>)> {
    vec![
        (
            "x-q0",
            circuit_with_gates(2, &[Gate::X { target: 0 }]),
            basis_state(2, 1),
        ),
        (
            "x-q1",
            circuit_with_gates(2, &[Gate::X { target: 1 }]),
            basis_state(2, 2),
        ),
        cnot_case("cnot-0-1-active", 0, 0, 1, 3),
        cnot_case("cnot-0-1-inactive", 1, 0, 1, 2),
        cnot_case("cnot-1-0-active", 1, 1, 0, 3),
        cnot_case("cnot-1-0-inactive", 0, 1, 0, 1),
        (
            "six-qubit-ordering",
            circuit_with_gates(
                6,
                &[
                    Gate::X { target: 0 },
                    Gate::X { target: 1 },
                    Gate::X { target: 3 },
                ],
            ),
            basis_state(6, 11),
        ),
    ]
}

fn cnot_case(
    label: &'static str,
    prepared_qubit: u32,
    control: u32,
    target: u32,
    expected_index: usize,
) -> (&'static str, Circuit, Vec<Complex64>) {
    (
        label,
        circuit_with_gates(
            2,
            &[
                Gate::X {
                    target: prepared_qubit,
                },
                Gate::Cnot { control, target },
            ],
        ),
        basis_state(2, expected_index),
    )
}

fn b0_rotation_cases() -> Vec<(&'static str, Circuit, Vec<Complex64>)> {
    let rotation_angle = 0.731;
    let (rotation_sine, rotation_cosine) = (rotation_angle / 2.0_f64).sin_cos();
    let mut rotation_expected = basis_state(2, 0);
    rotation_expected[0] = Complex64::new(rotation_cosine, 0.0);
    rotation_expected[1] = Complex64::new(0.0, -rotation_sine);

    let phase_angle = 0.913;
    let (phase_sine, phase_cosine) = (phase_angle / 2.0_f64).sin_cos();
    let mut phase_expected = basis_state(2, 0);
    phase_expected[0] = Complex64::new(phase_cosine, 0.0);
    phase_expected[1] = Complex64::new(0.0, -phase_sine);

    vec![
        (
            "asymmetric-rx",
            circuit_with_gates(
                2,
                &[Gate::Rx {
                    theta: rotation_angle,
                    target: 0,
                }],
            ),
            rotation_expected,
        ),
        (
            "complex-phase-interference",
            circuit_with_gates(
                2,
                &[
                    Gate::H { target: 0 },
                    Gate::Rz {
                        theta: phase_angle,
                        target: 0,
                    },
                    Gate::H { target: 0 },
                ],
            ),
            phase_expected,
        ),
    ]
}

fn b1_width_circuit(width: u32) -> Circuit {
    let gates = if width == 2 {
        vec![Gate::X { target: 0 }]
    } else if width == 3 {
        vec![Gate::X { target: 0 }, Gate::X { target: 2 }]
    } else {
        vec![
            Gate::X { target: 0 },
            Gate::X { target: width / 3 },
            Gate::X { target: width - 1 },
        ]
    };
    circuit_with_gates(width, &gates)
}

fn validate_b1_width_result(width: usize, result: &SimulationResult, readout: StateReadout) {
    let expected_target = MpsTarget::new(width, 128).expect("B1 target should be valid");
    let expected_target = convert_layout("expected target", &expected_target.extents)
        .expect("target extents should fit usize");
    assert_eq!(result.report.policy, ExecutionPolicy::base_qualification());
    assert_eq!(result.report.target_extents, expected_target);
    assert!(result.report.maximum_bond <= 128);
    assert_eq!(
        result.report.workspace.requested_maximum_bytes,
        68_719_476_736
    );
    assert!(
        result.report.workspace.native_recommended_bytes
            <= result.report.workspace.requested_maximum_bytes
    );
    assert_eq!(
        result.report.workspace.allocated_bytes,
        result.report.workspace.native_recommended_bytes
    );
    if readout == StateReadout::MetadataOnly {
        assert_eq!(result.amplitudes(), None);
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
#[allow(
    clippy::used_underscore_binding,
    reason = "the Phase 2 library guard is intentionally unused outside this internal fixture"
)]
fn base_profile_a100_qualification() {
    let availability = crate::discover().expect("native libraries should be available");
    let report = availability.report().clone();
    let policy = ExecutionPolicy::bell_regression()
        .validate()
        .expect("Bell policy should be valid");
    let mut session = Session::new(Arc::clone(&availability.libraries), policy)
        .expect("native session should be created");
    let mut circuit = Circuit::new(2).expect("two-qubit fixture should be valid");
    circuit
        .push(Gate::H { target: 0 })
        .expect("Hadamard should be valid");
    circuit
        .push(Gate::Cnot {
            control: 0,
            target: 1,
        })
        .expect("CNOT should be valid");

    let expected = [
        Complex64::new(FRAC_1_SQRT_2, 0.0),
        Complex64::new(0.0, 0.0),
        Complex64::new(0.0, 0.0),
        Complex64::new(FRAC_1_SQRT_2, 0.0),
    ];
    let started = Instant::now();
    let result = match session.simulate(&circuit, StateReadout::FullAmplitudes) {
        Ok(result) => result,
        Err(error) => {
            let elapsed = started.elapsed();
            let cleanup = session.close();
            eprintln!("simulation_error={error}");
            eprintln!("simulation_elapsed_seconds={:.9}", elapsed.as_secs_f64());
            eprintln!("cleanup={cleanup:?}");
            panic!("Base Profile replay failed");
        }
    };
    let elapsed = started.elapsed();
    let actual = result
        .amplitudes()
        .expect("full-amplitude readout should return amplitudes");
    let maximum_error = maximum_amplitude_error(actual, &expected);

    println!(
        "cutensornet_library={}",
        report.cutensornet_library.display()
    );
    println!("cudart_library={}", report.cuda_runtime_library.display());
    println!("cutensornet_version={}", report.cutensornet_version);
    println!(
        "cutensornet_cudart_version={}",
        report.cutensornet_cuda_runtime_version
    );
    println!("cudart_version={}", report.cuda_runtime_version);
    println!("cuda_driver_version={}", report.cuda_driver_version);
    println!("device_ordinal={}", session.device_ordinal());
    println!("circuit={circuit:?}");
    println!("basis_index=q0+2*q1");
    println!("matrix_storage=textbook-row-major-null-native-strides");
    println!("expected={expected:?}");
    println!("actual={actual:?}");
    println!("maximum_amplitude_error={maximum_error:.17e}");
    println!("simulation_elapsed_seconds={:.9}", elapsed.as_secs_f64());
    let cleanup = session.close();
    println!("cleanup={cleanup:?}");
    cleanup.expect("native session cleanup should succeed");
    assert!(
        maximum_error <= 1.0e-12,
        "maximum error was {maximum_error}"
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
#[allow(
    clippy::used_underscore_binding,
    reason = "the Phase 2 library guard is intentionally unused outside this internal fixture"
)]
fn b0_a100_ordering_and_gate_qualification() {
    let availability = crate::discover().expect("native libraries should be available");
    let policy = ExecutionPolicy::bell_regression()
        .validate()
        .expect("B0 policy should be valid");
    let mut session = Session::new(Arc::clone(&availability.libraries), policy)
        .expect("native session should be created");
    for (label, circuit, expected) in b0_qualification_cases() {
        let started = Instant::now();
        let result = session
            .simulate(&circuit, StateReadout::FullAmplitudes)
            .unwrap_or_else(|error| panic!("{label} failed: {error}"));
        let elapsed = started.elapsed();
        let error = maximum_amplitude_error(
            result
                .amplitudes()
                .expect("full-amplitude readout should return amplitudes"),
            &expected,
        );
        println!("case={label}");
        println!("circuit={circuit:?}");
        println!("maximum_amplitude_error={error:.17e}");
        println!("simulation_elapsed_seconds={:.9}", elapsed.as_secs_f64());
        assert!(error <= 1.0e-12, "{label} maximum error was {error}");
    }

    println!("ordering_qdk_semantic_q0_to_q5=[1,1,0,1,0,0]");
    println!("ordering_nonzero_dense_amplitude_index=11");
    println!("ordering_conventional_binary_q5_to_q0=001011");
    let cleanup = session.close();
    println!("cleanup={cleanup:?}");
    cleanup.expect("native session cleanup should succeed");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
#[allow(
    clippy::used_underscore_binding,
    reason = "the Phase 2 library guard is intentionally unused outside this internal fixture"
)]
fn b1_a100_width_qualification() {
    let availability = crate::discover().expect("native libraries should be available");
    let policy = ExecutionPolicy::base_qualification();
    let mut session = Session::new(Arc::clone(&availability.libraries), policy)
        .expect("native session should be created");

    for width in [2_u32, 3, 63, 64, 128] {
        let circuit = b1_width_circuit(width);
        let readout = if width <= 3 {
            StateReadout::FullAmplitudes
        } else {
            StateReadout::MetadataOnly
        };
        let started = Instant::now();
        let result = session
            .simulate(&circuit, readout)
            .unwrap_or_else(|error| panic!("width {width} failed: {error}"));
        let elapsed = started.elapsed();
        let width_usize = usize::try_from(width).expect("width should fit usize");
        validate_b1_width_result(width_usize, &result, readout);

        if width <= 3 {
            let expected_index = if width == 2 { 1 } else { 5 };
            let expected = basis_state(width_usize, expected_index);
            let error = maximum_amplitude_error(
                result
                    .amplitudes()
                    .expect("small-width readout should return amplitudes"),
                &expected,
            );
            println!("maximum_amplitude_error={error:.17e}");
            assert!(error <= 1.0e-12, "width {width} error was {error}");
        }

        println!("width={width}");
        println!("readout={readout:?}");
        println!("policy={:?}", result.report.policy);
        println!("target_extents={:?}", result.report.target_extents);
        println!("realized_extents={:?}", result.report.realized_extents);
        println!("maximum_bond={}", result.report.maximum_bond);
        println!("workspace={:?}", result.report.workspace);
        println!("simulation_elapsed_seconds={:.9}", elapsed.as_secs_f64());
    }

    let cleanup = session.close();
    println!("cleanup={cleanup:?}");
    cleanup.expect("native session cleanup should succeed");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
#[allow(
    clippy::used_underscore_binding,
    reason = "the Phase 2 library guard is intentionally unused outside this internal fixture"
)]
fn b2_a100_trotter_query_qualification() {
    let width = std::env::var("QDK_CUTENSORNET_B2_WIDTH")
        .expect("QDK_CUTENSORNET_B2_WIDTH must be set")
        .parse::<u32>()
        .expect("B2 width must be an integer");
    let expected = match width {
        12 => 4.332_869_154_633,
        16 => 6.347_012_657_087,
        20 => 8.361_156_159_877,
        _ => panic!("unsupported B2 qualification width {width}"),
    };
    run_trotter_query_qualification(
        "B2",
        width,
        8,
        expected,
        1.0e-9,
        ExecutionPolicy::base_qualification(),
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
#[allow(
    clippy::used_underscore_binding,
    reason = "the Phase 2 library guard is intentionally unused outside this internal fixture"
)]
fn b3_a100_matched_bond_qualification() {
    run_trotter_query_qualification(
        "B3",
        128,
        16,
        60.319_518_034_172_646,
        1.0e-10,
        ExecutionPolicy::b3_matched_bond_qualification(),
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
#[allow(
    clippy::used_underscore_binding,
    reason = "the Phase 2 library guard is intentionally unused outside this internal fixture"
)]
fn b4_a100_convergence_qualification() {
    let bond_cap = std::env::var("QDK_CUTENSORNET_B4_CAP")
        .expect("QDK_CUTENSORNET_B4_CAP must be set")
        .parse::<i64>()
        .expect("B4 bond cap must be an integer");
    let expected = match bond_cap {
        32 => 122.350_509_319_616,
        64 => 122.350_509_321_997,
        128 | 256 => 122.350_509_322_001,
        _ => panic!("unsupported B4 qualification bond cap {bond_cap}"),
    };
    run_trotter_query_qualification(
        "B4",
        256,
        16,
        expected,
        1.0e-9,
        ExecutionPolicy::b4_convergence_qualification(bond_cap),
    );
}

#[allow(
    clippy::too_many_lines,
    clippy::used_underscore_binding,
    reason = "the hardware qualification emits one complete traceable cell record and retains the Phase 2 library guard"
)]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_trotter_query_qualification(
    stage: &str,
    width: u32,
    steps: u32,
    expected: f64,
    relative_error_limit: f64,
    policy: ExecutionPolicy,
) {
    let total_started = Instant::now();

    let discovery_started = Instant::now();
    let availability = crate::discover().expect("native libraries should be available");
    let discovery_seconds = discovery_started.elapsed().as_secs_f64();
    let session_started = Instant::now();
    let mut session = Session::new(Arc::clone(&availability.libraries), policy)
        .expect("native session should be created");
    let session_creation_seconds = session_started.elapsed().as_secs_f64();
    let fixture_started = Instant::now();
    let circuit = Circuit::trotter_domain_wall(width, steps, 0.3)
        .unwrap_or_else(|error| panic!("{stage} Trotter fixture failed: {error}"));
    let query =
        AdjacentZQuery::new(width).unwrap_or_else(|error| panic!("{stage} Query failed: {error}"));
    let canonical_description = circuit.canonical_description();
    let fixture_construction_seconds = fixture_started.elapsed().as_secs_f64();

    let execution_started = Instant::now();
    let result = session
        .simulate_and_query(&circuit, &query)
        .unwrap_or_else(|error| panic!("{stage} width {width} failed: {error}"));
    let execution_seconds = execution_started.elapsed().as_secs_f64();
    let host_validation_started = Instant::now();
    let normalized = result.query.normalized_expectation;
    let relative_error = ((normalized.re - expected) / expected).abs();
    assert!(normalized.im.abs() <= 1.0e-12);
    assert!(
        relative_error <= relative_error_limit,
        "relative error was {relative_error}"
    );
    let host_validation_seconds = host_validation_started.elapsed().as_secs_f64();
    let session_cleanup_started = Instant::now();
    let cleanup = session.close();
    let session_cleanup_seconds = session_cleanup_started.elapsed().as_secs_f64();
    cleanup.expect("native session cleanup should succeed");

    println!("stage={stage}");
    println!("width={width}");
    println!("steps={steps}");
    println!("operation_count={}", circuit.gates().len());
    println!("canonical_circuit={canonical_description}");
    println!("query_term_count={}", query.terms.len());
    println!("expected_query={expected:.15}");
    println!("raw_expectation={:?}", result.query.raw_expectation);
    println!("squared_norm={:?}", result.query.squared_norm);
    println!("normalized_expectation={normalized:?}");
    println!("relative_error={relative_error:.17e}");
    println!("relative_error_limit={relative_error_limit:.17e}");
    println!("hyper_samples={}", result.query.hyper_samples);
    println!("policy={:?}", result.state.report.policy);
    println!("target_extents={:?}", result.state.report.target_extents);
    println!(
        "realized_extents={:?}",
        result.state.report.realized_extents
    );
    println!("maximum_bond={}", result.state.report.maximum_bond);
    println!("state_workspace={:?}", result.state.report.workspace);
    println!("query_workspace={:?}", result.query.workspace);
    println!("state_timings={:?}", result.state.report.timings);
    println!("query_timings={:?}", result.query.timings);
    println!("discovery_seconds={discovery_seconds:.9}");
    println!("session_creation_seconds={session_creation_seconds:.9}");
    println!("fixture_construction_seconds={fixture_construction_seconds:.9}");
    println!("execution_seconds={execution_seconds:.9}");
    println!(
        "through_query_completion_seconds={:.9}",
        result.through_query_completion_seconds
    );
    println!("host_validation_seconds={host_validation_seconds:.9}");
    println!(
        "replay_cleanup_seconds={:.9}",
        result.replay_cleanup_seconds
    );
    println!("session_cleanup_seconds={session_cleanup_seconds:.9}");
    println!(
        "total_process_test_seconds={:.9}",
        total_started.elapsed().as_secs_f64()
    );
    println!("cleanup=Ok(())");
}

fn apply_sparse_circuit(simulator: &mut qdk_simulators::SparseStateSim, circuit: &Circuit) {
    for gate in circuit.gates() {
        match *gate {
            Gate::X { target } => simulator.x(target as usize),
            Gate::H { target } => simulator.h(target as usize),
            Gate::Rx { theta, target } => simulator.rx(theta, target as usize),
            Gate::Rz { theta, target } => simulator.rz(theta, target as usize),
            Gate::Cnot { control, target } => {
                simulator.mcx(&[control as usize], target as usize);
            }
        }
    }
}

fn sparse_dense_state(
    simulator: &mut qdk_simulators::SparseStateSim,
    expected_width: usize,
) -> Vec<Complex64> {
    let (state, width) = simulator.get_state();
    assert_eq!(width, expected_width);
    let mut dense = vec![Complex64::new(0.0, 0.0); 1 << width];
    for (basis, amplitude) in state {
        let digits = basis.to_u64_digits();
        let index = match digits.as_slice() {
            [] => 0,
            [low] => usize::try_from(*low).expect("small basis index should fit usize"),
            _ => panic!("small basis index should fit one u64 digit"),
        };
        dense[index] = amplitude;
    }
    dense
}

fn maximum_global_phase_error(actual: &[Complex64], expected: &[Complex64]) -> f64 {
    assert_eq!(actual.len(), expected.len());
    let Some((actual_reference, expected_reference)) = actual
        .iter()
        .zip(expected)
        .find(|(actual, expected)| actual.norm() > 1.0e-14 && expected.norm() > 1.0e-14)
    else {
        return maximum_amplitude_error(actual, expected);
    };
    let phase = *actual_reference / *expected_reference;
    let phase = phase / phase.norm();
    actual
        .iter()
        .zip(expected)
        .map(|(actual, expected)| (*actual - phase * *expected).norm())
        .fold(0.0_f64, f64::max)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires a CUDA 12.9/cuTensorNet 2.13 GPU environment"]
#[allow(
    clippy::too_many_lines,
    clippy::used_underscore_binding,
    reason = "the private qualification keeps one readable end-to-end evidence transaction"
)]
fn b5_branch_capture_and_continuation_matches_qdk_sparse_oracle() {
    use branch::{BranchRequest, SelectedBranch};
    use qdk_simulators::SparseStateSim;

    let theta = 0.7;
    let initial = circuit_with_gates(
        3,
        &[
            Gate::Rx { theta, target: 0 },
            Gate::Cnot {
                control: 0,
                target: 1,
            },
            Gate::Cnot {
                control: 1,
                target: 2,
            },
        ],
    );
    let continuation = circuit_with_gates(
        3,
        &[
            Gate::H { target: 2 },
            Gate::Cnot {
                control: 2,
                target: 1,
            },
            Gate::Rz {
                theta: 0.41,
                target: 0,
            },
            Gate::Cnot {
                control: 1,
                target: 0,
            },
        ],
    );
    let query = AdjacentZQuery::new(3).expect("three-qubit Query should be valid");
    let availability = crate::discover().expect("native libraries should be available");
    let policy = ExecutionPolicy::bell_regression();

    let selected = match std::env::var("QDK_CUTENSORNET_B5_BRANCH").as_deref() {
        Ok("zero") => SelectedBranch::Zero,
        Ok("one") => SelectedBranch::One,
        Ok(value) => panic!("unsupported QDK_CUTENSORNET_B5_BRANCH value: {value}"),
        Err(error) => panic!("QDK_CUTENSORNET_B5_BRANCH must be set: {error}"),
    };
    {
        let mut oracle = SparseStateSim::new(None);
        for expected in 0..3 {
            assert_eq!(oracle.allocate(), expected);
        }
        apply_sparse_circuit(&mut oracle, &initial);
        let expected_initial = sparse_dense_state(&mut oracle, 3);
        let expected_norm = expected_initial
            .iter()
            .map(Complex64::norm_sqr)
            .sum::<f64>();
        let expected_q1 = expected_initial
            .iter()
            .enumerate()
            .filter(|(basis, _)| basis & 1 != 0)
            .map(|(_, amplitude)| amplitude.norm_sqr())
            .sum::<f64>();
        let expected_q0 = expected_norm - expected_q1;
        let expected_q0_analytic = (theta / 2.0_f64).cos().powi(2);
        let expected_q1_analytic = (theta / 2.0_f64).sin().powi(2);
        assert!((expected_q0 - expected_q0_analytic).abs() <= 1.0e-12);
        assert!((expected_q1 - expected_q1_analytic).abs() <= 1.0e-12);
        assert!(expected_q0 > expected_q1 && expected_q1 > 0.0);
        let forced_probability = oracle.force_collapse(selected == SelectedBranch::One, 0);
        let expected_post_projection = sparse_dense_state(&mut oracle, 3);
        assert!(
            (expected_post_projection
                .iter()
                .map(Complex64::norm_sqr)
                .sum::<f64>()
                - 1.0)
                .abs()
                <= 1.0e-12
        );
        apply_sparse_circuit(&mut oracle, &continuation);
        let expected_final = sparse_dense_state(&mut oracle, 3);
        let expected_query = (0..2)
            .map(|left| 1.0 - 2.0 * oracle.joint_probability(&[left, left + 1]))
            .sum::<f64>();

        let mut session = Session::new(Arc::clone(&availability.libraries), policy)
            .expect("native session should be created");
        let result = session
            .simulate_with_branch(
                &initial,
                BranchRequest { mode: 0, selected },
                &continuation,
                &query,
            )
            .expect("branch replay should succeed");
        session
            .close()
            .expect("native session cleanup should succeed");

        let tolerance = 1.0e-12;
        assert!((result.report.masses.norm - expected_norm).abs() <= tolerance);
        assert!((result.report.masses.q0 - expected_q0).abs() <= tolerance);
        assert!((result.report.masses.q1 - expected_q1).abs() <= tolerance);
        assert!((result.report.masses.p0 + result.report.masses.p1 - 1.0).abs() <= tolerance);
        assert!((result.report.probability - forced_probability).abs() <= tolerance);
        assert!((result.report.log_probability - forced_probability.ln()).abs() <= tolerance);
        assert!(
            maximum_global_phase_error(
                result
                    .initial_state
                    .amplitudes()
                    .expect("initial amplitudes should be retained"),
                &expected_initial,
            ) <= tolerance
        );
        assert!(
            maximum_global_phase_error(
                result
                    .post_projection_state
                    .amplitudes()
                    .expect("post-projection amplitudes should be retained"),
                &expected_post_projection,
            ) <= tolerance
        );
        assert!(
            maximum_global_phase_error(
                result
                    .continuation_state
                    .amplitudes()
                    .expect("continuation amplitudes should be retained"),
                &expected_final,
            ) <= tolerance
        );
        assert!((result.query.squared_norm.re - 1.0).abs() <= tolerance);
        assert!((result.query.raw_expectation.re - expected_query).abs() <= tolerance);
        assert!((result.query.normalized_expectation.re - expected_query).abs() <= tolerance);

        let label = match selected {
            SelectedBranch::Zero => "zero",
            SelectedBranch::One => "one",
        };
        let initial_error = maximum_global_phase_error(
            result
                .initial_state
                .amplitudes()
                .expect("initial amplitudes should be retained"),
            &expected_initial,
        );
        let projection_error = maximum_global_phase_error(
            result
                .post_projection_state
                .amplitudes()
                .expect("post-projection amplitudes should be retained"),
            &expected_post_projection,
        );
        let continuation_error = maximum_global_phase_error(
            result
                .continuation_state
                .amplitudes()
                .expect("continuation amplitudes should be retained"),
            &expected_final,
        );
        println!("b5_branch={label}");
        println!("b5_expected_norm={expected_norm:.17e}");
        println!("b5_native_norm={:.17e}", result.report.masses.norm);
        println!("b5_expected_q0={expected_q0:.17e}");
        println!("b5_native_q0={:.17e}", result.report.masses.q0);
        println!("b5_expected_q1={expected_q1:.17e}");
        println!("b5_native_q1={:.17e}", result.report.masses.q1);
        println!("b5_native_p0={:.17e}", result.report.masses.p0);
        println!("b5_native_p1={:.17e}", result.report.masses.p1);
        println!("b5_selected_probability={:.17e}", result.report.probability);
        println!(
            "b5_selected_log_probability={:.17e}",
            result.report.log_probability
        );
        println!("b5_initial_max_phase_error={initial_error:.17e}");
        println!("b5_projection_max_phase_error={projection_error:.17e}");
        println!("b5_continuation_max_phase_error={continuation_error:.17e}");
        println!("b5_expected_query={expected_query:.17e}");
        println!(
            "b5_native_raw_query={:.17e}",
            result.query.raw_expectation.re
        );
        println!(
            "b5_native_normalized_query={:.17e}",
            result.query.normalized_expectation.re
        );
        println!("b5_query_norm={:.17e}", result.query.squared_norm.re);
        println!(
            "b5_initial_execution_seconds={:.17e}",
            result.report.timings.initial_execution_seconds
        );
        println!(
            "b5_first_barrier_synchronization_seconds={:.17e}",
            result.report.timings.first_barrier_synchronization_seconds
        );
        println!(
            "b5_first_capture_seconds={:.17e}",
            result.report.timings.first_capture_seconds
        );
        println!(
            "b5_mass_computation_seconds={:.17e}",
            result.report.timings.mass_computation_seconds
        );
        println!(
            "b5_mass_synchronization_seconds={:.17e}",
            result.report.timings.mass_synchronization_seconds
        );
        println!(
            "b5_projection_registration_seconds={:.17e}",
            result.report.timings.projection_registration_seconds
        );
        println!(
            "b5_projection_preparation_compute_seconds={:.17e}",
            result.report.timings.projection_preparation_compute_seconds
        );
        println!(
            "b5_projection_barrier_synchronization_seconds={:.17e}",
            result
                .report
                .timings
                .projection_barrier_synchronization_seconds
        );
        println!(
            "b5_projection_capture_seconds={:.17e}",
            result.report.timings.projection_capture_seconds
        );
        println!(
            "b5_continuation_registration_seconds={:.17e}",
            result.report.timings.continuation_registration_seconds
        );
        println!(
            "b5_continuation_preparation_compute_seconds={:.17e}",
            result
                .report
                .timings
                .continuation_preparation_compute_seconds
        );
        println!(
            "b5_continuation_barrier_synchronization_seconds={:.17e}",
            result
                .report
                .timings
                .continuation_barrier_synchronization_seconds
        );
        println!(
            "b5_branch_query_seconds={:.17e}",
            result.report.timings.query_seconds
        );
        println!(
            "b5_branch_cleanup_seconds={:.17e}",
            result.report.timings.cleanup_seconds
        );
        println!(
            "b5_branch_total_wall_seconds={:.17e}",
            result.report.timings.total_wall_seconds
        );
        println!(
            "b5_query_construction_seconds={:.17e}",
            result.query.timings.construction_seconds
        );
        println!(
            "b5_query_preparation_path_planning_seconds={:.17e}",
            result.query.timings.preparation_path_planning_seconds
        );
        println!(
            "b5_query_workspace_allocation_attachment_seconds={:.17e}",
            result.query.timings.workspace_allocation_attachment_seconds
        );
        println!(
            "b5_query_compute_call_seconds={:.17e}",
            result.query.timings.compute_call_seconds
        );
        println!(
            "b5_query_synchronization_seconds={:.17e}",
            result.query.timings.synchronization_seconds
        );
        println!(
            "b5_query_output_validation_seconds={:.17e}",
            result.query.timings.output_validation_seconds
        );
        println!(
            "b5_initial_workspace={:?}",
            result.initial_state.report.workspace
        );
        println!(
            "b5_projection_workspace={:?}",
            result.post_projection_state.report.workspace
        );
        println!(
            "b5_continuation_workspace={:?}",
            result.continuation_state.report.workspace
        );
        println!("b5_query_workspace={:?}", result.query.workspace);
        println!("b5_session_cleanup=ok");
    }
}
