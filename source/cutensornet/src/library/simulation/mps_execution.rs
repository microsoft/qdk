//! cuTensorNet State/MPS execution of resolved circuit operations.
//!
//! Owns state, workspaces and retained device buffers beneath an MPS session.
//! Program control and shot/output orchestration belong to the shared execution
//! framework; arbitrary-network path optimization belongs to `contraction`.

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use super::memory_workspace::MemoryWorkspaceApi;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use super::query::BaseQueryResult;
use super::{
    Circuit, Gate, OpaqueHandle, ProjectedCircuit, ProjectedOperation, SimulationError,
    SimulationResult, Stream, branch,
    circuit::{
        ExecutionReport, StatePhaseTimings, StateReadout, WorkspaceReport, contract_open_mps,
    },
    error::combine_execution_and_cleanup,
    ffi::Complex64Abi,
    policy::ExecutionPolicy,
    query::{
        AdjacentZQuery, B2_EXPECTATION_HYPER_SAMPLES, QueryPhaseTimings, QueryResult,
        normalize_expectation,
    },
    sampler::{FullBitstringSamples, PreparedSampler, SamplerApi, SamplerContext, SamplingRequest},
};
use crate::execution::MpsCost;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use crate::library::MpsSession;
use num_complex::Complex64;
use qdk_simulators::{
    QubitID,
    execution::{OperatorMatrix, Pauli, PauliSum, basis_operator, unitary_matrix},
};
use std::{mem::size_of, time::Instant};
use tensornet::{Mps, MpsError};

#[cfg(test)]
#[path = "mps_execution/tests.rs"]
mod tests;

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
#[path = "mps_execution/qualification.rs"]
mod qualification;

/// Expectation values of the observables, in order, and the MPS Cost when
/// the state was an MPS.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StateQueryResult {
    pub(crate) expectations: Vec<Complex64>,
    pub(crate) cost: Option<MpsCost>,
}

/// One Query's device tensors, one per Pauli kind, uploaded on first use and
/// shared by every factor of that kind.
#[derive(Default)]
struct PauliTensors {
    x: Option<OpaqueHandle>,
    y: Option<OpaqueHandle>,
    z: Option<OpaqueHandle>,
}

impl PauliTensors {
    const fn slot(&mut self, pauli: Pauli) -> &mut Option<OpaqueHandle> {
        match pauli {
            Pauli::X => &mut self.x,
            Pauli::Y => &mut self.y,
            Pauli::Z => &mut self.z,
        }
    }
}

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
    /// Kept singular values are not rescaled after truncation, so the norm
    /// is the weight actually retained; that norm is the Probability.
    SvdNormalizationNone,
    SvdAlgorithmGesvd,
    MpsGaugeSimple,
}

/// Injected cuTensorNet State/MPS operations, not a backend-neutral execution API.
///
/// Callers keep the context, stream and child handles live and device-compatible.
/// Registered tensor buffers must outlive the native objects retaining them;
/// asynchronous results require stream synchronization before host access.
pub(crate) trait MpsExecutionApi: super::memory_workspace::MemoryWorkspaceApi {
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
    ) -> Result<i64, SimulationError> {
        use super::memory_workspace::{MemorySpace, WorkspaceKind, WorkspacePreference};
        self.workspace_memory_size(
            handle,
            workspace,
            WorkspacePreference::Recommended,
            MemorySpace::Device,
            WorkspaceKind::Scratch,
        )
    }
    fn set_workspace(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        allocation: OpaqueHandle,
        bytes: i64,
    ) -> Result<(), SimulationError> {
        use super::memory_workspace::{MemorySpace, WorkspaceKind};
        self.set_workspace_memory(
            handle,
            workspace,
            MemorySpace::Device,
            WorkspaceKind::Scratch,
            Some(allocation),
            bytes,
        )
    }
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
impl MpsSession {
    /// See [`MpsExecution::evaluate_observables`].
    pub(crate) fn evaluate_observables(
        &mut self,
        circuit: &Circuit,
        observables: &[PauliSum],
        mps: bool,
    ) -> Result<StateQueryResult, SimulationError> {
        let mut mps_execution = MpsExecution::new(
            self.api(),
            self.handle(),
            self.stream(),
            circuit,
            self.policy(),
        )?;
        let execution = mps_execution.evaluate_observables(circuit, observables, mps);
        let cleanup = mps_execution.close();
        combine_execution_and_cleanup(execution, cleanup)
    }

    /// See [`MpsExecution::evaluate_probability`].
    pub(crate) fn evaluate_probability(
        &mut self,
        program: &ProjectedCircuit,
    ) -> Result<ProbabilityResult, SimulationError> {
        let mut mps_execution = MpsExecution::new(
            self.api(),
            self.handle(),
            self.stream(),
            program,
            self.policy(),
        )?;
        let execution = mps_execution.evaluate_probability(program);
        let cleanup = mps_execution.close();
        combine_execution_and_cleanup(execution, cleanup)
    }

    pub(crate) fn sample(
        &mut self,
        circuit: &Circuit,
        sampled_qubits: &[QubitID],
        request: SamplingRequest,
    ) -> Result<Box<[i64]>, SimulationError> {
        let mut mps_execution = MpsExecution::new(
            self.api(),
            self.handle(),
            self.stream(),
            circuit,
            self.policy(),
        )?;
        let execution = mps_execution.sample_qubits(circuit, sampled_qubits, request);
        let cleanup = mps_execution.close();
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
        let mut mps_execution = MpsExecution::new(
            self.api(),
            self.handle(),
            self.stream(),
            initial_circuit,
            self.policy(),
        )?;
        let execution = mps_execution.execute_branch(
            initial_circuit,
            branch_request,
            continuation_circuit,
            query,
        );
        let cleanup_started = Instant::now();
        let cleanup = mps_execution.close();
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
        let mut mps_execution = MpsExecution::new(
            self.api(),
            self.handle(),
            self.stream(),
            circuit,
            self.policy(),
        )?;
        let execution = mps_execution.execute(circuit, readout);
        let cleanup = mps_execution.close();
        let mut result = combine_execution_and_cleanup(execution, cleanup)?;
        let (free_after_cleanup_bytes, _) = mps_execution.api.memory_info()?;
        result.report.workspace.free_after_cleanup_bytes = free_after_cleanup_bytes;
        Ok(result)
    }

    fn simulate_and_query(
        &mut self,
        circuit: &Circuit,
        query: &AdjacentZQuery,
    ) -> Result<BaseQueryResult, SimulationError> {
        let through_query_started = Instant::now();
        let mut mps_execution = MpsExecution::new(
            self.api(),
            self.handle(),
            self.stream(),
            circuit,
            self.policy(),
        )?;
        let execution = (|| {
            let state = mps_execution.execute(circuit, StateReadout::MetadataOnly)?;
            let query = mps_execution.execute_query(query)?;
            Ok((state, query))
        })();
        let through_query_completion_seconds = through_query_started.elapsed().as_secs_f64();
        let cleanup_started = Instant::now();
        let cleanup = mps_execution.close();
        let replay_cleanup_seconds = cleanup_started.elapsed().as_secs_f64();
        let (mut state, mut query) = combine_execution_and_cleanup(execution, cleanup)?;
        let (free_after_cleanup_bytes, _) = mps_execution.api.memory_info()?;
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

/// Unnormalized probability of one fixed-outcome path, and the MPS Cost of
/// computing it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProbabilityResult {
    /// ⟨ψ̃|ψ̃⟩ of the truncated MPS, never renormalized.
    pub(crate) probability: f64,
    pub(crate) cost: MpsCost,
}

/// A program an [`MpsExecution`] evolves, and the only source of its width.
///
/// The execution is created from the program it will run, so the state has
/// exactly the program's qubits; no width is ever passed on its own.
trait MpsProgram {
    fn qubit_count(&self) -> u32;
}

impl MpsProgram for Circuit {
    fn qubit_count(&self) -> u32 {
        Circuit::qubit_count(self)
    }
}

impl MpsProgram for ProjectedCircuit {
    fn qubit_count(&self) -> u32 {
        ProjectedCircuit::qubit_count(self)
    }
}

/// One GPU-resident MPS state and the resources for its evolution and readout.
///
/// Borrows the API; the caller must keep the parent context and stream alive
/// until close. Owns child descriptors and all retained device allocations.
/// Explicit close attempts synchronization before releasing children and buffers
/// and reports failures; Drop is the non-panicking fallback. This is not a QIR
/// interpreter.
struct MpsExecution<'api, Api: MpsExecutionApi + ?Sized> {
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

/// One single-site expectation and what computing it took.
struct ProjectorExpectation {
    value: Complex64,
    squared_norm: Complex64,
    synchronization_seconds: f64,
    workspace_bytes: usize,
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

impl<'api, Api: MpsExecutionApi + ?Sized> MpsExecution<'api, Api> {
    /// Creates the |0…0⟩ state of `program`'s qubits. Pass the same program
    /// to the run method; operators outside the state are rejected there.
    fn new(
        api: &'api Api,
        handle: OpaqueHandle,
        stream: Stream,
        program: &impl MpsProgram,
        policy: ExecutionPolicy,
    ) -> Result<Self, SimulationError> {
        let policy = policy.validate()?;
        let qubit_count = usize::try_from(program.qubit_count()).map_err(|_| {
            SimulationError::ResourceSizeOverflow {
                resource: "state mode count",
            }
        })?;
        let target = MpsTarget::new(qubit_count, policy.bond_cap)?;
        let state_extents = vec![2; qubit_count].into_boxed_slice();
        let state = api.create_state(handle, state_extents.as_ref())?;
        let mut mps_execution = Self {
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
            Ok(workspace) => mps_execution.workspace = Some(workspace),
            Err(error) => {
                return combine_execution_and_cleanup(Err(error), mps_execution.close());
            }
        }
        Ok(mps_execution)
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

        // The continuation is registered only after the projection, so a
        // mismatch is rejected here, before any native work.
        self.check_width(continuation_circuit)?;
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
        self.register_circuit(continuation_circuit, &mut continuation_timings)?;
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
        self.register_circuit(circuit, timings)?;
        self.finalize(timings)
    }

    /// Finalizes the registered operators as a bond-capped MPS and applies
    /// the truncation configuration.
    fn finalize(&mut self, timings: &mut StatePhaseTimings) -> Result<(), SimulationError> {
        let state = self.state();
        let phase_started = Instant::now();
        self.api.finalize_mps(self.handle, state, &self.target)?;
        self.configure(state)?;
        timings.finalization_configuration_seconds = phase_started.elapsed().as_secs_f64();
        Ok(())
    }

    /// Registers `circuit`'s gates in order, after checking that it has the
    /// state's width.
    fn register_circuit(
        &mut self,
        circuit: &Circuit,
        timings: &mut StatePhaseTimings,
    ) -> Result<(), SimulationError> {
        self.check_width(circuit)?;
        for gate in circuit.gates() {
            self.register_gate(*gate, timings)?;
        }
        Ok(())
    }

    /// Rejects a program whose width differs from the state's, before any
    /// of its operators reaches the state.
    fn check_width(&self, program: &impl MpsProgram) -> Result<(), SimulationError> {
        if usize::try_from(program.qubit_count()).ok() == Some(self.state_extents.len()) {
            Ok(())
        } else {
            Err(SimulationError::InvalidCircuit {
                reason: format!(
                    "a {}-qubit program does not match the {}-qubit native state",
                    program.qubit_count(),
                    self.state_extents.len()
                ),
            })
        }
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
        self.register_operator(operator, true, timings)
    }

    /// Evaluates each observable on the circuit's state, in order.
    ///
    /// With `mps`, the state is finalized as a bond-capped MPS and computed
    /// once, which yields the Cost; every observable then reads that truncated
    /// state. Without it, the gates stay a lazy network that each expectation
    /// contracts exactly, so there is no MPS and no Cost. Each Query's
    /// descriptors and device buffers are released before the next one, so
    /// device memory does not grow with the number of observables.
    fn evaluate_observables(
        &mut self,
        circuit: &Circuit,
        observables: &[PauliSum],
        mps: bool,
    ) -> Result<StateQueryResult, SimulationError> {
        let mut timings = StatePhaseTimings::default();
        let mut cost = if mps {
            self.finalize_initial(circuit, &mut timings)?;
            let state = self.materialize_current_state(StateReadout::MetadataOnly, timings)?;
            Some(mps_cost(&state.report)?)
        } else {
            self.register_circuit(circuit, &mut timings)?;
            None
        };
        let mut expectations = Vec::with_capacity(observables.len());
        for observable in observables {
            let mut value = observable.identity_coefficient();
            if !observable.terms().is_empty() {
                let mark = self.allocations.len();
                let query = self.execute_pauli_query(observable);
                let release = self.release_query(mark);
                let query = combine_execution_and_cleanup(query, release)?;
                value += query.normalized_expectation;
                if let Some(cost) = &mut cost {
                    cost.workspace_bytes = cost
                        .workspace_bytes
                        .max(query.workspace.native_recommended_bytes);
                }
            }
            expectations.push(value);
        }
        Ok(StateQueryResult { expectations, cost })
    }

    /// Evaluates the unnormalized probability P̃ = ⟨ψ̃|ψ̃⟩ of `program`'s
    /// fixed outcomes.
    ///
    /// Gates and basis operators are registered in order, then the state is
    /// finalized as a bond-capped MPS and computed once, which yields the
    /// Cost. Basis operators are not unitary, so |ψ̃⟩ carries the probability
    /// as its norm, and nothing rescales it. The norm is read from the
    /// expectation of the identity on site 0, whose value ⟨ψ̃|I|ψ̃⟩ must agree
    /// with the norm reported beside it. Truncation is relative only: the
    /// absolute cutoff of the policy is dropped for this evaluation.
    fn evaluate_probability(
        &mut self,
        program: &ProjectedCircuit,
    ) -> Result<ProbabilityResult, SimulationError> {
        self.check_width(program)?;
        // An absolute cutoff would discard every singular value once the
        // retained weight falls below it; see
        // [`ExecutionPolicy::without_absolute_cutoff`].
        self.policy = self.policy.without_absolute_cutoff();
        let mut timings = StatePhaseTimings::default();
        for &operation in program.operations() {
            match operation {
                ProjectedOperation::Gate(gate) => self.register_gate(gate, &mut timings)?,
                ProjectedOperation::BasisOperator {
                    target,
                    result,
                    basis,
                } => {
                    let phase_started = Instant::now();
                    let operator =
                        table_operator(vec![mode_id(target)?], &basis_operator(result, basis))?;
                    timings.matrix_construction_seconds += phase_started.elapsed().as_secs_f64();
                    self.register_operator(operator, false, &mut timings)?;
                }
            }
        }
        self.finalize(&mut timings)?;
        let state = self.materialize_current_state(StateReadout::MetadataOnly, timings)?;
        let mut cost = mps_cost(&state.report)?;
        let norm = self.execute_projector_expectation(0, [1.0, 1.0])?;
        cost.workspace_bytes = cost.workspace_bytes.max(norm.workspace_bytes);
        Ok(ProbabilityResult {
            probability: unnormalized_probability(norm.value, norm.squared_norm)?,
            cost,
        })
    }

    /// Closes the Query lifecycle, then frees the allocations made since
    /// `mark`, newest first. The Query has synchronized, so no device work
    /// still reads them.
    fn release_query(&mut self, mark: usize) -> Result<(), SimulationError> {
        let mut first_error = self.close_query_lifecycle().err();
        while self.allocations.len() > mark {
            let allocation = self
                .allocations
                .pop()
                .expect("allocations above the mark exist");
            if let Err(error) = self.api.free(allocation)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn execute_query(&mut self, query: &AdjacentZQuery) -> Result<QueryResult, SimulationError> {
        if usize::try_from(query.width).ok() != Some(self.state_extents.len()) {
            return Err(SimulationError::InvalidCircuit {
                reason: "Query width does not match the native state".to_string(),
            });
        }
        self.execute_pauli_query(&query.pauli_sum())
    }

    /// Evaluates `Σₖ cₖ⟨ψ|Pₖ|ψ⟩ / ⟨ψ|ψ⟩` over the non-identity terms; the
    /// caller adds the identity coefficient. Leaves the Query lifecycle and
    /// its allocations open, so the caller decides when to release them.
    ///
    /// Each factor is a one-mode tensor. For one mode, `AppendProduct`'s
    /// forward mode order and `ApplyTensorOperator`'s reversed order coincide,
    /// so the textbook row-major matrix is read as `⟨i|P|j⟩`; this matters
    /// for Y, the only non-symmetric Pauli.
    #[allow(
        clippy::too_many_lines,
        reason = "the separate Query lifecycle is one failure and timing transaction"
    )]
    fn execute_pauli_query(
        &mut self,
        observable: &PauliSum,
    ) -> Result<QueryResult, SimulationError> {
        let mut timings = QueryPhaseTimings::default();
        if observable.terms().is_empty() {
            return Err(SimulationError::InvalidCircuit {
                reason: "a Query needs at least one non-identity Pauli term".to_string(),
            });
        }
        if let Some(qubit) = observable.max_qubit()
            && qubit >= self.state_extents.len()
        {
            return Err(SimulationError::InvalidCircuit {
                reason: format!(
                    "Query acts on qubit {qubit} of a {}-qubit state",
                    self.state_extents.len()
                ),
            });
        }
        if self.network_operator.is_some()
            || self.expectation.is_some()
            || self.query_workspace.is_some()
        {
            return Err(SimulationError::InvalidCircuit {
                reason: "this MPS execution already owns a Query lifecycle".to_string(),
            });
        }

        let phase_started = Instant::now();
        let operator = self
            .api
            .create_network_operator(self.handle, self.state_extents.as_ref())?;
        self.network_operator = Some(operator);
        let mut pauli_tensors = PauliTensors::default();
        for term in observable.terms() {
            let mut factor_modes = Vec::with_capacity(term.factors().len());
            let mut factor_tensors = Vec::with_capacity(term.factors().len());
            for &(qubit, pauli) in term.factors() {
                let tensor = if let Some(tensor) = *pauli_tensors.slot(pauli) {
                    tensor
                } else {
                    let matrix = pauli.matrix().map(Complex64Abi::from);
                    let tensor = self.allocate_complex(matrix.len(), "Query Pauli tensor")?;
                    self.api.copy_to_device(tensor, &matrix)?;
                    *pauli_tensors.slot(pauli) = Some(tensor);
                    tensor
                };
                factor_modes.push(vec![qubit_mode_id(qubit)?].into_boxed_slice());
                factor_tensors.push(tensor);
            }
            self.api.append_product(
                self.handle,
                operator,
                term.coefficient(),
                &factor_modes,
                &factor_tensors,
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

    /// Evaluates the one-site diagonal operator `diagonal` on `mode` in its
    /// own Query lifecycle, closed before returning.
    fn execute_projector_expectation(
        &mut self,
        mode: i32,
        diagonal: [f64; 2],
    ) -> Result<ProjectorExpectation, SimulationError> {
        self.check_modes(&[mode])?;
        if self.network_operator.is_some()
            || self.expectation.is_some()
            || self.query_workspace.is_some()
        {
            return Err(SimulationError::InvalidCircuit {
                reason: "this MPS execution already owns a Query lifecycle".to_string(),
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
            let (value, squared_norm) =
                self.api
                    .compute_expectation(self.handle, expectation, workspace, self.stream)?;
            let synchronization_started = Instant::now();
            self.api.synchronize_stream(self.stream)?;
            Ok(ProjectorExpectation {
                value,
                squared_norm,
                synchronization_seconds: synchronization_started.elapsed().as_secs_f64(),
                workspace_bytes: workspace_size,
            })
        })();
        let cleanup = self.close_query_lifecycle();
        combine_execution_and_cleanup(execution, cleanup)
    }

    /// Uploads `operator` and applies it to the state. Every operator reaches
    /// the state here, so this is where its modes are checked against the
    /// state, before any allocation.
    fn register_operator(
        &mut self,
        operator: OwnedOperator,
        unitary: bool,
        timings: &mut StatePhaseTimings,
    ) -> Result<(), SimulationError> {
        self.check_modes(&operator.modes)?;
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
        let result =
            self.api
                .apply_tensor_operator(self.handle, self.state(), modes, tensor, unitary);
        timings.operator_registration_seconds += phase_started.elapsed().as_secs_f64();
        result
    }

    /// Rejects modes outside the state; the library would otherwise index
    /// past its mode extents.
    fn check_modes(&self, modes: &[i32]) -> Result<(), SimulationError> {
        let width = self.state_extents.len();
        match modes
            .iter()
            .find(|&&mode| usize::try_from(mode).map_or(true, |mode| mode >= width))
        {
            Some(mode) => Err(SimulationError::InvalidCircuit {
                reason: format!("operator mode {mode} is outside the {width}-qubit native state"),
            }),
            None => Ok(()),
        }
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
        // No normalization is the library default, pinned so the norm stays
        // the retained weight whichever default a future release picks.
        self.api.configure_state_u32(
            self.handle,
            state,
            StateU32Configuration::SvdNormalizationNone,
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
        self.state
            .expect("a live MPS execution always owns its state")
    }

    fn workspace(&self) -> OpaqueHandle {
        self.workspace
            .expect("a live MPS execution always owns its workspace descriptor")
    }

    /// Attempts all releases and returns the first failure; subsequent closes
    /// are no-ops, including after a native destruction failure.
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

fn compute_branch_masses<Api: MpsExecutionApi + ?Sized>(
    mps_execution: &mut MpsExecution<'_, Api>,
    mode: u32,
) -> Result<(branch::BranchMasses, f64), SimulationError> {
    let mode_id = mode_id(mode)?;
    let p0 = mps_execution.execute_projector_expectation(mode_id, [1.0, 0.0])?;
    let p1 = mps_execution.execute_projector_expectation(mode_id, [0.0, 1.0])?;
    let masses = branch::BranchMasses::from_expectations(
        p0.value,
        p0.squared_norm,
        p1.value,
        p1.squared_norm,
    )?;
    Ok((
        masses,
        p0.synchronization_seconds + p1.synchronization_seconds,
    ))
}

fn apply_projection<Api: MpsExecutionApi + ?Sized>(
    mps_execution: &mut MpsExecution<'_, Api>,
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
    mps_execution.register_operator(
        OwnedOperator::new(vec![mode_id], projector)?,
        false,
        &mut StatePhaseTimings::default(),
    )
}

fn preparation_compute_seconds(timings: &StatePhaseTimings) -> f64 {
    timings.preparation_workspace_sizing_seconds
        + timings.workspace_allocation_attachment_seconds
        + timings.state_compute_call_seconds
}

/// The operator of `gate`: its shared-table matrix, copied verbatim because
/// cuTensorNet reads the row-major `M[out][in]` layout with default strides,
/// on its operands in gate order (first operand most significant).
fn fixture_operator(gate: Gate) -> Result<OwnedOperator, SimulationError> {
    let modes = match gate {
        Gate::X { target }
        | Gate::Y { target }
        | Gate::Z { target }
        | Gate::H { target }
        | Gate::S { target }
        | Gate::SAdj { target }
        | Gate::T { target }
        | Gate::TAdj { target }
        | Gate::Sx { target }
        | Gate::SxAdj { target }
        | Gate::Rx { target, .. }
        | Gate::Ry { target, .. }
        | Gate::Rz { target, .. } => vec![mode_id(target)?],
        Gate::Cnot { control, target }
        | Gate::Cy { control, target }
        | Gate::Cz { control, target } => {
            vec![mode_id(control)?, mode_id(target)?]
        }
        Gate::Rxx { q1, q2, .. }
        | Gate::Ryy { q1, q2, .. }
        | Gate::Rzz { q1, q2, .. }
        | Gate::Swap { q1, q2 } => vec![mode_id(q1)?, mode_id(q2)?],
    };
    let matrix = unitary_matrix(gate.into())
        .expect("the shared operator table defines every cuTensorNet gate");
    table_operator(modes, &matrix)
}

/// A shared-table matrix on `modes`, copied verbatim (see [`fixture_operator`]).
fn table_operator(
    modes: Vec<i32>,
    matrix: &OperatorMatrix,
) -> Result<OwnedOperator, SimulationError> {
    OwnedOperator::new(
        modes,
        matrix
            .row_major()
            .iter()
            .copied()
            .map(Complex64Abi::from)
            .collect(),
    )
}

/// The MPS Cost of a computed state: its largest bond, the bytes of its
/// realized tensors and the state workspace.
fn mps_cost(report: &ExecutionReport) -> Result<MpsCost, SimulationError> {
    let overflow = || SimulationError::ResourceSizeOverflow {
        resource: "MPS state bytes",
    };
    let state_elements = report
        .realized_extents
        .iter()
        .map(|extents| extents.iter().product::<usize>())
        .try_fold(0_usize, usize::checked_add)
        .ok_or_else(overflow)?;
    Ok(MpsCost {
        max_bond_dimension: report.maximum_bond,
        state_bytes: state_elements
            .checked_mul(size_of::<Complex64Abi>())
            .ok_or_else(overflow)?,
        workspace_bytes: report.workspace.native_recommended_bytes,
    })
}

/// Relative tolerance on the Probability norm's imaginary part and on its
/// agreement with ⟨ψ̃|I|ψ̃⟩. Relative, because P can be as small as 2⁻ᵐ.
const PROBABILITY_RELATIVE_TOLERANCE: f64 = 1.0e-12;

/// The Probability ⟨ψ̃|ψ̃⟩ from the identity expectation `value` and the norm
/// reported beside it. Zero is a valid Probability; a non-finite, complex or
/// negative norm, or one the identity expectation disagrees with, is not.
fn unnormalized_probability(
    value: Complex64,
    squared_norm: Complex64,
) -> Result<f64, SimulationError> {
    let invalid = |reason: String| Err(SimulationError::InvalidNativeResult { reason });
    if ![value.re, value.im, squared_norm.re, squared_norm.im]
        .iter()
        .all(|part| part.is_finite())
    {
        return invalid(format!(
            "Probability norm {squared_norm} or identity expectation {value} is not finite"
        ));
    }
    let probability = squared_norm.re;
    let tolerance = PROBABILITY_RELATIVE_TOLERANCE * probability.abs();
    if squared_norm.im.abs() > tolerance {
        return invalid(format!(
            "Probability norm {squared_norm} has a material imaginary part"
        ));
    }
    if probability < 0.0 {
        return invalid(format!("Probability norm {probability:e} is negative"));
    }
    if (value - squared_norm).norm() > tolerance {
        return invalid(format!(
            "identity expectation {value} does not match the Probability norm {squared_norm}"
        ));
    }
    Ok(probability)
}

impl<Api: MpsExecutionApi + ?Sized> Drop for MpsExecution<'_, Api> {
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

fn qubit_mode_id(qubit: QubitID) -> Result<i32, SimulationError> {
    i32::try_from(qubit).map_err(|_| SimulationError::InvalidCircuit {
        reason: format!("qubit {qubit} does not fit the native mode identifier"),
    })
}
