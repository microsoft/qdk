//! Host coverage of MPS state ownership and failure propagation.

use super::{
    MpsExecution, MpsExecutionApi, MpsProgram, MpsTarget, OutputMetadata, OwnedOperator,
    ProbabilityResult, StateF64Attribute, StatePhaseTimings, StateU32Configuration,
    combine_execution_and_cleanup, convert_layout, fixture_operator, saturating_power_of_two,
    target_bond_extent, unnormalized_probability, validate_realized_chain,
};
use crate::simulation::{
    Circuit, Gate, OpaqueHandle, ProjectedCircuit, SimulationError, Stream,
    branch::{BranchRequest, BranchSimulationResult, SelectedBranch},
    circuit::StateReadout,
    ffi::Complex64Abi,
    policy::ExecutionPolicy,
    query::{AdjacentZQuery, B2_EXPECTATION_HYPER_SAMPLES},
    sampler::{SamplerApi, SamplingRequest},
};
use std::{cell::RefCell, collections::VecDeque, ffi::c_void, ptr::NonNull};

use crate::execution::MpsCost;
use crate::simulation::memory_workspace::{
    MemorySpace, MemoryWorkspaceApi, WorkspaceKind, WorkspacePreference,
};
use num_complex::Complex64;
use qdk_simulators::{
    MeasurementResult,
    execution::{
        FixedOutcomeCircuit, FixedOutcomeOperation, OperatorMatrix, Pauli, PauliSum,
        UnitaryOperation, basis_operator, unitary_matrix,
    },
};
use std::mem::size_of;
use tensornet::Mps;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Event {
    CreateState,
    CreateWorkspace,
    Allocate(usize),
    CopyToDevice(usize),
    ApplyUnitary(usize),
    ApplyProjection,
    FinalizeMps,
    CaptureMps(usize),
    ConfigureF64(usize),
    ConfigureU32(usize),
    MemoryInfo,
    PrepareState,
    WorkspaceSize,
    SetWorkspace,
    ComputeState,
    Synchronize(usize),
    CopyFromDevice(usize),
    CreateNetworkOperator,
    AppendProduct(usize),
    CreateExpectation,
    ConfigureExpectation,
    CreateQueryWorkspace,
    PrepareExpectation,
    ComputeExpectation,
    DestroyQueryWorkspace,
    DestroyExpectation,
    DestroyNetworkOperator,
    QueryMemoryInfo,
    QueryWorkspaceSize,
    QuerySetWorkspace,
    CreateSampler,
    ConfigureSamplerHyperSamples,
    ConfigureSamplerPathSeed,
    PrepareSampler,
    ConfigureSamplerSampleSeed,
    Sample,
    DestroySampler,
    DestroyWorkspace,
    DestroyState,
    Free(usize),
}

impl Event {
    fn operation(self) -> &'static str {
        match self {
            Self::CreateState => "create_state",
            Self::CreateWorkspace => "create_workspace",
            Self::Allocate(_) => "allocate",
            Self::CopyToDevice(_) => "copy_to_device",
            Self::ApplyUnitary(_) | Self::ApplyProjection => "apply_tensor_operator",
            Self::FinalizeMps => "finalize_mps",
            Self::CaptureMps(_) => "capture_mps",
            Self::ConfigureF64(_) => "configure_state_f64",
            Self::ConfigureU32(_) => "configure_state_u32",
            Self::MemoryInfo | Self::QueryMemoryInfo => "memory_info",
            Self::PrepareState => "prepare_state",
            Self::WorkspaceSize | Self::QueryWorkspaceSize => "workspace_size",
            Self::SetWorkspace | Self::QuerySetWorkspace => "set_workspace",
            Self::ComputeState => "compute_state",
            Self::Synchronize(_) => "synchronize_stream",
            Self::CopyFromDevice(_) => "copy_from_device",
            Self::CreateNetworkOperator => "create_network_operator",
            Self::AppendProduct(_) => "append_product",
            Self::CreateExpectation => "create_expectation",
            Self::ConfigureExpectation => "configure_expectation",
            Self::CreateQueryWorkspace => "create_query_workspace",
            Self::PrepareExpectation => "prepare_expectation",
            Self::ComputeExpectation => "compute_expectation",
            Self::DestroyQueryWorkspace => "destroy_query_workspace",
            Self::DestroyExpectation => "destroy_expectation",
            Self::DestroyNetworkOperator => "destroy_network_operator",
            Self::CreateSampler => "create_sampler",
            Self::ConfigureSamplerHyperSamples => "configure_sampler_hyper_samples",
            Self::ConfigureSamplerPathSeed => "configure_sampler_path_seed",
            Self::PrepareSampler => "prepare_sampler",
            Self::ConfigureSamplerSampleSeed => "configure_sampler_sample_seed",
            Self::Sample => "sample",
            Self::DestroySampler => "destroy_sampler",
            Self::DestroyWorkspace => "destroy_workspace",
            Self::DestroyState => "destroy_state",
            Self::Free(_) => "free",
        }
    }
}

/// An operator as the test double saw it applied: its modes, whether it
/// was declared unitary, and the device data its tensor held.
#[derive(Clone, Debug, PartialEq)]
struct AppliedOperator {
    modes: Vec<i32>,
    unitary: bool,
    matrix: Vec<Complex64Abi>,
}

#[derive(Default)]
struct TestDoubleCallLog {
    events: Vec<Event>,
    next_handle: usize,
    allocate_count: usize,
    copy_to_device_count: usize,
    apply_count: usize,
    capture_count: usize,
    configure_f64_count: usize,
    configure_u32_count: usize,
    synchronize_count: usize,
    copy_from_device_count: usize,
    free_count: usize,
    allocation_handles: Vec<usize>,
    allocation_sizes: Vec<usize>,
    uploaded_tensors: Vec<Vec<Complex64Abi>>,
    /// Device allocation address and the data last copied into it.
    device_tensors: Vec<(usize, Vec<Complex64Abi>)>,
    applied_operators: Vec<AppliedOperator>,
    configured_f64: Vec<(StateF64Attribute, f64)>,
    configured_u32: Vec<StateU32Configuration>,
    freed_handles: Vec<usize>,
    prepare_maximum_workspace_bytes: Vec<usize>,
    append_count: usize,
    appended_modes: Vec<Vec<Vec<i32>>>,
    appended_coefficients: Vec<Complex64>,
    appended_tensors: Vec<Vec<usize>>,
    expectation_hyper_samples: Vec<i32>,
    state_workspace_handle: Option<usize>,
    workspace_handles: Vec<usize>,
    memory_info_count: usize,
    workspace_size_count: usize,
    set_workspace_count: usize,
}

struct TestDoubleMpsExecutionApi {
    state: RefCell<TestDoubleCallLog>,
    failures: Vec<Event>,
    memory_info: (usize, usize),
    workspace_size: i64,
    workspace_sizes: RefCell<VecDeque<i64>>,
    expectation_outputs: RefCell<VecDeque<(Complex64, Complex64)>>,
}

impl TestDoubleMpsExecutionApi {
    fn new(failures: impl IntoIterator<Item = Event>) -> Self {
        Self {
            state: RefCell::new(TestDoubleCallLog {
                next_handle: 0x100,
                ..TestDoubleCallLog::default()
            }),
            failures: failures.into_iter().collect(),
            memory_info: (64 * 1024 * 1024, 80 * 1024 * 1024),
            workspace_size: 256,
            workspace_sizes: RefCell::new(VecDeque::new()),
            expectation_outputs: RefCell::new(VecDeque::new()),
        }
    }

    fn with_workspace(mut self, free_bytes: usize, workspace_size: i64) -> Self {
        self.memory_info = (free_bytes, 80 * 1024 * 1024 * 1024);
        self.workspace_size = workspace_size;
        self
    }

    /// Workspace recommendations returned in call order, before falling back
    /// to the fixed `workspace_size`.
    fn with_workspace_sizes(self, sizes: impl IntoIterator<Item = i64>) -> Self {
        self.workspace_sizes.replace(sizes.into_iter().collect());
        self
    }

    fn with_expectations(self, outputs: impl IntoIterator<Item = (Complex64, Complex64)>) -> Self {
        self.expectation_outputs
            .replace(outputs.into_iter().collect());
        self
    }

    fn events(&self) -> Vec<Event> {
        self.state.borrow().events.clone()
    }

    fn allocation_handles(&self) -> Vec<usize> {
        self.state.borrow().allocation_handles.clone()
    }

    fn freed_handles(&self) -> Vec<usize> {
        self.state.borrow().freed_handles.clone()
    }

    fn allocation_sizes(&self) -> Vec<usize> {
        self.state.borrow().allocation_sizes.clone()
    }

    fn uploaded_tensors(&self) -> Vec<Vec<Complex64Abi>> {
        self.state.borrow().uploaded_tensors.clone()
    }

    fn applied_operators(&self) -> Vec<AppliedOperator> {
        self.state.borrow().applied_operators.clone()
    }

    fn configured_f64(&self) -> Vec<(StateF64Attribute, f64)> {
        self.state.borrow().configured_f64.clone()
    }

    fn configured_u32(&self) -> Vec<StateU32Configuration> {
        self.state.borrow().configured_u32.clone()
    }

    fn device_tensor(&self, tensor: usize) -> Vec<Complex64Abi> {
        self.state
            .borrow()
            .device_tensors
            .iter()
            .rev()
            .find(|(address, _)| *address == tensor)
            .map(|(_, data)| data.clone())
            .expect("the tensor was copied to the device before use")
    }

    fn appended_tensors(&self) -> Vec<Vec<usize>> {
        self.state.borrow().appended_tensors.clone()
    }

    fn appended_modes(&self) -> Vec<Vec<Vec<i32>>> {
        self.state.borrow().appended_modes.clone()
    }

    fn prepare_maximum_workspace_bytes(&self) -> Vec<usize> {
        self.state.borrow().prepare_maximum_workspace_bytes.clone()
    }

    fn record(&self, event: Event) -> Result<(), SimulationError> {
        self.state.borrow_mut().events.push(event);
        if self.failures.contains(&event) {
            Err(SimulationError::NativeCallFailed {
                component: "MPS execution test double",
                operation: event.operation(),
                status: 17,
                message: "injected failure".to_string(),
            })
        } else {
            Ok(())
        }
    }

    fn handle(&self) -> OpaqueHandle {
        let mut state = self.state.borrow_mut();
        let address = state.next_handle;
        state.next_handle += 0x100;
        NonNull::new(address as *mut c_void).expect("fake handles are non-null")
    }

    fn next_allocate(&self) -> Event {
        let mut state = self.state.borrow_mut();
        let event = Event::Allocate(state.allocate_count);
        state.allocate_count += 1;
        event
    }

    fn next_copy_to_device(&self) -> Event {
        let mut state = self.state.borrow_mut();
        let event = Event::CopyToDevice(state.copy_to_device_count);
        state.copy_to_device_count += 1;
        event
    }

    fn next_apply(&self, unitary: bool) -> Event {
        if !unitary {
            return Event::ApplyProjection;
        }
        let mut state = self.state.borrow_mut();
        let event = Event::ApplyUnitary(state.apply_count);
        state.apply_count += 1;
        event
    }

    fn next_capture(&self) -> Event {
        let mut state = self.state.borrow_mut();
        let event = Event::CaptureMps(state.capture_count);
        state.capture_count += 1;
        event
    }

    fn next_configure_f64(&self) -> Event {
        let mut state = self.state.borrow_mut();
        let event = Event::ConfigureF64(state.configure_f64_count);
        state.configure_f64_count += 1;
        event
    }

    fn next_configure_u32(&self) -> Event {
        let mut state = self.state.borrow_mut();
        let event = Event::ConfigureU32(state.configure_u32_count);
        state.configure_u32_count += 1;
        event
    }

    fn next_synchronize(&self) -> Event {
        let mut state = self.state.borrow_mut();
        let event = Event::Synchronize(state.synchronize_count);
        state.synchronize_count += 1;
        event
    }

    fn next_copy_from_device(&self) -> Event {
        let mut state = self.state.borrow_mut();
        let event = Event::CopyFromDevice(state.copy_from_device_count);
        state.copy_from_device_count += 1;
        event
    }

    fn next_free(&self) -> Event {
        let mut state = self.state.borrow_mut();
        let event = Event::Free(state.free_count);
        state.free_count += 1;
        event
    }

    fn next_append(&self) -> Event {
        let mut state = self.state.borrow_mut();
        let event = Event::AppendProduct(state.append_count);
        state.append_count += 1;
        event
    }
}

impl MemoryWorkspaceApi for TestDoubleMpsExecutionApi {
    fn memory_info(&self) -> Result<(usize, usize), SimulationError> {
        let event = {
            let mut state = self.state.borrow_mut();
            let event = if state.memory_info_count == 0 {
                Event::MemoryInfo
            } else {
                Event::QueryMemoryInfo
            };
            state.memory_info_count += 1;
            event
        };
        self.record(event)?;
        Ok(self.memory_info)
    }

    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, SimulationError> {
        self.record(self.next_allocate())?;
        let allocation = self.handle();
        let mut state = self.state.borrow_mut();
        state.allocation_handles.push(allocation.as_ptr() as usize);
        state.allocation_sizes.push(bytes);
        Ok(allocation)
    }

    fn free(&self, allocation: OpaqueHandle) -> Result<(), SimulationError> {
        self.state
            .borrow_mut()
            .freed_handles
            .push(allocation.as_ptr() as usize);
        self.record(self.next_free())
    }

    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), SimulationError> {
        let mut state = self.state.borrow_mut();
        state.uploaded_tensors.push(source.to_vec());
        state
            .device_tensors
            .push((destination.as_ptr() as usize, source.to_vec()));
        drop(state);
        self.record(self.next_copy_to_device())
    }

    fn copy_from_device(
        &self,
        _source: OpaqueHandle,
        destination: &mut [Complex64Abi],
    ) -> Result<(), SimulationError> {
        let event = self.next_copy_from_device();
        self.record(event)?;
        let zero = Complex64Abi::new(0.0, 0.0);
        let one = Complex64Abi::new(1.0, 0.0);
        let scale = Complex64Abi::new(std::f64::consts::FRAC_1_SQRT_2, 0.0);
        let left = [scale, zero, zero, scale];
        let right = [one, zero, zero, one];
        destination.copy_from_slice(match event {
            Event::CopyFromDevice(index) if index.is_multiple_of(2) => &left,
            Event::CopyFromDevice(_) => &right,
            _ => unreachable!("copy events always include an index"),
        });
        Ok(())
    }

    fn create_workspace(&self, _handle: OpaqueHandle) -> Result<OpaqueHandle, SimulationError> {
        let event = {
            let state = self.state.borrow();
            if state.state_workspace_handle.is_none() {
                Event::CreateWorkspace
            } else {
                Event::CreateQueryWorkspace
            }
        };
        self.record(event)?;
        let workspace = self.handle();
        let mut state = self.state.borrow_mut();
        let address = workspace.as_ptr() as usize;
        if state.state_workspace_handle.is_none() {
            state.state_workspace_handle = Some(address);
        }
        state.workspace_handles.push(address);
        Ok(workspace)
    }

    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), SimulationError> {
        let event = {
            let state = self.state.borrow();
            if state.state_workspace_handle == Some(workspace.as_ptr() as usize) {
                Event::DestroyWorkspace
            } else {
                Event::DestroyQueryWorkspace
            }
        };
        self.record(event)
    }

    fn workspace_memory_size(
        &self,
        _handle: OpaqueHandle,
        _workspace: OpaqueHandle,
        preference: WorkspacePreference,
        space: MemorySpace,
        kind: WorkspaceKind,
    ) -> Result<i64, SimulationError> {
        assert_eq!(preference, WorkspacePreference::Recommended);
        assert_eq!(space, MemorySpace::Device);
        assert_eq!(kind, WorkspaceKind::Scratch);
        let event = {
            let mut state = self.state.borrow_mut();
            let event = if state.workspace_size_count == 0 {
                Event::WorkspaceSize
            } else {
                Event::QueryWorkspaceSize
            };
            state.workspace_size_count += 1;
            event
        };
        self.record(event)?;
        Ok(self
            .workspace_sizes
            .borrow_mut()
            .pop_front()
            .unwrap_or(self.workspace_size))
    }

    fn set_workspace_memory(
        &self,
        _handle: OpaqueHandle,
        _workspace: OpaqueHandle,
        space: MemorySpace,
        kind: WorkspaceKind,
        allocation: Option<OpaqueHandle>,
        _bytes: i64,
    ) -> Result<(), SimulationError> {
        assert_eq!(space, MemorySpace::Device);
        assert_eq!(kind, WorkspaceKind::Scratch);
        assert!(allocation.is_some());
        let event = {
            let mut state = self.state.borrow_mut();
            let event = if state.set_workspace_count == 0 {
                Event::SetWorkspace
            } else {
                Event::QuerySetWorkspace
            };
            state.set_workspace_count += 1;
            event
        };
        self.record(event)
    }
}

impl MpsExecutionApi for TestDoubleMpsExecutionApi {
    fn create_state(
        &self,
        _handle: OpaqueHandle,
        _mode_extents: &[i64],
    ) -> Result<OpaqueHandle, SimulationError> {
        self.record(Event::CreateState)?;
        Ok(self.handle())
    }

    fn destroy_state(&self, _state: OpaqueHandle) -> Result<(), SimulationError> {
        self.record(Event::DestroyState)
    }

    fn apply_tensor_operator(
        &self,
        _handle: OpaqueHandle,
        _state: OpaqueHandle,
        modes: &[i32],
        tensor: OpaqueHandle,
        unitary: bool,
    ) -> Result<(), SimulationError> {
        let matrix = self.device_tensor(tensor.as_ptr() as usize);
        self.state
            .borrow_mut()
            .applied_operators
            .push(AppliedOperator {
                modes: modes.to_vec(),
                unitary,
                matrix,
            });
        self.record(self.next_apply(unitary))
    }

    fn finalize_mps(
        &self,
        _handle: OpaqueHandle,
        _state: OpaqueHandle,
        _target: &MpsTarget,
    ) -> Result<(), SimulationError> {
        self.record(Event::FinalizeMps)
    }

    fn capture_mps(
        &self,
        _handle: OpaqueHandle,
        _state: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        self.record(self.next_capture())
    }

    fn configure_state_f64(
        &self,
        _handle: OpaqueHandle,
        _state: OpaqueHandle,
        attribute: StateF64Attribute,
        value: f64,
    ) -> Result<(), SimulationError> {
        self.state
            .borrow_mut()
            .configured_f64
            .push((attribute, value));
        self.record(self.next_configure_f64())
    }

    fn configure_state_u32(
        &self,
        _handle: OpaqueHandle,
        _state: OpaqueHandle,
        configuration: StateU32Configuration,
    ) -> Result<(), SimulationError> {
        self.state.borrow_mut().configured_u32.push(configuration);
        self.record(self.next_configure_u32())
    }

    fn prepare_state(
        &self,
        _handle: OpaqueHandle,
        _state: OpaqueHandle,
        maximum_workspace_bytes: usize,
        _workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(), SimulationError> {
        self.state
            .borrow_mut()
            .prepare_maximum_workspace_bytes
            .push(maximum_workspace_bytes);
        self.record(Event::PrepareState)
    }

    fn compute_state(
        &self,
        _handle: OpaqueHandle,
        _state: OpaqueHandle,
        _workspace: OpaqueHandle,
        metadata: &mut OutputMetadata,
        _outputs: &mut [OpaqueHandle],
        _stream: Stream,
    ) -> Result<(), SimulationError> {
        self.record(Event::ComputeState)?;
        metadata.extents[0].copy_from_slice(&[2, 2]);
        metadata.extents[1].copy_from_slice(&[2, 2]);
        metadata.strides[0].copy_from_slice(&[2, 1]);
        metadata.strides[1].copy_from_slice(&[2, 1]);
        Ok(())
    }

    fn synchronize_stream(&self, _stream: Stream) -> Result<(), SimulationError> {
        self.record(self.next_synchronize())
    }

    fn create_network_operator(
        &self,
        _handle: OpaqueHandle,
        _mode_extents: &[i64],
    ) -> Result<OpaqueHandle, SimulationError> {
        self.record(Event::CreateNetworkOperator)?;
        Ok(self.handle())
    }

    fn destroy_network_operator(&self, _operator: OpaqueHandle) -> Result<(), SimulationError> {
        self.record(Event::DestroyNetworkOperator)
    }

    fn append_product(
        &self,
        _handle: OpaqueHandle,
        _operator: OpaqueHandle,
        coefficient: Complex64,
        factor_modes: &[Box<[i32]>],
        factor_tensors: &[OpaqueHandle],
    ) -> Result<(), SimulationError> {
        assert_eq!(factor_modes.len(), factor_tensors.len());
        let mut state = self.state.borrow_mut();
        state
            .appended_modes
            .push(factor_modes.iter().map(|modes| modes.to_vec()).collect());
        state.appended_coefficients.push(coefficient);
        state.appended_tensors.push(
            factor_tensors
                .iter()
                .map(|tensor| tensor.as_ptr() as usize)
                .collect(),
        );
        drop(state);
        self.record(self.next_append())
    }

    fn create_expectation(
        &self,
        _handle: OpaqueHandle,
        _state: OpaqueHandle,
        _operator: OpaqueHandle,
    ) -> Result<OpaqueHandle, SimulationError> {
        self.record(Event::CreateExpectation)?;
        Ok(self.handle())
    }

    fn destroy_expectation(&self, _expectation: OpaqueHandle) -> Result<(), SimulationError> {
        self.record(Event::DestroyExpectation)
    }

    fn configure_expectation_hyper_samples(
        &self,
        _handle: OpaqueHandle,
        _expectation: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), SimulationError> {
        self.state
            .borrow_mut()
            .expectation_hyper_samples
            .push(hyper_samples);
        self.record(Event::ConfigureExpectation)
    }

    fn prepare_expectation(
        &self,
        _handle: OpaqueHandle,
        _expectation: OpaqueHandle,
        maximum_workspace_bytes: usize,
        _workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(), SimulationError> {
        self.state
            .borrow_mut()
            .prepare_maximum_workspace_bytes
            .push(maximum_workspace_bytes);
        self.record(Event::PrepareExpectation)
    }

    fn compute_expectation(
        &self,
        _handle: OpaqueHandle,
        _expectation: OpaqueHandle,
        _workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(Complex64, Complex64), SimulationError> {
        self.record(Event::ComputeExpectation)?;
        Ok(self
            .expectation_outputs
            .borrow_mut()
            .pop_front()
            .unwrap_or((Complex64::new(6.0, 0.0), Complex64::new(2.0, 0.0))))
    }
}

impl SamplerApi for TestDoubleMpsExecutionApi {
    fn create_sampler(
        &self,
        _handle: OpaqueHandle,
        _state: OpaqueHandle,
        modes_to_sample: &[i32],
    ) -> Result<OpaqueHandle, SimulationError> {
        assert_eq!(modes_to_sample, [0, 1]);
        self.record(Event::CreateSampler)?;
        Ok(self.handle())
    }

    fn destroy_sampler(&self, _sampler: OpaqueHandle) -> Result<(), SimulationError> {
        self.record(Event::DestroySampler)
    }

    fn configure_sampler_hyper_samples(
        &self,
        _handle: OpaqueHandle,
        _sampler: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), SimulationError> {
        assert_eq!(hyper_samples, 8);
        self.record(Event::ConfigureSamplerHyperSamples)
    }

    fn configure_sampler_path_seed(
        &self,
        _handle: OpaqueHandle,
        _sampler: OpaqueHandle,
        seed: i32,
    ) -> Result<(), SimulationError> {
        assert_eq!(seed, 11);
        self.record(Event::ConfigureSamplerPathSeed)
    }

    fn prepare_sampler(
        &self,
        _handle: OpaqueHandle,
        _sampler: OpaqueHandle,
        maximum_workspace_bytes: usize,
        _workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(), SimulationError> {
        assert_eq!(
            maximum_workspace_bytes,
            ExecutionPolicy::bell_regression().maximum_workspace_bytes
        );
        self.record(Event::PrepareSampler)
    }

    fn configure_sampler_sample_seed(
        &self,
        _handle: OpaqueHandle,
        _sampler: OpaqueHandle,
        seed: i32,
    ) -> Result<(), SimulationError> {
        assert_eq!(seed, 29);
        self.record(Event::ConfigureSamplerSampleSeed)
    }

    fn sample(
        &self,
        _handle: OpaqueHandle,
        _sampler: OpaqueHandle,
        shots: i64,
        _workspace: OpaqueHandle,
        output: &mut [i64],
        _stream: Stream,
    ) -> Result<(), SimulationError> {
        assert_eq!(shots, 3);
        assert_eq!(output.len(), 6);
        self.record(Event::Sample)?;
        output.copy_from_slice(&[0, 0, 1, 1, 0, 0]);
        Ok(())
    }
}

fn circuit() -> Circuit {
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
    circuit
}

fn run(api: &TestDoubleMpsExecutionApi) -> Result<(), SimulationError> {
    let circuit = circuit();
    let mut mps_execution = new_mps_execution(api, &circuit)?;
    let execution = mps_execution
        .execute(&circuit, StateReadout::FullAmplitudes)
        .map(|_| ());
    let cleanup = mps_execution.close();
    combine_execution_and_cleanup(execution, cleanup)
}

fn new_mps_execution<'api>(
    api: &'api TestDoubleMpsExecutionApi,
    program: &impl MpsProgram,
) -> Result<MpsExecution<'api, TestDoubleMpsExecutionApi>, SimulationError> {
    let handle = NonNull::new(0x1000_usize as *mut c_void).expect("handle is non-null");
    let stream = NonNull::dangling();
    MpsExecution::new(
        api,
        handle,
        stream,
        program,
        ExecutionPolicy::bell_regression(),
    )
}

fn projected(qubit_count: usize, operations: Vec<FixedOutcomeOperation>) -> ProjectedCircuit {
    let circuit = FixedOutcomeCircuit::new(qubit_count, operations)
        .expect("fixed-outcome fixture should be valid");
    ProjectedCircuit::from_fixed_outcome(&circuit).expect("fixture should convert")
}

/// H(0), CNOT(0, 1), then outcome 0 on qubit 0 and outcome 1 on qubit 1
/// with a reset: |0⟩⟨0| on mode 0, then |0⟩⟨1| on mode 1.
fn bell_outcomes() -> ProjectedCircuit {
    projected(
        2,
        vec![
            FixedOutcomeOperation::Unitary(UnitaryOperation::H { target: 0 }),
            FixedOutcomeOperation::Unitary(UnitaryOperation::Cx {
                control: 0,
                target: 1,
            }),
            FixedOutcomeOperation::Measure {
                qubit: 0,
                result_id: 0,
                outcome: MeasurementResult::Zero,
                reset: false,
            },
            FixedOutcomeOperation::Measure {
                qubit: 1,
                result_id: 1,
                outcome: MeasurementResult::One,
                reset: true,
            },
        ],
    )
}

fn run_probability(
    api: &TestDoubleMpsExecutionApi,
    program: &ProjectedCircuit,
) -> Result<ProbabilityResult, SimulationError> {
    let mut mps_execution = new_mps_execution(api, program)?;
    let execution = mps_execution.evaluate_probability(program);
    let cleanup = mps_execution.close();
    combine_execution_and_cleanup(execution, cleanup)
}

/// A test double whose identity expectation agrees with its norm, 1/2.
fn probability_api(failures: impl IntoIterator<Item = Event>) -> TestDoubleMpsExecutionApi {
    let half = Complex64::new(0.5, 0.0);
    TestDoubleMpsExecutionApi::new(failures).with_expectations([(half, half)])
}

fn table_values(matrix: &OperatorMatrix) -> Vec<Complex64Abi> {
    matrix
        .row_major()
        .iter()
        .copied()
        .map(Complex64Abi::from)
        .collect()
}

fn continuation_circuit() -> Circuit {
    let mut continuation = Circuit::new(2).expect("two-qubit continuation should be valid");
    continuation
        .push(Gate::H { target: 1 })
        .expect("continuation Hadamard should be valid");
    continuation
        .push(Gate::Cnot {
            control: 1,
            target: 0,
        })
        .expect("continuation CNOT should be valid");
    continuation
}

fn branch_api(
    failures: impl IntoIterator<Item = Event>,
    q0: f64,
    q1: f64,
) -> TestDoubleMpsExecutionApi {
    TestDoubleMpsExecutionApi::new(failures).with_expectations([
        (Complex64::new(q0, 0.0), Complex64::new(1.0, 0.0)),
        (Complex64::new(q1, 0.0), Complex64::new(1.0, 0.0)),
        (Complex64::new(0.25, 0.0), Complex64::new(1.0, 0.0)),
    ])
}

fn run_branch(
    api: &TestDoubleMpsExecutionApi,
    selected: SelectedBranch,
) -> Result<BranchSimulationResult, SimulationError> {
    let initial = circuit();
    let continuation = continuation_circuit();
    let query = AdjacentZQuery::new(2).expect("Query should be valid");
    let mut mps_execution = new_mps_execution(api, &initial)?;
    let execution = mps_execution.execute_branch(
        &initial,
        BranchRequest { mode: 0, selected },
        &continuation,
        &query,
    );
    let cleanup = mps_execution.close();
    combine_execution_and_cleanup(execution, cleanup)
}

fn positions(events: &[Event], selected: Event) -> Vec<usize> {
    events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| (*event == selected).then_some(index))
        .collect()
}

fn assert_native_operation(error: &SimulationError, operation: &'static str) {
    assert!(
        matches!(
            error,
            SimulationError::NativeCallFailed {
                operation: actual,
                status: 17,
                ..
            } if *actual == operation
        ),
        "expected injected {operation} failure, got {error:?}"
    );
}

fn apply_one_site(matrix: &[Complex64Abi], input: [Complex64; 2]) -> [Complex64; 2] {
    assert_eq!(matrix.len(), 4);
    [
        Complex64::from(matrix[0]) * input[0] + Complex64::from(matrix[1]) * input[1],
        Complex64::from(matrix[2]) * input[0] + Complex64::from(matrix[3]) * input[1],
    ]
}

fn assert_complex_close(actual: Complex64, expected: Complex64) {
    let error = (actual - expected).norm();
    assert!(
        error <= 1.0e-15,
        "expected {expected:?}, got {actual:?}, error {error}"
    );
}

#[test]
fn target_bonds_are_checked_without_exponential_allocation() {
    let cases: &[(usize, i64, &[i64])] = &[
        (2, 128, &[2]),
        (3, 128, &[2, 2]),
        (63, 128, &[2, 4, 8, 16, 32, 64, 128]),
        (64, 128, &[2, 4, 8, 16, 32, 64, 128]),
        (128, 128, &[2, 4, 8, 16, 32, 64, 128]),
    ];

    for &(width, cap, leading) in cases {
        let bonds = (0..width - 1)
            .map(|cut| target_bond_extent(width, cut, cap).expect("bond should be valid"))
            .collect::<Vec<_>>();
        assert_eq!(
            &bonds[..leading.len().min(bonds.len())],
            &leading[..leading.len().min(bonds.len())]
        );
        assert_eq!(bonds.first(), Some(&2));
        assert_eq!(bonds.last(), Some(&2));
        assert!(bonds.iter().all(|extent| *extent <= cap));
        assert_eq!(bonds, bonds.iter().copied().rev().collect::<Vec<_>>());

        let target = MpsTarget::new(width, 2).expect("cap-2 target should be valid");
        assert_eq!(target.extents.len(), width);
        assert_eq!(target.extent_pointers.len(), width);
        assert_eq!(target.shape().site_count(), width);
        assert!(
            target
                .shape()
                .element_counts()
                .iter()
                .all(|elements| *elements <= 8)
        );
    }
}

#[test]
fn single_qubit_target_is_rejected() {
    assert!(matches!(
        MpsTarget::new(1, 128),
        Err(SimulationError::InvalidCircuit { reason })
            if reason == "MPS simulation requires at least two qubits; use type=\"cpu\" for single-qubit circuits"
    ));
}

#[test]
fn target_bonds_respect_caps_below_and_above_physical_limits() {
    assert_eq!(target_bond_extent(8, 3, 3).expect("center cut is valid"), 3);
    assert_eq!(
        target_bond_extent(8, 0, 1024).expect("edge cut is valid"),
        2
    );
    assert_eq!(
        target_bond_extent(8, 3, 1024).expect("center cut is valid"),
        16
    );
}

#[test]
fn target_bonds_reject_underflow_and_invalid_caps() {
    assert!(target_bond_extent(0, 0, 128).is_err());
    assert!(target_bond_extent(1, 0, 128).is_err());
    assert!(target_bond_extent(8, 7, 128).is_err());
    assert!(target_bond_extent(8, 8, 128).is_err());
    assert!(target_bond_extent(8, usize::MAX, 128).is_err());
    assert!(target_bond_extent(8, 0, 0).is_err());
    assert!(target_bond_extent(8, 0, -1).is_err());
}

#[test]
fn powers_of_two_saturate_before_signed_overflow() {
    assert_eq!(saturating_power_of_two(0), 1);
    assert_eq!(saturating_power_of_two(62), 1_i64 << 62);
    assert_eq!(saturating_power_of_two(63), i64::MAX);
    assert_eq!(saturating_power_of_two(64), i64::MAX);
    assert_eq!(saturating_power_of_two(128), i64::MAX);
}

#[test]
fn native_layout_conversion_rejects_nonpositive_and_overflowing_extents() {
    for invalid in [vec![2, 0], vec![2, -1]] {
        assert!(matches!(
            convert_layout("extent", &[invalid.into_boxed_slice()]),
            Err(SimulationError::InvalidNativeResult { .. })
        ));
    }
}

#[test]
fn realized_extents_are_bounded_and_report_maximum_bond() {
    let target = MpsTarget::new(4, 8).expect("target should be valid");
    let realized = chain(vec![vec![2, 2], vec![2, 2, 4], vec![4, 2, 2], vec![2, 2]]);

    validate_realized_chain(&realized, target.shape())
        .expect("realized extents should fit target capacities");
    assert_eq!(realized.max_bond(), 4);
}

#[test]
fn rejects_realized_chains_this_backend_cannot_accept() {
    let target = MpsTarget::new(3, 2).expect("target should be valid");
    let invalid = [
        // A bond grown past the capacity the library was given.
        chain(vec![vec![2, 3], vec![3, 2, 2], vec![2, 2]]),
        // A site that is not a qubit, which the dense readout cannot index.
        chain(vec![vec![3, 2], vec![2, 2, 2], vec![2, 2]]),
    ];

    for realized in invalid {
        assert!(validate_realized_chain(&realized, target.shape()).is_err());
    }
}

#[test]
fn malformed_realized_chains_never_reach_this_backend() {
    // Chain structure is `Mps`'s invariant, so neither of these can be built
    // in the first place and this backend has no check of its own for them.
    assert!(Mps::new(vec![vec![2, 2], vec![1, 2, 2], vec![2, 2]]).is_err());
    assert!(Mps::new(vec![vec![2, 2], vec![2, 2], vec![2, 2]]).is_err());
}

fn chain(sites: Vec<Vec<usize>>) -> Mps {
    Mps::new(sites).expect("test chain should be valid")
}

#[test]
fn x_operators_retain_qdk_mode_order() {
    let q0 = fixture_operator(Gate::X { target: 0 }).expect("X(0) is valid");
    let q1 = fixture_operator(Gate::X { target: 1 }).expect("X(1) is valid");

    assert_eq!(&*q0.modes, [0]);
    assert_eq!(&*q1.modes, [1]);
    assert_eq!(q0.matrix, q1.matrix);
}

#[test]
fn directional_cnot_operators_retain_operand_order() {
    let forward = fixture_operator(Gate::Cnot {
        control: 0,
        target: 1,
    })
    .expect("CNOT(0, 1) is valid");
    let reverse = fixture_operator(Gate::Cnot {
        control: 1,
        target: 0,
    })
    .expect("CNOT(1, 0) is valid");

    assert_eq!(&*forward.modes, [0, 1]);
    assert_eq!(&*reverse.modes, [1, 0]);
    assert_eq!(forward.matrix, reverse.matrix);
}

#[test]
fn asymmetric_rx_uses_negative_imaginary_half_angle() {
    let theta = 0.731;
    let operator =
        fixture_operator(Gate::Rx { theta, target: 1 }).expect("finite Rx should be valid");
    let output = apply_one_site(
        &operator.matrix,
        [Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)],
    );
    let (sine, cosine) = (theta / 2.0).sin_cos();

    assert_eq!(&*operator.modes, [1]);
    assert_complex_close(output[0], Complex64::new(cosine, 0.0));
    assert_complex_close(output[1], Complex64::new(0.0, -sine));
}

#[test]
fn rz_phase_interferes_with_the_expected_sign() {
    let theta = 0.913;
    let hadamard = fixture_operator(Gate::H { target: 0 }).expect("H is valid");
    let rz = fixture_operator(Gate::Rz { theta, target: 0 }).expect("Rz is valid");
    let zero = [Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)];
    let output = apply_one_site(
        &hadamard.matrix,
        apply_one_site(&rz.matrix, apply_one_site(&hadamard.matrix, zero)),
    );
    let (sine, cosine) = (theta / 2.0).sin_cos();

    assert_complex_close(output[0], Complex64::new(cosine, 0.0));
    assert_complex_close(output[1], Complex64::new(0.0, -sine));
}

/// The `(re, im)` bit patterns of an operator, so signed zeros count.
fn operator_bits(matrix: &[Complex64Abi]) -> Vec<(u64, u64)> {
    matrix
        .iter()
        .map(|&value| {
            let value = Complex64::from(value);
            (value.re.to_bits(), value.im.to_bits())
        })
        .collect()
}

#[test]
fn operator_data_is_pinned_bit_for_bit() {
    let theta = 0.731_f64;
    let (sine, cosine) = (theta / 2.0).sin_cos();
    let c = Complex64::new;
    let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let (minus, plus) = (c(cosine, -sine), c(cosine, sine));
    let cases = [
        (Gate::X { target: 0 }, vec![o, l, l, o]),
        (
            Gate::H { target: 0 },
            vec![c(h, 0.0), c(h, 0.0), c(h, 0.0), c(-h, 0.0)],
        ),
        (
            Gate::Rx { theta, target: 0 },
            vec![c(cosine, 0.0), c(0.0, -sine), c(0.0, -sine), c(cosine, 0.0)],
        ),
        (Gate::Rz { theta, target: 0 }, vec![minus, o, o, plus]),
        (Gate::S { target: 0 }, vec![l, o, o, c(0.0, 1.0)]),
        (
            Gate::Sx { target: 0 },
            vec![c(0.5, 0.5), c(0.5, -0.5), c(0.5, -0.5), c(0.5, 0.5)],
        ),
        (
            Gate::Cz {
                control: 0,
                target: 1,
            },
            vec![l, o, o, o, o, l, o, o, o, o, l, o, o, o, o, c(-1.0, 0.0)],
        ),
        (
            Gate::Cnot {
                control: 0,
                target: 1,
            },
            vec![l, o, o, o, o, l, o, o, o, o, o, l, o, o, l, o],
        ),
        (
            Gate::Rzz {
                theta,
                q1: 0,
                q2: 1,
            },
            vec![minus, o, o, o, o, plus, o, o, o, o, plus, o, o, o, o, minus],
        ),
    ];
    for (gate, expected) in cases {
        let operator = fixture_operator(gate).expect("valid gate");
        let expected: Vec<_> = expected.into_iter().map(Complex64Abi::from).collect();
        assert_eq!(
            operator_bits(&operator.matrix),
            operator_bits(&expected),
            "{gate:?}"
        );
    }
}

#[test]
fn operators_copy_the_shared_table_on_their_operands_in_gate_order() {
    let gates = [
        (Gate::X { target: 2 }, vec![2]),
        (Gate::H { target: 2 }, vec![2]),
        (Gate::S { target: 2 }, vec![2]),
        (Gate::Sx { target: 2 }, vec![2]),
        (
            Gate::Rx {
                theta: 0.4,
                target: 2,
            },
            vec![2],
        ),
        (
            Gate::Rz {
                theta: 0.4,
                target: 2,
            },
            vec![2],
        ),
        (
            Gate::Cnot {
                control: 3,
                target: 1,
            },
            vec![3, 1],
        ),
        (
            Gate::Cz {
                control: 3,
                target: 1,
            },
            vec![3, 1],
        ),
        (
            Gate::Rzz {
                theta: 0.4,
                q1: 3,
                q2: 1,
            },
            vec![3, 1],
        ),
    ];
    for (gate, modes) in gates {
        let operator = fixture_operator(gate).expect("valid gate");
        let expected = unitary_matrix(gate.into()).expect("tabulated gate");
        let expected: Vec<_> = expected
            .row_major()
            .iter()
            .copied()
            .map(Complex64Abi::from)
            .collect();
        assert_eq!(&*operator.modes, modes, "{gate:?}");
        assert_eq!(
            operator_bits(&operator.matrix),
            operator_bits(&expected),
            "{gate:?}"
        );
    }
}

#[test]
fn owned_operator_rejects_invalid_arity_modes_and_matrix_shape() {
    let zero = Complex64Abi::new(0.0, 0.0);
    let invalid = [
        OwnedOperator::new(vec![], vec![]),
        OwnedOperator::new(vec![0, 1, 2], vec![zero; 64]),
        OwnedOperator::new(vec![-1], vec![zero; 4]),
        OwnedOperator::new(vec![1, 1], vec![zero; 16]),
        OwnedOperator::new(vec![0], vec![zero; 3]),
        OwnedOperator::new(vec![0, 1], vec![zero; 15]),
    ];

    assert!(invalid.into_iter().all(|result| result.is_err()));
}

#[test]
fn successful_mps_execution_cleans_up_in_dependency_order() {
    let api = TestDoubleMpsExecutionApi::new([]);
    run(&api).expect("fake MPS execution should succeed");

    assert_eq!(
        api.events(),
        [
            Event::CreateState,
            Event::CreateWorkspace,
            Event::Allocate(0),
            Event::CopyToDevice(0),
            Event::ApplyUnitary(0),
            Event::Allocate(1),
            Event::CopyToDevice(1),
            Event::ApplyUnitary(1),
            Event::FinalizeMps,
            Event::ConfigureF64(0),
            Event::ConfigureF64(1),
            Event::ConfigureU32(0),
            Event::ConfigureU32(1),
            Event::ConfigureU32(2),
            Event::Allocate(2),
            Event::Allocate(3),
            Event::MemoryInfo,
            Event::PrepareState,
            Event::WorkspaceSize,
            Event::Allocate(4),
            Event::SetWorkspace,
            Event::ComputeState,
            Event::Synchronize(0),
            Event::CopyFromDevice(0),
            Event::CopyFromDevice(1),
            Event::Synchronize(1),
            Event::DestroyWorkspace,
            Event::DestroyState,
            Event::Free(0),
            Event::Free(1),
            Event::Free(2),
            Event::Free(3),
            Event::Free(4),
        ]
    );
    let mut expected_freed_handles = api.allocation_handles();
    expected_freed_handles.reverse();
    assert_eq!(api.freed_handles(), expected_freed_handles);
}

#[test]
fn sampler_materializes_mps_before_creation_and_retains_it_through_cleanup() {
    let api = TestDoubleMpsExecutionApi::new([]);
    let circuit = circuit();
    let mut mps_execution =
        new_mps_execution(&api, &circuit).expect("MPS execution should be created");
    let request =
        SamplingRequest::new(3, 8, Some(11), 29).expect("sampling request should be valid");

    let samples = mps_execution
        .sample_full_bitstrings(&circuit, request)
        .expect("sampling should succeed");
    assert_eq!(samples.shot(1), Some([1, 1].as_slice()));
    mps_execution
        .close()
        .expect("MPS execution cleanup should succeed");

    let events = api.events();
    let finalized = positions(&events, Event::FinalizeMps)[0];
    let prepared_state = positions(&events, Event::PrepareState)[0];
    let computed_state = positions(&events, Event::ComputeState)[0];
    let synchronized_state = positions(&events, Event::Synchronize(0))[0];
    let created_sampler = positions(&events, Event::CreateSampler)[0];
    let destroyed_sampler = positions(&events, Event::DestroySampler)[0];
    let destroyed_state = positions(&events, Event::DestroyState)[0];
    let first_free = positions(&events, Event::Free(0))[0];

    assert!(finalized < prepared_state);
    assert!(prepared_state < computed_state);
    assert!(computed_state < synchronized_state);
    assert!(synchronized_state < created_sampler);
    assert!(created_sampler < destroyed_sampler);
    assert!(destroyed_sampler < destroyed_state);
    assert!(destroyed_state < first_free);
}

#[test]
fn close_is_idempotent() {
    let api = TestDoubleMpsExecutionApi::new([]);
    let circuit = circuit();
    let mut mps_execution =
        new_mps_execution(&api, &circuit).expect("MPS execution should be created");

    mps_execution.close().expect("first close should succeed");
    let events_after_first_close = api.events();
    mps_execution.close().expect("second close should succeed");

    assert_eq!(api.events(), events_after_first_close);
}

#[test]
fn drop_after_close_performs_no_more_cleanup() {
    let api = TestDoubleMpsExecutionApi::new([]);
    let circuit = circuit();
    let mut mps_execution =
        new_mps_execution(&api, &circuit).expect("MPS execution should be created");

    mps_execution.close().expect("close should succeed");
    let events_after_close = api.events();
    drop(mps_execution);

    assert_eq!(api.events(), events_after_close);
}

#[test]
fn drop_without_close_performs_dependency_ordered_cleanup() {
    let api = TestDoubleMpsExecutionApi::new([]);
    let circuit = circuit();
    let mps_execution = new_mps_execution(&api, &circuit).expect("MPS execution should be created");

    drop(mps_execution);

    assert_eq!(
        api.events(),
        [
            Event::CreateState,
            Event::CreateWorkspace,
            Event::Synchronize(0),
            Event::DestroyWorkspace,
            Event::DestroyState,
        ]
    );
}

#[test]
fn repeated_mps_execution_balances_every_device_allocation() {
    let api = TestDoubleMpsExecutionApi::new([]);

    run(&api).expect("first MPS execution should succeed");
    run(&api).expect("second MPS execution should succeed without stale state");

    let mut expected_freed_handles = api.allocation_handles();
    expected_freed_handles[..5].reverse();
    expected_freed_handles[5..].reverse();
    assert_eq!(api.freed_handles(), expected_freed_handles);
    assert_eq!(api.allocation_handles().len(), 10);
    assert_eq!(api.freed_handles().len(), 10);
}

#[test]
fn metadata_only_execution_skips_output_transfer_and_reports_resources() {
    let api = TestDoubleMpsExecutionApi::new([]);
    let circuit = circuit();
    let mut mps_execution =
        new_mps_execution(&api, &circuit).expect("MPS execution should be created");

    let result = mps_execution
        .execute(&circuit, StateReadout::MetadataOnly)
        .expect("metadata-only execution should succeed");
    mps_execution.close().expect("cleanup should succeed");

    assert_eq!(result.amplitudes(), None);
    assert_eq!(result.report.target_extents, [vec![2, 2], vec![2, 2]]);
    assert_eq!(result.report.realized_extents, [vec![2, 2], vec![2, 2]]);
    assert_eq!(result.report.maximum_bond, 2);
    assert_eq!(result.report.policy, ExecutionPolicy::bell_regression());
    assert_eq!(result.report.workspace.total_bytes, 80 * 1024 * 1024);
    assert_eq!(result.report.workspace.free_before_bytes, 64 * 1024 * 1024);
    assert_eq!(
        result.report.workspace.requested_maximum_bytes,
        ExecutionPolicy::bell_regression().maximum_workspace_bytes
    );
    assert_eq!(result.report.workspace.native_recommended_bytes, 256);
    assert_eq!(result.report.workspace.allocated_bytes, 256);
    assert_eq!(result.report.workspace.free_after_cleanup_bytes, 0);
    assert_eq!(
        api.prepare_maximum_workspace_bytes(),
        [ExecutionPolicy::bell_regression().maximum_workspace_bytes]
    );
    assert_eq!(api.allocation_sizes().last(), Some(&256));
    assert!(
        api.events()
            .iter()
            .all(|event| !matches!(event, Event::CopyFromDevice(_)))
    );
}

#[test]
fn full_readout_returns_every_amplitude_in_little_endian_order() {
    let api = TestDoubleMpsExecutionApi::new([]);
    let circuit = circuit();
    let mut mps_execution =
        new_mps_execution(&api, &circuit).expect("MPS execution should be created");

    let result = mps_execution
        .execute(&circuit, StateReadout::FullAmplitudes)
        .expect("full readout should succeed");
    mps_execution.close().expect("cleanup should succeed");

    let zero = Complex64::new(0.0, 0.0);
    let scale = Complex64::new(std::f64::consts::FRAC_1_SQRT_2, 0.0);
    assert_eq!(result.amplitudes(), Some(&[scale, zero, zero, scale][..]));
    assert_eq!(
        api.events()
            .iter()
            .filter(|event| matches!(event, Event::CopyFromDevice(_)))
            .count(),
        2
    );
}

#[test]
fn query_uses_ordered_product_terms_and_separate_synchronized_lifecycle() {
    let api = TestDoubleMpsExecutionApi::new([]);
    let circuit = circuit();
    let mut mps_execution =
        new_mps_execution(&api, &circuit).expect("MPS execution should be created");
    mps_execution
        .execute(&circuit, StateReadout::MetadataOnly)
        .expect("state execution should succeed");

    let query = AdjacentZQuery::new(2).expect("Query should be valid");
    let result = mps_execution
        .execute_query(&query)
        .expect("Query should succeed");
    mps_execution.close().expect("cleanup should succeed");

    assert_eq!(result.raw_expectation, Complex64::new(6.0, 0.0));
    assert_eq!(result.squared_norm, Complex64::new(2.0, 0.0));
    assert_eq!(result.normalized_expectation, Complex64::new(3.0, 0.0));
    assert_eq!(result.hyper_samples, B2_EXPECTATION_HYPER_SAMPLES);
    let state = api.state.borrow();
    assert_eq!(state.appended_modes, [vec![vec![0], vec![1]]]);
    assert_eq!(state.appended_coefficients, [Complex64::new(1.0, 0.0)]);
    assert_eq!(
        state.expectation_hyper_samples,
        [B2_EXPECTATION_HYPER_SAMPLES]
    );
    drop(state);
    let events = api.events();
    let compute = events
        .iter()
        .position(|event| *event == Event::ComputeExpectation)
        .expect("expectation compute should occur");
    assert!(matches!(events[compute + 1], Event::Synchronize(_)));
    let query_workspace_destroy = events
        .iter()
        .position(|event| *event == Event::DestroyQueryWorkspace)
        .expect("Query workspace should be destroyed");
    let expectation_destroy = events
        .iter()
        .position(|event| *event == Event::DestroyExpectation)
        .expect("expectation should be destroyed");
    let operator_destroy = events
        .iter()
        .position(|event| *event == Event::DestroyNetworkOperator)
        .expect("operator should be destroyed");
    let state_destroy = events
        .iter()
        .position(|event| *event == Event::DestroyState)
        .expect("state should be destroyed");
    assert!(query_workspace_destroy < expectation_destroy);
    assert!(expectation_destroy < operator_destroy);
    assert!(operator_destroy < state_destroy);
}

fn pauli_sum(terms: &[(Complex64, &str, &[usize])]) -> PauliSum {
    let mut sum = PauliSum::new();
    for &(coefficient, labels, qubits) in terms {
        sum.push_labels(coefficient, labels, qubits)
            .expect("test Pauli terms should be valid");
    }
    sum
}

#[test]
fn pauli_query_uploads_one_tensor_per_kind_and_passes_modes_and_coefficients() {
    let api = TestDoubleMpsExecutionApi::new([]);
    let circuit = circuit();
    let mut mps_execution =
        new_mps_execution(&api, &circuit).expect("MPS execution should be created");
    mps_execution
        .execute(&circuit, StateReadout::MetadataOnly)
        .expect("state execution should succeed");
    let uploads_before = api.uploaded_tensors().len();

    let observable = pauli_sum(&[
        (Complex64::new(0.5, 0.0), "XY", &[0, 1]),
        (Complex64::new(0.0, -2.0), "ZX", &[1, 0]),
    ]);
    let result = mps_execution
        .execute_pauli_query(&observable)
        .expect("Query should succeed");
    mps_execution.close().expect("cleanup should succeed");

    assert_eq!(result.normalized_expectation, Complex64::new(3.0, 0.0));
    assert_eq!(
        api.uploaded_tensors()[uploads_before..],
        [Pauli::X, Pauli::Y, Pauli::Z].map(|pauli| pauli.matrix().map(Complex64Abi::from).to_vec())
    );
    let state = api.state.borrow();
    assert_eq!(
        state.appended_modes,
        [vec![vec![0], vec![1]], vec![vec![1], vec![0]]]
    );
    assert_eq!(
        state.appended_coefficients,
        [Complex64::new(0.5, 0.0), Complex64::new(0.0, -2.0)]
    );
    let [first, second] = state.appended_tensors.as_slice() else {
        panic!("expected two appended products");
    };
    assert_eq!(first[0], second[1], "both X factors share one tensor");
    assert_ne!(first[0], first[1]);
    assert_ne!(first[1], second[0]);
}

#[test]
fn pauli_query_rejects_an_absent_qubit_before_native_query_calls() {
    let api = TestDoubleMpsExecutionApi::new([]);
    let circuit = circuit();
    let mut mps_execution =
        new_mps_execution(&api, &circuit).expect("MPS execution should be created");
    mps_execution
        .execute(&circuit, StateReadout::MetadataOnly)
        .expect("state execution should succeed");

    let observable = pauli_sum(&[(Complex64::new(1.0, 0.0), "Z", &[2])]);
    let error = mps_execution
        .execute_pauli_query(&observable)
        .expect_err("qubit 2 is outside a two-qubit state");
    mps_execution.close().expect("cleanup should succeed");

    assert!(matches!(error, SimulationError::InvalidCircuit { .. }));
    assert!(!api.events().contains(&Event::CreateNetworkOperator));
}

#[test]
fn exact_observables_skip_mps_finalization_and_report_no_cost() {
    let api = TestDoubleMpsExecutionApi::new([])
        .with_expectations([(Complex64::new(2.0, 0.0), Complex64::new(2.0, 0.0))]);
    let circuit = circuit();
    let mut mps_execution =
        new_mps_execution(&api, &circuit).expect("MPS execution should be created");

    let observable = pauli_sum(&[(Complex64::new(1.0, 0.0), "ZZ", &[0, 1])]);
    let result = mps_execution
        .evaluate_observables(&circuit, &[observable], false)
        .expect("exact evaluation should succeed");
    mps_execution.close().expect("cleanup should succeed");

    assert_eq!(result.expectations, [Complex64::new(1.0, 0.0)]);
    assert_eq!(result.cost, None);
    let events = api.events();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::ApplyUnitary(_)))
            .count(),
        2
    );
    assert!(events.iter().all(|event| !matches!(
        event,
        Event::FinalizeMps
            | Event::ConfigureF64(_)
            | Event::ConfigureU32(_)
            | Event::PrepareState
            | Event::ComputeState
    )));
    assert!(events.contains(&Event::ComputeExpectation));
}

#[test]
fn mps_observables_add_identity_release_each_query_and_report_cost() {
    let api = TestDoubleMpsExecutionApi::new([]).with_expectations([
        (Complex64::new(1.2, 0.0), Complex64::new(2.0, 0.0)),
        (Complex64::new(-0.5, 0.0), Complex64::new(1.0, 0.0)),
    ]);
    let circuit = circuit();
    let mut mps_execution =
        new_mps_execution(&api, &circuit).expect("MPS execution should be created");

    let observables = [
        pauli_sum(&[
            (Complex64::new(0.25, 0.0), "II", &[0, 1]),
            (Complex64::new(1.0, 0.0), "ZZ", &[0, 1]),
        ]),
        pauli_sum(&[(Complex64::new(3.0, 0.0), "I", &[0])]),
        pauli_sum(&[(Complex64::new(1.0, 0.0), "X", &[0])]),
    ];
    let result = mps_execution
        .evaluate_observables(&circuit, &observables, true)
        .expect("MPS evaluation should succeed");
    let events_before_close = api.events();
    mps_execution.close().expect("cleanup should succeed");

    assert_eq!(
        result.expectations,
        [
            Complex64::new(0.85, 0.0),
            Complex64::new(3.0, 0.0),
            Complex64::new(-0.5, 0.0)
        ]
    );
    // The test double realizes a two-site chain with extents [2, 2] per site.
    assert_eq!(
        result.cost,
        Some(MpsCost {
            max_bond_dimension: 2,
            state_bytes: 8 * size_of::<Complex64Abi>(),
            workspace_bytes: 256,
        })
    );
    assert_eq!(
        positions(&events_before_close, Event::ComputeExpectation).len(),
        2,
        "an identity-only observable needs no native Query"
    );
    let operator_creations = positions(&events_before_close, Event::CreateNetworkOperator);
    let operator_destructions = positions(&events_before_close, Event::DestroyNetworkOperator);
    assert_eq!(operator_destructions.len(), 2);
    assert!(operator_destructions[0] < operator_creations[1]);
    let frees_before_second_query = events_before_close[..operator_creations[1]]
        .iter()
        .filter(|event| matches!(event, Event::Free(_)))
        .count();
    assert_eq!(
        frees_before_second_query, 2,
        "the first Query's Z tensor and workspace are freed before the next Query"
    );
    let mut allocated = api.allocation_handles();
    let mut freed = api.freed_handles();
    allocated.sort_unstable();
    freed.sort_unstable();
    assert_eq!(allocated, freed);
}

#[test]
fn branch_materializes_and_captures_each_live_state_before_dependent_work() {
    let api = branch_api([], 0.8, 0.2);
    let result =
        run_branch(&api, SelectedBranch::Zero).expect("branch MPS execution should succeed");

    assert!((result.report.masses.q0 - 0.8).abs() <= f64::EPSILON);
    assert!((result.report.masses.q1 - 0.2).abs() <= f64::EPSILON);
    assert!((result.report.masses.norm - 1.0).abs() <= f64::EPSILON);
    assert!((result.report.probability - 0.8).abs() <= f64::EPSILON);
    assert!((result.report.log_probability - 0.8_f64.ln()).abs() <= f64::EPSILON);
    assert!(result.initial_state.amplitudes().is_some());
    assert!(result.post_projection_state.amplitudes().is_some());
    assert!(result.continuation_state.amplitudes().is_some());
    assert_eq!(result.query.raw_expectation, Complex64::new(0.25, 0.0));

    let events = api.events();
    let state_computes = positions(&events, Event::ComputeState);
    let expectation_computes = positions(&events, Event::ComputeExpectation);
    let property_workspace_destroys = positions(&events, Event::DestroyQueryWorkspace);
    let expectation_destroys = positions(&events, Event::DestroyExpectation);
    let operator_destroys = positions(&events, Event::DestroyNetworkOperator);
    let network_operator_creates = positions(&events, Event::CreateNetworkOperator);
    let projection = positions(&events, Event::ApplyProjection)[0];
    let first_continuation = positions(&events, Event::ApplyUnitary(2))[0];

    assert_eq!(state_computes.len(), 3);
    assert_eq!(expectation_computes.len(), 3);
    assert!(state_computes[0] < positions(&events, Event::Synchronize(0))[0]);
    assert!(
        positions(&events, Event::Synchronize(0))[0] < positions(&events, Event::CaptureMps(0))[0]
    );
    let first_capture = positions(&events, Event::CaptureMps(0))[0];
    assert!(first_capture < expectation_computes[0]);
    assert!(expectation_computes[0] < expectation_computes[1]);
    assert!(expectation_computes[1] < projection);
    assert!(
        events[first_capture + 1..projection]
            .iter()
            .all(|event| !matches!(event, Event::ApplyUnitary(_) | Event::ApplyProjection))
    );
    for index in 0..2 {
        assert!(property_workspace_destroys[index] < expectation_destroys[index]);
        assert!(expectation_destroys[index] < operator_destroys[index]);
        assert!(operator_destroys[index] < projection);
    }
    assert!(operator_destroys[0] < network_operator_creates[1]);
    assert!(projection < state_computes[1]);
    assert!(state_computes[1] < positions(&events, Event::Synchronize(3))[0]);
    assert!(
        positions(&events, Event::Synchronize(3))[0] < positions(&events, Event::CaptureMps(1))[0]
    );
    assert!(positions(&events, Event::CaptureMps(1))[0] < first_continuation);
    assert!(first_continuation < state_computes[2]);
    assert!(state_computes[2] < positions(&events, Event::Synchronize(4))[0]);
    assert!(positions(&events, Event::Synchronize(4))[0] < network_operator_creates[2]);

    let uploads = api.uploaded_tensors();
    assert_eq!(
        uploads[2],
        [
            Complex64Abi::new(1.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
        ]
    );
    assert_eq!(
        uploads[3],
        [
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(1.0, 0.0),
        ]
    );
    assert_eq!(
        uploads[4],
        [
            Complex64Abi::new(1.0 / 0.8_f64.sqrt(), 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
        ]
    );
}

#[test]
fn zero_selected_mass_fails_before_projection_allocation_or_mutation() {
    let api = branch_api([], 0.0, 1.0);
    let error =
        run_branch(&api, SelectedBranch::Zero).expect_err("zero-mass selected branch should fail");

    assert!(matches!(error, SimulationError::InvalidCircuit { .. }));
    let events = api.events();
    assert_eq!(positions(&events, Event::ComputeExpectation).len(), 2);
    assert!(!events.contains(&Event::ApplyProjection));
    let second_operator_destroy = positions(&events, Event::DestroyNetworkOperator)[1];
    assert!(
        events[second_operator_destroy + 1..]
            .iter()
            .all(|event| !matches!(event, Event::Allocate(_) | Event::CopyToDevice(_)))
    );
}

#[test]
fn one_branch_projection_is_normalized_on_the_selected_diagonal() {
    let api = branch_api([], 0.8, 0.2);
    let result =
        run_branch(&api, SelectedBranch::One).expect("one-branch MPS execution should succeed");

    assert!((result.report.probability - 0.2).abs() <= f64::EPSILON);
    assert_eq!(
        api.uploaded_tensors()[4],
        [
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(0.0, 0.0),
            Complex64Abi::new(1.0 / 0.2_f64.sqrt(), 0.0),
        ]
    );
}

#[test]
fn branch_barrier_failures_stop_the_next_semantic_stage() {
    let cases = [
        (Event::Synchronize(0), Event::CaptureMps(0)),
        (Event::Synchronize(1), Event::CreateNetworkOperator),
        (Event::Synchronize(2), Event::ApplyProjection),
        (Event::Synchronize(3), Event::CaptureMps(1)),
        (Event::Synchronize(4), Event::CreateNetworkOperator),
    ];

    for (failure, forbidden) in cases {
        let api = branch_api([failure], 0.8, 0.2);
        let error = run_branch(&api, SelectedBranch::Zero)
            .expect_err("injected barrier failure should stop branch MPS execution");
        assert_native_operation(&error, "synchronize_stream");
        let events = api.events();
        let failure_position = positions(&events, failure)[0];
        assert!(!events[failure_position + 1..].contains(&forbidden));
    }
}

#[test]
fn capture_failures_surface_before_properties_or_continuation() {
    for (failure, forbidden) in [
        (Event::CaptureMps(0), Event::CreateNetworkOperator),
        (Event::CaptureMps(1), Event::ApplyUnitary(2)),
    ] {
        let api = branch_api([failure], 0.8, 0.2);
        let error = run_branch(&api, SelectedBranch::Zero)
            .expect_err("injected capture failure should stop branch MPS execution");
        assert_native_operation(&error, "capture_mps");
        let events = api.events();
        let failure_position = positions(&events, failure)[0];
        assert!(!events[failure_position + 1..].contains(&forbidden));
    }
}

#[test]
fn branch_execution_and_outer_cleanup_errors_are_both_preserved() {
    let api = branch_api([Event::CaptureMps(0), Event::DestroyWorkspace], 0.8, 0.2);
    let error =
        run_branch(&api, SelectedBranch::Zero).expect_err("capture and cleanup should both fail");

    let SimulationError::ExecutionAndCleanupFailed { execution, cleanup } = error else {
        panic!("expected combined failure, got {error:?}");
    };
    assert_native_operation(&execution, "capture_mps");
    assert_native_operation(&cleanup, "destroy_workspace");
    let events = api.events();
    assert!(events.contains(&Event::DestroyState));
    assert!(matches!(events.last(), Some(Event::Free(_))));
}

#[test]
fn property_transaction_preserves_execution_and_first_cleanup_error() {
    let api = branch_api(
        [
            Event::ComputeExpectation,
            Event::DestroyQueryWorkspace,
            Event::DestroyExpectation,
        ],
        0.8,
        0.2,
    );
    let error = run_branch(&api, SelectedBranch::Zero)
        .expect_err("property execution and cleanup should fail");

    let SimulationError::ExecutionAndCleanupFailed { execution, cleanup } = error else {
        panic!("expected combined property failure, got {error:?}");
    };
    assert_native_operation(&execution, "compute_expectation");
    assert_native_operation(&cleanup, "destroy_query_workspace");
    let events = api.events();
    let workspace_destroy = positions(&events, Event::DestroyQueryWorkspace)[0];
    let expectation_destroy = positions(&events, Event::DestroyExpectation)[0];
    let operator_destroy = positions(&events, Event::DestroyNetworkOperator)[0];
    assert!(workspace_destroy < expectation_destroy);
    assert!(expectation_destroy < operator_destroy);
    assert!(events.contains(&Event::DestroyState));
}

fn run_query(api: &TestDoubleMpsExecutionApi) -> Result<(), SimulationError> {
    let circuit = circuit();
    let mut mps_execution = new_mps_execution(api, &circuit)?;
    mps_execution.execute(&circuit, StateReadout::MetadataOnly)?;
    let query = AdjacentZQuery::new(2).expect("Query should be valid");
    let execution = mps_execution.execute_query(&query).map(|_| ());
    let cleanup = mps_execution.close();
    combine_execution_and_cleanup(execution, cleanup)
}

#[test]
fn every_query_call_is_fallible_and_cleans_up_owned_resources() {
    let failure_points = [
        Event::CreateNetworkOperator,
        Event::Allocate(5),
        Event::CopyToDevice(2),
        Event::AppendProduct(0),
        Event::CreateExpectation,
        Event::ConfigureExpectation,
        Event::CreateQueryWorkspace,
        Event::QueryMemoryInfo,
        Event::PrepareExpectation,
        Event::QueryWorkspaceSize,
        Event::Allocate(6),
        Event::QuerySetWorkspace,
        Event::ComputeExpectation,
        Event::Synchronize(1),
    ];

    for failure in failure_points {
        let api = TestDoubleMpsExecutionApi::new([failure]);
        let error = run_query(&api).expect_err("the selected Query stage should fail");
        assert_native_operation(&error, failure.operation());
        let events = api.events();
        assert!(events.contains(&failure));
        assert!(events.contains(&Event::DestroyState));
        if failure != Event::CreateNetworkOperator {
            assert!(events.contains(&Event::DestroyNetworkOperator));
        }
        if matches!(
            failure,
            Event::ConfigureExpectation
                | Event::CreateQueryWorkspace
                | Event::QueryMemoryInfo
                | Event::PrepareExpectation
                | Event::QueryWorkspaceSize
                | Event::Allocate(6)
                | Event::QuerySetWorkspace
                | Event::ComputeExpectation
                | Event::Synchronize(1)
        ) {
            assert!(events.contains(&Event::DestroyExpectation));
        }
    }
}

#[test]
fn simultaneous_query_execution_and_cleanup_failures_are_both_retained() {
    let api =
        TestDoubleMpsExecutionApi::new([Event::ComputeExpectation, Event::DestroyExpectation]);
    let error = run_query(&api).expect_err("Query execution and cleanup should both fail");

    let SimulationError::ExecutionAndCleanupFailed { execution, cleanup } = error else {
        panic!("expected combined failure, got {error:?}");
    };
    assert_native_operation(&execution, "compute_expectation");
    assert_native_operation(&cleanup, "destroy_expectation");
    let events = api.events();
    assert!(events.contains(&Event::DestroyNetworkOperator));
    assert!(events.contains(&Event::DestroyState));
}

#[test]
fn deferred_compute_or_sync_failure_prevents_output_transfer() {
    for failure in [Event::ComputeState, Event::Synchronize(0)] {
        let api = TestDoubleMpsExecutionApi::new([failure]);
        let error = run(&api).expect_err("execution should fail before output transfer");

        assert_native_operation(&error, failure.operation());
        assert!(
            api.events()
                .iter()
                .all(|event| !matches!(event, Event::CopyFromDevice(_)))
        );
    }
}

#[test]
fn workspace_recommendation_above_policy_fails_before_scratch_allocation() {
    let policy = ExecutionPolicy::bell_regression();
    let workspace_size = i64::try_from(policy.maximum_workspace_bytes + 1)
        .expect("approved workspace ceiling should fit i64");
    let api = TestDoubleMpsExecutionApi::new([]).with_workspace(usize::MAX, workspace_size);
    let circuit = circuit();
    let mut mps_execution =
        new_mps_execution(&api, &circuit).expect("MPS execution should be created");

    let error = mps_execution
        .execute(&circuit, StateReadout::MetadataOnly)
        .expect_err("workspace recommendation should exceed policy");
    mps_execution.close().expect("cleanup should succeed");

    assert!(matches!(
        error,
        SimulationError::WorkspaceLimitExceeded {
            required,
            maximum,
        } if required == policy.maximum_workspace_bytes + 1
            && maximum == policy.maximum_workspace_bytes
    ));
    assert!(!api.events().contains(&Event::Allocate(4)));
    assert!(!api.events().contains(&Event::SetWorkspace));
}

#[test]
fn workspace_recommendation_above_free_memory_fails_before_scratch_allocation() {
    let api = TestDoubleMpsExecutionApi::new([]).with_workspace(255, 256);
    let circuit = circuit();
    let mut mps_execution =
        new_mps_execution(&api, &circuit).expect("MPS execution should be created");

    let error = mps_execution
        .execute(&circuit, StateReadout::MetadataOnly)
        .expect_err("workspace recommendation should exceed free memory");
    mps_execution.close().expect("cleanup should succeed");

    assert!(matches!(
        error,
        SimulationError::WorkspaceLimitExceeded {
            required: 256,
            maximum: 255,
        }
    ));
    assert!(!api.events().contains(&Event::Allocate(4)));
    assert!(!api.events().contains(&Event::SetWorkspace));
}

#[test]
fn nonpositive_workspace_recommendation_fails_before_scratch_allocation() {
    for workspace_size in [0, -1] {
        let api = TestDoubleMpsExecutionApi::new([]).with_workspace(usize::MAX, workspace_size);
        let circuit = circuit();
        let mut mps_execution =
            new_mps_execution(&api, &circuit).expect("MPS execution should be created");

        let error = mps_execution
            .execute(&circuit, StateReadout::MetadataOnly)
            .expect_err("nonpositive workspace recommendation should fail");
        mps_execution.close().expect("cleanup should succeed");

        assert!(matches!(error, SimulationError::InvalidNativeResult { .. }));
        assert!(!api.events().contains(&Event::Allocate(4)));
        assert!(!api.events().contains(&Event::SetWorkspace));
    }
}

#[test]
fn every_construction_and_execution_call_is_fallible_and_cleans_up() {
    let failure_points = [
        Event::CreateState,
        Event::CreateWorkspace,
        Event::Allocate(0),
        Event::CopyToDevice(0),
        Event::ApplyUnitary(0),
        Event::Allocate(1),
        Event::CopyToDevice(1),
        Event::ApplyUnitary(1),
        Event::FinalizeMps,
        Event::ConfigureF64(0),
        Event::ConfigureF64(1),
        Event::ConfigureU32(0),
        Event::ConfigureU32(1),
        Event::ConfigureU32(2),
        Event::Allocate(2),
        Event::Allocate(3),
        Event::MemoryInfo,
        Event::PrepareState,
        Event::WorkspaceSize,
        Event::Allocate(4),
        Event::SetWorkspace,
        Event::ComputeState,
        Event::Synchronize(0),
        Event::CopyFromDevice(0),
        Event::CopyFromDevice(1),
    ];

    for failure in failure_points {
        let api = TestDoubleMpsExecutionApi::new([failure]);
        let error = run(&api).expect_err("the selected MPS execution call should fail");
        assert_native_operation(&error, failure.operation());
        let events = api.events();
        assert!(events.contains(&failure));
        if failure != Event::CreateState {
            assert!(events.contains(&Event::DestroyState));
        }
        if failure != Event::CreateState && failure != Event::CreateWorkspace {
            assert!(events.contains(&Event::DestroyWorkspace));
        }
    }
}

#[test]
fn every_cleanup_call_is_attempted_after_a_failure() {
    let failure_points = [
        Event::Synchronize(1),
        Event::DestroyWorkspace,
        Event::DestroyState,
        Event::Free(0),
        Event::Free(1),
        Event::Free(2),
        Event::Free(3),
        Event::Free(4),
    ];

    for failure in failure_points {
        let api = TestDoubleMpsExecutionApi::new([failure]);
        let error = run(&api).expect_err("the selected cleanup call should fail");
        assert_native_operation(&error, failure.operation());
        let events = api.events();
        assert!(events.contains(&Event::DestroyWorkspace));
        assert!(events.contains(&Event::DestroyState));
        assert!(events.contains(&Event::Free(4)));
    }
}

#[test]
fn simultaneous_execution_and_cleanup_failures_are_both_retained() {
    let api = TestDoubleMpsExecutionApi::new([Event::ComputeState, Event::Synchronize(0)]);
    let error = run(&api).expect_err("execution and cleanup should both fail");

    let SimulationError::ExecutionAndCleanupFailed { execution, cleanup } = error else {
        panic!("expected combined failure, got {error:?}");
    };
    assert_native_operation(&execution, "compute_state");
    assert_native_operation(&cleanup, "synchronize_stream");
    let events = api.events();
    assert!(events.contains(&Event::DestroyWorkspace));
    assert!(events.contains(&Event::DestroyState));
    assert!(events.contains(&Event::Free(4)));
}

#[test]
fn simultaneous_construction_and_cleanup_failures_are_both_retained() {
    let api = TestDoubleMpsExecutionApi::new([Event::CreateWorkspace, Event::Synchronize(0)]);
    let error = run(&api).expect_err("construction and cleanup should both fail");

    let SimulationError::ExecutionAndCleanupFailed { execution, cleanup } = error else {
        panic!("expected combined failure, got {error:?}");
    };
    assert_native_operation(&execution, "create_workspace");
    assert_native_operation(&cleanup, "synchronize_stream");
    assert_eq!(
        api.events(),
        [
            Event::CreateState,
            Event::CreateWorkspace,
            Event::Synchronize(0),
            Event::DestroyState,
        ]
    );
}

#[test]
fn probability_applies_the_shared_table_in_order_and_reads_the_identity_norm() {
    let api = probability_api([]);

    let result = run_probability(&api, &bell_outcomes()).expect("Probability should succeed");

    let expected = [
        (
            vec![0],
            true,
            unitary_matrix(UnitaryOperation::H { target: 0 }),
        ),
        (
            vec![0, 1],
            true,
            unitary_matrix(UnitaryOperation::Cx {
                control: 0,
                target: 1,
            }),
        ),
        (vec![0], false, Some(basis_operator(false, false))),
        (vec![1], false, Some(basis_operator(false, true))),
    ];
    let applied = api.applied_operators();
    assert_eq!(applied.len(), expected.len());
    for (applied, (modes, unitary, matrix)) in applied.iter().zip(expected) {
        let matrix = table_values(&matrix.expect("tabulated operator"));
        assert_eq!(applied.modes, modes);
        assert_eq!(applied.unitary, unitary);
        assert_eq!(operator_bits(&applied.matrix), operator_bits(&matrix));
    }
    // The norm is the expectation of one identity factor on mode 0.
    assert_eq!(api.appended_modes(), [vec![vec![0]]]);
    let one = Complex64Abi::new(1.0, 0.0);
    let zero = Complex64Abi::new(0.0, 0.0);
    assert_eq!(
        operator_bits(&api.device_tensor(api.appended_tensors()[0][0])),
        operator_bits(&[one, zero, zero, one])
    );
    // The test double realizes a two-site chain with extents [2, 2] per site.
    assert_eq!(
        result,
        ProbabilityResult {
            probability: 0.5,
            cost: MpsCost {
                max_bond_dimension: 2,
                state_bytes: 8 * size_of::<Complex64Abi>(),
                workspace_bytes: 256,
            },
        }
    );
}

#[test]
fn probability_truncates_relatively_without_renormalizing() {
    let api = probability_api([]);
    run_probability(&api, &bell_outcomes()).expect("Probability should succeed");

    let policy = ExecutionPolicy::bell_regression();
    assert_eq!(
        api.configured_f64(),
        [
            (StateF64Attribute::SvdAbsoluteCutoff, 0.0),
            (StateF64Attribute::SvdRelativeCutoff, policy.relative_cutoff),
        ]
    );
    assert_eq!(
        api.configured_u32(),
        [
            StateU32Configuration::SvdNormalizationNone,
            StateU32Configuration::SvdAlgorithmGesvd,
            StateU32Configuration::MpsGaugeSimple,
        ]
    );
}

#[test]
fn state_executions_keep_the_policy_cutoffs_and_pin_no_normalization() {
    let api = TestDoubleMpsExecutionApi::new([]);
    run(&api).expect("MPS execution should succeed");

    let policy = ExecutionPolicy::bell_regression();
    assert_eq!(
        api.configured_f64(),
        [
            (StateF64Attribute::SvdAbsoluteCutoff, policy.absolute_cutoff),
            (StateF64Attribute::SvdRelativeCutoff, policy.relative_cutoff),
        ]
    );
    assert_eq!(
        api.configured_u32()[0],
        StateU32Configuration::SvdNormalizationNone
    );
}

#[test]
fn probability_is_returned_without_renormalization_including_zero() {
    for probability in [2.0_f64.powi(-40), 0.0] {
        let norm = Complex64::new(probability, 0.0);
        let api = TestDoubleMpsExecutionApi::new([]).with_expectations([(norm, norm)]);

        let result = run_probability(&api, &bell_outcomes()).expect("Probability should succeed");

        assert_eq!(result.probability.to_bits(), probability.to_bits());
    }
}

#[test]
fn probability_cost_reports_the_larger_of_state_and_norm_workspaces() {
    for sizes in [[256, 1024], [1024, 256]] {
        let api = probability_api([]).with_workspace_sizes(sizes);

        let result = run_probability(&api, &bell_outcomes()).expect("Probability should succeed");

        assert_eq!(result.cost.workspace_bytes, 1024, "{sizes:?}");
    }
}

#[test]
fn invalid_probability_norms_are_rejected() {
    let real = |value: f64| Complex64::new(value, 0.0);
    let invalid = [
        (real(f64::NAN), real(f64::NAN)),
        (real(0.5), real(f64::INFINITY)),
        (real(0.5), Complex64::new(0.5, 1.0e-6)),
        (real(-0.25), real(-0.25)),
        (real(0.5), real(0.25)),
        (Complex64::new(0.5, 1.0e-6), real(0.5)),
    ];
    for (value, squared_norm) in invalid {
        assert!(
            matches!(
                unnormalized_probability(value, squared_norm),
                Err(SimulationError::InvalidNativeResult { .. })
            ),
            "{value} / {squared_norm}"
        );
    }
    // Round-off far below the relative tolerance is accepted, even at 2⁻⁸⁰.
    let norm = 2.0_f64.powi(-80);
    assert_eq!(
        unnormalized_probability(
            Complex64::new(norm * (1.0 + 1.0e-14), 0.0),
            Complex64::new(norm, norm * 1.0e-14)
        )
        .expect("round-off should be accepted")
        .to_bits(),
        norm.to_bits()
    );
}

#[test]
fn every_probability_call_is_fallible_and_releases_every_allocation() {
    let failure_points = [
        Event::ApplyUnitary(0),
        Event::ApplyProjection,
        Event::FinalizeMps,
        Event::ConfigureU32(0),
        Event::ComputeState,
        Event::CreateNetworkOperator,
        Event::ComputeExpectation,
        Event::Synchronize(1),
    ];

    for failure in failure_points {
        let api = probability_api([failure]);
        let error = run_probability(&api, &bell_outcomes())
            .expect_err("the selected Probability call should fail");
        assert_native_operation(&error, failure.operation());
        let events = api.events();
        assert!(events.contains(&Event::DestroyState), "{failure:?}");
        assert!(events.contains(&Event::DestroyWorkspace), "{failure:?}");
        let mut allocated = api.allocation_handles();
        let mut freed = api.freed_handles();
        allocated.sort_unstable();
        freed.sort_unstable();
        assert_eq!(allocated, freed, "{failure:?}");
    }
}

#[test]
fn a_program_of_another_width_is_rejected_before_native_work() {
    let wider = projected(
        3,
        vec![FixedOutcomeOperation::Unitary(UnitaryOperation::H {
            target: 2,
        })],
    );
    let mut wider_circuit = Circuit::new(3).expect("three-qubit circuit should be valid");
    wider_circuit
        .push(Gate::H { target: 2 })
        .expect("in-range gate should be valid");
    let api = TestDoubleMpsExecutionApi::new([]);
    let mut mps_execution =
        new_mps_execution(&api, &circuit()).expect("MPS execution should be created");

    let probability = mps_execution.evaluate_probability(&wider).map(|_| ());
    let execution = mps_execution
        .execute(&wider_circuit, StateReadout::MetadataOnly)
        .map(|_| ());
    assert_eq!(api.events(), [Event::CreateState, Event::CreateWorkspace]);
    mps_execution.close().expect("cleanup should succeed");

    for error in [probability, execution] {
        assert!(
            matches!(
                &error,
                Err(SimulationError::InvalidCircuit { reason })
                    if reason == "a 3-qubit program does not match the 2-qubit native state"
            ),
            "{error:?}"
        );
    }
}

#[test]
fn a_branch_continuation_of_another_width_is_rejected_before_native_work() {
    let api = branch_api([], 0.8, 0.2);
    let initial = circuit();
    let continuation = Circuit::new(3).expect("three-qubit continuation should be valid");
    let query = AdjacentZQuery::new(2).expect("Query should be valid");
    let mut mps_execution =
        new_mps_execution(&api, &initial).expect("MPS execution should be created");

    let error = mps_execution
        .execute_branch(
            &initial,
            BranchRequest {
                mode: 0,
                selected: SelectedBranch::Zero,
            },
            &continuation,
            &query,
        )
        .expect_err("the continuation width differs from the state");
    assert_eq!(api.events(), [Event::CreateState, Event::CreateWorkspace]);
    mps_execution.close().expect("cleanup should succeed");

    assert!(matches!(
        error,
        SimulationError::InvalidCircuit { reason }
            if reason == "a 3-qubit program does not match the 2-qubit native state"
    ));
}

#[test]
fn an_operator_mode_outside_the_state_is_rejected_before_allocation() {
    let api = TestDoubleMpsExecutionApi::new([]);
    let mut mps_execution =
        new_mps_execution(&api, &circuit()).expect("MPS execution should be created");
    let operator = OwnedOperator::new(vec![2], vec![Complex64Abi::new(1.0, 0.0); 4])
        .expect("the operator itself is well formed");

    let error = mps_execution
        .register_operator(operator, false, &mut StatePhaseTimings::default())
        .expect_err("mode 2 is outside a two-qubit state");
    assert_eq!(api.events(), [Event::CreateState, Event::CreateWorkspace]);
    mps_execution.close().expect("cleanup should succeed");

    assert!(matches!(
        error,
        SimulationError::InvalidCircuit { reason }
            if reason == "operator mode 2 is outside the 2-qubit native state"
    ));
}
