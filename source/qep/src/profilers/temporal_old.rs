// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    Profiler, ProfilerResult, TotalCounts, event_recorder::EventRecorder, factories::TFactory,
};
use crate::{
    Parameter, QuantumBackendResult,
    interface::{BatchResult, QuantumBackend, QuantumBackendInstruction},
    profilers::{
        InstructionTrace,
        profile_types::{LoggedInstruction, LoggedInstructionParameter},
    },
};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
};

/// Configuration for temporal quantum profiling.
#[derive(Clone, Copy)]
pub struct TemporalProfilerConfig {
    /// Duration of a Clifford instruction in nanoseconds.
    pub clifford_gate_time_nanos: usize,
    /// Duration of a measurement or reset in nanoseconds.
    pub measurment_time_nanos: usize,
    pub aggregation_window_nanos: usize,
    pub t_factories_count: usize,
    /// Seed for temporal-profiler distillation and measurement randomness.
    pub seed: Option<u64>,
    /// Records temporal instruction and factory-round intervals when true.
    /// Disabling this leaves metrics unchanged and returns no execution trace.
    pub record_trace: bool,

    /// Distance of the error-coorecting code.
    /// Assumes that all logical qubits have the same error correcting code.
    /// TODO: change it to specify different ECCs for different qubits.
    pub ecc_distance: usize,
}

impl Default for TemporalProfilerConfig {
    /// Uses ten factories, one-second windows, unseeded randomness, and no trace recording.
    fn default() -> Self {
        Self {
            clifford_gate_time_nanos: 5_000,
            measurment_time_nanos: 1_000,
            aggregation_window_nanos: 1_000_000_000,
            t_factories_count: 10,
            seed: None,
            record_trace: false,
            ecc_distance: 5,
        }
    }
}

/// Returns a coarse logical-instruction duration, in nanoseconds.
///
/// Litinski, arXiv:1808.02892, Section 1, defines a logical time step as roughly
/// d surface-code cycles; Section 4 assumes a 1 us physical code cycle. For this
/// distance-5 demo, one logical step is therefore `5 * 1_000 = 5_000` ns.
/// These are the default timings; callers can override them in `TemporalProfilerConfig`.
/// Durations do not automatically scale with `TemporalProfilerConfig::ecc_distance`.
///
/// We charge every Clifford one logical step. Actual costs differ: Pauli gates
/// can be tracked classically, and lattice-surgery CNOTs can require several
/// stages. Destructive logical readout need not take d rounds (Section 1);
/// the default allows one physical code cycle for each measurement or reset,
/// rather than modeling their separate hardware and decoding latencies.
fn instruction_time_nanos(
    instruction: &QuantumBackendInstruction,
    config: &TemporalProfilerConfig,
) -> usize {
    match instruction.instruction_name {
        "cx" | "h" | "s" | "sadj" | "x" | "y" | "z" => config.clifford_gate_time_nanos,
        "mz" | "reset" => config.measurment_time_nanos,
        // Explicit injection: CNOT + measurement + worst-case conditional S,
        // with CNOT and S each charged one Clifford step: 11_000 ns by default. We omit
        // extra classical feedforward latency and assume a distilled state is
        // already available; the scheduler accounts for factory waiting.
        // This is not Litinski's optimized fast-block consumption time of one
        // logical step, which tracks or avoids the separate correction.
        "inject_t" | "inject_tadj" => config
            .clifford_gate_time_nanos
            .checked_mul(2)
            .and_then(|time| time.checked_add(config.measurment_time_nanos))
            .expect("T injection duration overflow"),

        "t" | "tadj" => panic!("These should be implemented by inject gates"),
        _ => panic!("Unsupported temporal-profiler instruction: {instruction:?}"),
    }
}

/// Identifies a resource produced or consumed by the simulated backend.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum ResourceType {
    TState,
}

/// An event that changes the state of the simulated backend.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Event {
    InstructionFinished(InstructionRef),
    DistillationRoundFinished(FactoryId),
}

/// Stable reference to an instruction across all received batches.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct InstructionRef(usize);

/// Stable identifier for a distillation factory.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct FactoryId(usize);

/// Scheduling state for a logical qubit.
#[derive(Default)]
struct LogicalQubit {
    /// Latest instruction touching this qubit.
    last_instruction: Option<InstructionRef>,
}

/// Mutable scheduling data retained for an instruction until it can be retired.
struct InstructionState {
    instruction: QuantumBackendInstruction,
    done: bool,
    started: bool,
    pending_dependencies: usize,
    dependents: Vec<InstructionRef>,
    /// Resource type consumed at start, requiring exactly one unit when present.
    resource_requirements: Option<ResourceType>,
    waiting_on_resource: Option<ResourceType>,
    result_index: Option<usize>,
}

impl InstructionState {
    /// Creates an instruction state with no dependencies or scheduled work.
    fn new(instruction: QuantumBackendInstruction, result_index: Option<usize>) -> Self {
        let resource_requirements = match instruction.instruction_name {
            "t" | "tadj" => Some(ResourceType::TState),
            _ => None,
        };

        Self {
            instruction,
            done: false,
            started: false,
            pending_dependencies: 0,
            dependents: Vec::new(),
            resource_requirements,
            waiting_on_resource: None,
            result_index,
        }
    }
}

/// Simulates a minimal Clifford+T instruction set with per-qubit dependencies.
///
/// Each instruction depends on the latest earlier operation touching each target qubit;
/// independent instructions run concurrently, and completion can unblock multiple successors.
/// Per-qubit dependencies persist across batches. Use ResultAdapter outside a
/// TranspilingAdapter for result registers and non-native instructions.
///
/// A simulated event clock schedules instruction completions and factory distillation rounds.
/// Events at the same timestamp are handled in insertion order. Instruction durations use
/// the configured logical timing model. Each factory holds at most one T-state and restarts after a failed
/// round or after the injection consuming its state completes.
///
/// Distillation and measurement randomness share a seedable RNG. Instruction and round intervals
/// are optionally recorded in the trace and always aggregated into operation counts and rate profiles. Instructions
/// with no resource dependency may start while distillation events remain scheduled, and each batch
/// runs the event loop only until all requested results are available.
pub struct TemporalProfilerOld {
    /// Configuration used for instruction durations.
    config: TemporalProfilerConfig,
    /// Logical-qubit scheduling state, indexed by qubit ID.
    qubits: Vec<LogicalQubit>,
    /// Factories.
    factories: Vec<TFactory>,
    /// Measurement and readout results for the batch being executed.
    current_batch_result: BatchResult,
    /// Instructions whose results must be available before returning the current batch.
    pending_results: FxHashSet<InstructionRef>,
    /// Events are ordered by time and then by insertion sequence.
    events_queue: BinaryHeap<Reverse<(usize, u64, Event)>>,
    /// Sequence number assigned to the next event to break timestamp ties.
    next_event_sequence: u64,
    /// Current simulated time in nanoseconds.
    current_time_nanos: usize,
    /// Factories holding a ready state, grouped by resource type.
    available_resources: FxHashMap<ResourceType, VecDeque<FactoryId>>,
    /// Shared randomness for distillation outcomes and measurement results.
    rng: StdRng,
    /// FIFO queues of instructions waiting for each resource type.
    instructions_blocked_on_resources: FxHashMap<ResourceType, VecDeque<InstructionRef>>,
    /// Scheduling states retained until completed instructions can be retired.
    all_instructions: VecDeque<InstructionState>,
    /// Global instruction index corresponding to the front of `all_instructions`.
    all_instructions_offset: usize,
    /// Retains execution traces and aggregates event metrics.
    event_recorder: EventRecorder,
}

impl TemporalProfilerOld {
    /// Creates a profiler and starts a distillation round on every factory.
    ///
    /// # Panics
    ///
    /// Panics if there are no T factories or the aggregation window is zero.
    #[must_use]
    pub fn new(config: TemporalProfilerConfig) -> Self {
        assert!(
            config.t_factories_count > 0,
            "at least one T factory is required"
        );
        let mut profiler = Self {
            config,
            qubits: Vec::new(),
            factories: vec![TFactory::new(config.ecc_distance); config.t_factories_count],
            current_batch_result: BatchResult::default(),
            pending_results: FxHashSet::default(),
            events_queue: BinaryHeap::new(),
            next_event_sequence: 0,
            current_time_nanos: 0,
            available_resources: FxHashMap::default(),
            rng: config
                .seed
                .map_or_else(|| StdRng::from_rng(&mut rand::rng()), StdRng::seed_from_u64),
            instructions_blocked_on_resources: FxHashMap::default(),
            all_instructions: VecDeque::new(),
            all_instructions_offset: 0,
            event_recorder: EventRecorder::new(
                config.record_trace,
                config.aggregation_window_nanos,
            ),
        };
        for factory in 0..profiler.factories.len() {
            profiler.start_distillation_round(FactoryId(factory));
        }
        profiler
    }

    /// Schedules and records one distillation attempt on an unoccupied factory.
    fn start_distillation_round(&mut self, factory: FactoryId) {
        let end_time_nanos = self
            .current_time_nanos
            .checked_add(self.factories[factory.0].distillation_round_duration_nanos())
            .expect("simulated distillation time overflow");
        self.enqueue_event(end_time_nanos, Event::DistillationRoundFinished(factory));
        if self.event_recorder.record_trace {
            self.event_recorder.logs.push(InstructionTrace {
                start_time_nanos: self.current_time_nanos,
                end_time_nanos,
                instruction: LoggedInstruction {
                    instruction_name: "round".into(),
                    parameters: vec![LoggedInstructionParameter::FactoryId(factory.0)],
                },
            });
        }
        self.event_recorder
            .event_aggregator
            .record_event(self.current_time_nanos, "round", 1);
        self.event_recorder.utilization_aggregator.record_event(
            self.current_time_nanos,
            "Factory utilization",
            end_time_nanos - self.current_time_nanos,
        );
    }

    /// Stores batch instructions and records their qubit dependencies.
    fn enqueue_instructions(&mut self, instructions: &[QuantumBackendInstruction]) {
        let mut instruction_refs = Vec::with_capacity(instructions.len());

        for instruction in instructions {
            let result_index = (instruction.instruction_name == "mz").then(|| {
                let index = self.current_batch_result.results.len();
                self.current_batch_result
                    .results
                    .push(QuantumBackendResult::Error);
                index
            });
            let instruction_ref =
                InstructionRef(self.all_instructions_offset + self.all_instructions.len());
            if result_index.is_some() {
                self.pending_results.insert(instruction_ref);
            }
            self.all_instructions
                .push_back(InstructionState::new(instruction.clone(), result_index));
            instruction_refs.push(instruction_ref);

            let mut dependencies = FxHashSet::default();
            for parameter in &instruction.parameters {
                let Parameter::QubitId(qubit) = *parameter else {
                    continue;
                };
                if let Some(dependency) = self.qubits[qubit].last_instruction {
                    dependencies.insert(dependency);
                }
                self.qubits[qubit].last_instruction = Some(instruction_ref);
            }
            for dependency in dependencies {
                if !self.is_instruction_done(dependency) {
                    let dependency_index = dependency.0 - self.all_instructions_offset;
                    self.all_instructions[dependency_index]
                        .dependents
                        .push(instruction_ref);
                    let instruction_index = instruction_ref.0 - self.all_instructions_offset;
                    self.all_instructions[instruction_index].pending_dependencies += 1;
                }
            }
        }

        for instruction_ref in instruction_refs {
            self.try_instruction_start(instruction_ref);
        }
    }

    /// Returns the instruction stored for a global reference.
    fn get_instruction(
        &mut self,
        instruction_ref: InstructionRef,
    ) -> &mut QuantumBackendInstruction {
        let index = instruction_ref
            .0
            .checked_sub(self.all_instructions_offset)
            .expect("instruction reference was already retired");
        &mut self.all_instructions[index].instruction
    }

    /// Reports whether an instruction has completed, including retired references.
    fn is_instruction_done(&self, instruction_ref: InstructionRef) -> bool {
        if instruction_ref.0 < self.all_instructions_offset {
            return true;
        }
        let index = instruction_ref.0 - self.all_instructions_offset;
        self.all_instructions[index].done
    }

    /// Marks an instruction complete and retires completed instructions from the queue front.
    fn mark_instruction_as_done(&mut self, instruction_ref: InstructionRef) {
        let index = instruction_ref
            .0
            .checked_sub(self.all_instructions_offset)
            .expect("instruction completed more than once");
        self.all_instructions[index].done = true;
        while self
            .all_instructions
            .front()
            .is_some_and(|instruction| instruction.done)
        {
            self.all_instructions.pop_front();
            self.all_instructions_offset += 1;
        }
    }

    /// Returns whether dependencies are complete and any required resource is available.
    fn instruction_can_start(&self, instruction_ref: InstructionRef) -> bool {
        if self.is_instruction_done(instruction_ref) {
            return false;
        }
        let index = instruction_ref.0 - self.all_instructions_offset;
        let instruction = &self.all_instructions[index];
        instruction.pending_dependencies == 0
            && instruction.resource_requirements.is_none_or(|resource| {
                self.available_resources
                    .get(&resource)
                    .is_some_and(|factories| !factories.is_empty())
            })
    }

    /// Starts an instruction, consumes its resources, and schedules its completion.
    fn instruction_start(&mut self, instruction_ref: InstructionRef) {
        debug_assert!(self.instruction_can_start(instruction_ref));
        let index = instruction_ref.0 - self.all_instructions_offset;
        let requirement = self.all_instructions[index].resource_requirements;
        self.all_instructions[index].started = true;
        self.all_instructions[index].waiting_on_resource = None;

        if let Some(resource) = requirement {
            let factory = self
                .available_resources
                .get_mut(&resource)
                .and_then(VecDeque::pop_front)
                .expect("instruction requires an available factory");
            let instruction = self.get_instruction(instruction_ref);
            instruction.instruction_name = match instruction.instruction_name {
                "t" => "inject_t",
                "tadj" => "inject_tadj",
                _ => panic!("Unsupported resource-consuming instruction: {instruction:?}"),
            };
            instruction
                .parameters
                .insert(0, Parameter::FactoryId(factory.0));
        }
        let instruction = &self.all_instructions[index].instruction;

        let end_time_nanos = self
            .current_time_nanos
            .checked_add(instruction_time_nanos(instruction, &self.config))
            .expect("simulated instruction time overflow");

        // Record gate in aggreator, using gate start time as gate time.
        self.event_recorder.event_aggregator.record_event(
            self.current_time_nanos,
            instruction.instruction_name,
            1,
        );

        // Record instruction in detailed profile.
        if self.event_recorder.record_trace {
            self.event_recorder.logs.push(InstructionTrace {
                start_time_nanos: self.current_time_nanos,
                end_time_nanos,
                instruction: instruction.into(),
            });
        }

        self.enqueue_event(end_time_nanos, Event::InstructionFinished(instruction_ref));
    }

    /// Starts an instruction when its dependencies and required resources are ready.
    fn try_instruction_start(&mut self, instruction_ref: InstructionRef) {
        if self.is_instruction_done(instruction_ref) {
            return;
        }
        let index = instruction_ref.0 - self.all_instructions_offset;
        if self.all_instructions[index].started
            || self.all_instructions[index].pending_dependencies > 0
        {
            return;
        }
        if self.instruction_can_start(instruction_ref) {
            self.instruction_start(instruction_ref);
            return;
        }

        if let Some(resource) = self.all_instructions[index].resource_requirements
            && self.all_instructions[index].waiting_on_resource.is_none()
        {
            self.all_instructions[index].waiting_on_resource = Some(resource);
            self.instructions_blocked_on_resources
                .entry(resource)
                .or_default()
                .push_back(instruction_ref);
        }
    }

    /// Adds an event to the queue using FIFO order for equal timestamps.
    fn enqueue_event(&mut self, time_nanos: usize, event: Event) {
        let sequence = self.next_event_sequence;
        self.next_event_sequence += 1;
        self.events_queue
            .push(Reverse((time_nanos, sequence, event)));
    }

    /// Starts queued instructions whose resource requirements are now satisfied.
    fn start_resource_waiters(&mut self, resource: ResourceType) {
        loop {
            let next = self
                .instructions_blocked_on_resources
                .get(&resource)
                .and_then(VecDeque::front)
                .copied();
            let Some(instruction_ref) = next else {
                break;
            };
            if self.is_instruction_done(instruction_ref) {
                self.instructions_blocked_on_resources
                    .get_mut(&resource)
                    .expect("resource waiter queue exists")
                    .pop_front();
                continue;
            }
            let index = instruction_ref.0 - self.all_instructions_offset;
            if !self.instruction_can_start(instruction_ref) {
                break;
            }
            self.instructions_blocked_on_resources
                .get_mut(&resource)
                .expect("resource waiter queue exists")
                .pop_front();
            self.all_instructions[index].waiting_on_resource = None;
            self.try_instruction_start(instruction_ref);
        }
    }

    /// Processes one event and schedules newly enabled work.
    fn process_event(&mut self, event: Event) {
        match event {
            Event::DistillationRoundFinished(factory) => {
                if self.rng.random::<f64>() < self.factories[factory.0].probability_success {
                    self.event_recorder.event_aggregator.record_event(
                        self.current_time_nanos,
                        "T state produced",
                        1,
                    );
                    self.available_resources
                        .entry(ResourceType::TState)
                        .or_default()
                        .push_back(factory);
                    self.start_resource_waiters(ResourceType::TState);
                } else {
                    self.start_distillation_round(factory);
                }
            }
            Event::InstructionFinished(instruction_ref) => {
                let index = instruction_ref.0 - self.all_instructions_offset;
                let dependents = self.all_instructions[index].dependents.clone();
                let result_index = self.all_instructions[index].result_index;
                if let Some(result_index) = result_index {
                    self.current_batch_result.results[result_index] =
                        QuantumBackendResult::from(self.rng.random::<bool>());
                    self.pending_results.remove(&instruction_ref);
                }
                let instruction = &self.all_instructions[index].instruction;
                if matches!(instruction.instruction_name, "inject_t" | "inject_tadj") {
                    self.event_recorder.event_aggregator.record_event(
                        self.current_time_nanos,
                        "T state consumed",
                        1,
                    );
                    let [Parameter::FactoryId(factory), Parameter::QubitId(_)] =
                        instruction.parameters.as_slice()
                    else {
                        panic!("Malformed injection instruction: {instruction:?}");
                    };
                    self.start_distillation_round(FactoryId(*factory));
                }
                self.mark_instruction_as_done(instruction_ref);

                for dependent in dependents {
                    if self.is_instruction_done(dependent) {
                        continue;
                    }
                    let dependent_index = dependent.0 - self.all_instructions_offset;
                    self.all_instructions[dependent_index].pending_dependencies = self
                        .all_instructions[dependent_index]
                        .pending_dependencies
                        .checked_sub(1)
                        .expect("instruction dependency completed more than once");
                    self.try_instruction_start(dependent);
                }
            }
        }
    }

    /// Processes events until every requested result is available.
    fn event_loop(&mut self) {
        while !self.pending_results.is_empty() {
            let Reverse((time_nanos, _, event)) = self
                .events_queue
                .pop()
                .expect("event queue exhausted before all requested results completed");
            debug_assert!(time_nanos >= self.current_time_nanos);
            self.current_time_nanos = time_nanos;
            self.process_event(event);
        }
    }
}

impl Default for TemporalProfilerOld {
    /// Creates a profiler with one-second aggregation windows.
    fn default() -> Self {
        Self::new(TemporalProfilerConfig::default())
    }
}

impl QuantumBackend for TemporalProfilerOld {
    /// Schedules the batch and advances simulation only until all requested results are available.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        for instruction in instructions {
            if !matches!(
                instruction.instruction_name,
                "cx" | "h" | "s" | "sadj" | "x" | "y" | "z" | "t" | "tadj" | "mz" | "reset"
            ) {
                return Err(format!(
                    "Unsupported temporal-profiler instruction: {instruction:?}"
                ));
            }
        }
        self.current_batch_result = BatchResult::default();

        // Grow the qubit reference array to fit all instructions.
        let max_qubit_id: usize = instructions
            .iter()
            .flat_map(|instruction| instruction.parameters.iter())
            .filter_map(|parameter| match parameter {
                Parameter::QubitId(qubit) => Some(*qubit),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        if max_qubit_id + 1 > self.qubits.len() {
            self.qubits
                .resize_with(max_qubit_id + 1, LogicalQubit::default);
        }

        self.enqueue_instructions(instructions);

        self.event_loop();
        Ok(std::mem::take(&mut self.current_batch_result))
    }
}

impl Profiler for TemporalProfilerOld {
    /// Returns the profiling shell and the recorded instruction timeline.
    fn get_result(&self) -> ProfilerResult {
        let factory_logical_qubits: usize = self.factories.iter().map(|f| f.logical_qubits).sum();
        let logical_qubits = self.qubits.len() + factory_logical_qubits;

        // Assuming Surface Code.
        let factory_physical_qubits: usize =
            self.factories.iter().map(TFactory::physical_qubits).sum();
        let d = self.config.ecc_distance;
        let physical_qubits = self.qubits.len() * (2 * d * d - 1) + factory_physical_qubits;

        #[allow(clippy::cast_precision_loss)]
        let total_runtime_sec = self.current_time_nanos as f64 / 1e9;
        let totals = TotalCounts {
            logical_qubits,
            logical_compute_qubits: self.qubits.len(),
            physical_qubits,
            total_runtime_sec,
            total_operations_count: self.event_recorder.event_aggregator.total_event_count(),
            calls_per_operation: Default::default(),
            runtime_sec_per_operation: Default::default(),
        };
        ProfilerResult {
            totals,
            profiles: self.event_recorder.get_profiles(),
            trace: self.event_recorder.get_trace(),
            infos: vec![],
        }
    }
}
