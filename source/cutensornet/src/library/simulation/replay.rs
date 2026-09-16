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
use std::{f64::consts::FRAC_1_SQRT_2, mem::size_of, time::Instant};
use tensornet::{Mps, MpsError};

#[cfg(test)]
#[path = "replay/tests.rs"]
mod tests;

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
#[path = "replay/qualification.rs"]
mod qualification;

/// The MPS shape a simulation asks the library to produce, and the FFI
/// scaffolding that shape needs.
///
/// The shape itself is a [`Mps`]: extents only, since a target describes a
/// capacity rather than a buffer and so has no layout. The `i64` copy and the
/// pointers into it exist purely because the library takes extents as an array
/// of pointers, and they must outlive the call that reads them.
pub(crate) struct MpsTarget {
    shape: Mps,
    extents: Vec<Box<[i64]>>,
    extent_pointers: Box<[*const i64]>,
}

impl MpsTarget {
    fn new(qubit_count: usize, bond_cap: i64) -> Result<Self, SimulationError> {
        // cuTensorNet requires at least two sites for MPS finalization
        // (bindings/v2_13.rs:338). This crate implements only MPS and has one
        // unconditional finalize_mps call below, so it cannot represent one site.
        // `Mps` itself accepts a lone site: the limit is this backend's, not the
        // abstraction's.
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
        let shape = Mps::new(convert_layout("target extent", &extents)?).map_err(|error| {
            match error {
                // Preserves the variant this path raised before the chain was
                // a type: a bond cap large enough to overflow is a resource
                // limit, not a malformed request.
                MpsError::ElementCountOverflow { .. } => SimulationError::ResourceSizeOverflow {
                    resource: "MPS output",
                },
                other => SimulationError::InvalidCircuit {
                    reason: format!("target MPS is not a valid chain: {other}"),
                },
            }
        })?;
        let extent_pointers = extents
            .iter()
            .map(|shape| shape.as_ptr())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(Self {
            shape,
            extents,
            extent_pointers,
        })
    }

    /// The chain this target asks for.
    pub(crate) fn shape(&self) -> &Mps {
        &self.shape
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
        let output_elements = self.target.shape().element_counts();
        let mut outputs = Vec::with_capacity(output_elements.len());
        for elements in &output_elements {
            outputs.push(self.allocate_complex(*elements, "MPS output")?);
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
        // The library reports the shape it produced and, separately, the
        // strides it used to write it. Only the shape is part of the state;
        // the strides describe this one readout's buffers, so they stay beside
        // the buffers below rather than travelling in the `Mps`.
        let realized = Mps::new(convert_layout("extent", &metadata.extents)?).map_err(|error| {
            SimulationError::InvalidNativeResult {
                reason: format!("realized MPS is not a valid chain: {error}"),
            }
        })?;
        let transferred = if readout == StateReadout::FullAmplitudes {
            let mut host_outputs = Vec::with_capacity(outputs.len());
            for (output, elements) in outputs.iter().zip(&output_elements) {
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
        validate_realized_chain(&realized, self.target.shape())?;
        let maximum_bond = realized.max_bond();
        let amplitudes = transferred
            .map(|(host_outputs, strides)| contract_open_mps(&realized, &host_outputs, &strides))
            .transpose()?;
        timings.host_validation_seconds = phase_started.elapsed().as_secs_f64();
        Ok(SimulationResult::new(
            amplitudes,
            ExecutionReport {
                policy: self.policy,
                target_extents: self.target.shape().sites().to_vec(),
                realized_extents: realized.sites().to_vec(),
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
        Gate::Rzz { theta, q1, q2 } => {
            let (sine, cosine) = (theta / 2.0).sin_cos();
            let zero = Complex64Abi::new(0.0, 0.0);
            (
                vec![mode_id(q1)?, mode_id(q2)?],
                vec![
                    Complex64Abi::new(cosine, -sine),
                    zero,
                    zero,
                    zero,
                    zero,
                    Complex64Abi::new(cosine, sine),
                    zero,
                    zero,
                    zero,
                    zero,
                    Complex64Abi::new(cosine, sine),
                    zero,
                    zero,
                    zero,
                    zero,
                    Complex64Abi::new(cosine, -sine),
                ],
            )
        }
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

/// Checks a realized chain against the shape this backend asked for.
///
/// `Mps` has already established that the chain is well formed, so what is
/// left are two demands this backend makes on its own behalf. The library must
/// not have exceeded the capacity it was given, and every site must be a
/// qubit, because the dense readout indexes the computational basis in bits.
/// Neither belongs to matrix product states in general.
fn validate_realized_chain(realized: &Mps, target: &Mps) -> Result<(), SimulationError> {
    if !realized.fits_within(target) {
        return Err(invalid_native_extents(
            "realized extent exceeds target capacity",
        ));
    }
    if (0..realized.site_count()).any(|site| realized.physical_dim(site) != Some(2)) {
        return Err(invalid_native_extents(
            "realized physical extent is not two",
        ));
    }
    Ok(())
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
