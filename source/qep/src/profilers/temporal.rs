// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Profiler, ProfilerResult, TotalCounts, factories::TFactory};
use crate::{
    Parameter, QuantumBackendResult, TumblingWindowAggregator,
    interface::{BatchResult, QuantumBackend, QuantumBackendInstruction},
    profilers::{ExecutionTrace, InstructionTrace, ProfilerConfig},
};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::smallvec;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
};

/// Returns a coarse logical-instruction duration, in nanoseconds.
///
/// Litinski, arXiv:1808.02892, Section 1, defines a logical time step as roughly
/// d surface-code cycles; Section 4 assumes a 1 us physical code cycle. For this
/// distance-5 demo, one logical step is therefore 5 * 1_000 = 5_000 ns.
/// This is a fixed benchmark, not a hardware calibration or a distance-dependent
/// estimate: it does not currently use `ProfilerConfig::ecc_distance`.
///
/// We charge every Clifford one logical step. Actual costs differ: Pauli gates
/// can be tracked classically, and lattice-surgery CNOTs can require several
/// stages. Destructive logical readout need not take d rounds (Section 1);
/// we allow one physical code cycle for measurement, reset, or their combination,
/// rather than modeling their separate hardware and decoding latencies.
fn instruction_time_nanos(instruction: &QuantumBackendInstruction) -> usize {
    const CLIFFORD_TIME_NANOS: usize = 5_000;
    const MEASUREMENT_RESET_TIME_NANOS: usize = 1_000;

    match instruction.instruction_name {
        "cx" | "cy" | "cz" | "h" | "s" | "sadj" | "sx" | "x" | "y" | "z" | "swap" => {
            CLIFFORD_TIME_NANOS
        }
        "mz" | "mz_reset" | "mz_store" | "mz_store_reset" | "reset" => MEASUREMENT_RESET_TIME_NANOS,
        // Explicit injection: CNOT + measurement + worst-case conditional S,
        // with CNOT and S each charged one Clifford step: 11_000 ns. We omit
        // extra classical feedforward latency and assume a distilled state is
        // already available; the scheduler accounts for factory waiting.
        // This is not Litinski's optimized fast-block consumption time of one
        // logical step, which tracks or avoids the separate correction.
        "inject_t" | "inject_tadj" => 2 * CLIFFORD_TIME_NANOS + MEASUREMENT_RESET_TIME_NANOS,

        "read_result" => 0,
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

/// Simulates QIR execution with per-qubit and measurement-result dependencies.
///
/// Each instruction depends on the latest earlier operation touching each target qubit;
/// `read_result` also depends on the latest writer of its result ID. Independent instructions run
/// concurrently, and a completed instruction may unblock multiple successors. Global instruction
/// references and the latest qubit/result writers persist across backend batches, so adaptive
/// measurement barriers preserve dependencies.
///
/// A simulated event clock schedules instruction completions and factory distillation rounds.
/// Events at the same timestamp are handled in insertion order. Instruction durations use
/// the fixed logical timing model; `read_result` takes no time. Each factory holds at most one T-state and restarts after a failed
/// round or after the injection consuming its state completes.
///
/// Distillation and measurement randomness share a seedable RNG. Instruction and round intervals
/// are optionally recorded in the trace and always aggregated into operation counts and rate profiles. Instructions
/// with no resource dependency may start while distillation events remain scheduled, and each batch
/// runs the event loop only until that batch completes.
pub struct TemporalProfiler {
    /// Logical-qubit scheduling state, indexed by qubit ID.
    qubits: Vec<LogicalQubit>,
    /// Factories.
    factories: Vec<TFactory>,
    /// Latest measurement instruction writing each result ID.
    result_writers: FxHashMap<usize, InstructionRef>,
    /// Measurement and readout results for the batch being executed.
    current_batch_result: BatchResult,
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
    /// Instruction execution intervals recorded in start order.
    logs: Vec<InstructionTrace>,
    /// Whether instruction and factory-round intervals should be retained.
    record_trace: bool,
    /// Collects event counts into fixed-window rate profiles.
    aggregator: TumblingWindowAggregator,
}

impl TemporalProfiler {
    /// Creates a profiler and starts a distillation round on every factory.
    ///
    /// # Panics
    ///
    /// Panics if there are no T factories or the aggregation window is zero.
    #[must_use]
    pub fn new(config: ProfilerConfig) -> Self {
        assert!(
            config.t_factories_count > 0,
            "at least one T factory is required"
        );
        let mut profiler = Self {
            qubits: Vec::new(),
            factories: vec![TFactory::new(); config.t_factories_count],
            result_writers: FxHashMap::default(),
            current_batch_result: BatchResult::default(),
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
            logs: Vec::new(),
            record_trace: config.record_trace,
            aggregator: TumblingWindowAggregator::new(config.aggregation_window_nanos),
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
        if self.record_trace {
            self.logs.push(InstructionTrace {
                start_time_nanos: self.current_time_nanos,
                end_time_nanos,
                instruction: QuantumBackendInstruction {
                    instruction_name: "round",
                    parameters: smallvec![Parameter::FactoryId(factory.0)],
                },
            });
        }
        self.aggregator
            .record_event(self.current_time_nanos, "round");
    }

    /// Stores batch instructions and records their qubit and result dependencies.
    fn enqueue_instructions(&mut self, instructions: &[QuantumBackendInstruction]) {
        let mut instruction_refs = Vec::with_capacity(instructions.len());

        for instruction in instructions {
            let result_index = matches!(
                instruction.instruction_name,
                "read_result" | "mz" | "mz_reset"
            )
            .then(|| {
                let index = self.current_batch_result.results.len();
                self.current_batch_result
                    .results
                    .push(QuantumBackendResult::Error);
                index
            });
            let instruction_ref =
                InstructionRef(self.all_instructions_offset + self.all_instructions.len());
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
            if instruction.instruction_name == "read_result"
                && let Some(Parameter::ResultId(result_id)) = instruction.parameters.first()
                && let Some(dependency) = self.result_writers.get(result_id)
            {
                dependencies.insert(*dependency);
            }
            if matches!(instruction.instruction_name, "mz_store" | "mz_store_reset")
                && let [Parameter::QubitId(_), Parameter::ResultId(result_id)] =
                    instruction.parameters.as_slice()
            {
                self.result_writers.insert(*result_id, instruction_ref);
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
            .checked_add(instruction_time_nanos(instruction))
            .expect("simulated instruction time overflow");

        // Record gate in aggreator, using gate start time as gate time.
        self.aggregator
            .record_event(self.current_time_nanos, instruction.instruction_name);

        // Record instruction in detailed profile.
        if self.record_trace {
            self.logs.push(InstructionTrace {
                start_time_nanos: self.current_time_nanos,
                end_time_nanos,
                instruction: instruction.clone(),
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
                    self.aggregator
                        .record_event(self.current_time_nanos, "T state produced");
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
                }
                let instruction = &self.all_instructions[index].instruction;
                if matches!(instruction.instruction_name, "inject_t" | "inject_tadj") {
                    self.aggregator
                        .record_event(self.current_time_nanos, "T state consumed");
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

    /// Processes events until every instruction in the current batch is complete.
    fn event_loop(&mut self, batch_end: usize) {
        while self.all_instructions_offset < batch_end {
            let Reverse((time_nanos, _, event)) = self
                .events_queue
                .pop()
                .expect("event queue exhausted before the batch completed");
            debug_assert!(time_nanos >= self.current_time_nanos);
            self.current_time_nanos = time_nanos;
            self.process_event(event);
        }
    }
}

impl Default for TemporalProfiler {
    /// Creates a profiler with one-second aggregation windows.
    fn default() -> Self {
        Self::new(ProfilerConfig::default())
    }
}

impl QuantumBackend for TemporalProfiler {
    /// Builds dependencies, simulates the batch, and returns measurement results.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        self.current_batch_result = BatchResult::default();
        let batch_start = self.all_instructions_offset + self.all_instructions.len();

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

        let batch_end = batch_start + instructions.len();
        self.event_loop(batch_end);
        Ok(std::mem::take(&mut self.current_batch_result))
    }
}

impl Profiler for TemporalProfiler {
    /// Returns the profiling shell and the recorded instruction timeline.
    fn get_result(&self) -> ProfilerResult {
        let factory_logical_qubits: usize = self.factories.iter().map(|f| f.logical_qubits).sum();

        #[allow(clippy::cast_precision_loss)]
        let total_runtime_sec = self.current_time_nanos as f64 / 1e9;
        let totals = TotalCounts {
            logical_qubits: self.qubits.len() + factory_logical_qubits,
            physical_qubits: 0,
            total_runtime_sec,
            total_operations_count: self.aggregator.total_event_count(),
            calls_per_operation: Default::default(),
            runtime_sec_per_operation: Default::default(),
        };
        ProfilerResult {
            totals,
            profiles: self.aggregator.get_profiles(),
            trace: self.record_trace.then(|| ExecutionTrace {
                instructions: self.logs.clone(),
            }),
            infos: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Reverse;
    use std::path::PathBuf;
    use std::time::Instant;

    use super::{Event, FactoryId, ResourceType, TemporalProfiler, instruction_time_nanos};
    use crate::tests::compile_qsharp_to_qir;
    use crate::{
        Parameter::{FactoryId as FactoryParameter, QubitId},
        Profiler, ProfilerConfig, QuantumBackend, QuantumBackendInstruction, TranspilingAdapter,
    };
    use expect_test::expect;
    use qsc::target::Profile;

    /// Creates a repeatable five-factory profiler with tracing enabled for timing assertions.
    fn seeded_profiler() -> TemporalProfiler {
        TemporalProfiler::new(ProfilerConfig {
            t_factories_count: 5,
            seed: Some(42),
            record_trace: true,
            ..ProfilerConfig::default()
        })
    }
    #[test]
    /// Profiles the Shor's algorithm QIR fixture, saves the result, and reports elapsed time.
    fn temporal_profiler_shors_qir() -> Result<(), String> {
        if cfg!(debug_assertions) {
            return Ok(()); // Skip in debug mode
        }

        let len = 50;
        let qir_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tmp/shors_{len}.ll"));
        let qir_path = qir_path
            .to_str()
            .ok_or("Shor's algorithm QIR path is not valid UTF-8")?;
        let profiler = TemporalProfiler::new(ProfilerConfig {
            aggregation_window_nanos: 1_000_000_000,
            t_factories_count: 5,
            seed: Some(42),
            record_trace: false,
            ..ProfilerConfig::default()
        });
        let mut profiler = TranspilingAdapter::with_defaults(profiler);
        let start = Instant::now();

        // SAFETY: The checked-in QIR fixture was produced by a trusted compiler.
        let result = unsafe { profiler.profile_qir(qir_path) }?;
        let elapsed = start.elapsed();
        let profile_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tmp/shors_{len}_profile.json"));
        result.to_json(&profile_path)?;
        assert!(result.infos.is_empty(), "{:?}", result.infos);
        let op_count: usize = result.totals.total_operations_count;

        assert!(op_count > 0);
        let ns_per_op = (elapsed.as_nanos() as f64) / (op_count as f64);
        println!(
            "Profiled shors_{len}.ll: {op_count} operations in {elapsed:?} ({ns_per_op:.1} ns/op)"
        );
        Ok(())
    }

    #[test]
    /// Snapshots instruction timing and dependencies for a simple program.
    fn profiles_simple_program() -> Result<(), String> {
        let source = r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Result[] {
                    use q = Qubit[2];
                    X(q[0]);
                    H(q[0]);
                    H(q[1]);
                    CNOT(q[0], q[1]);
                    T(q[0]);
                    Adjoint T(q[1]);
                    [M(q[0]), M(q[1])]
                }
            }
        "#;
        let file = compile_qsharp_to_qir(source, Profile::Base);
        let mut profiler = seeded_profiler();

        // SAFETY: The QIR was generated by the in-tree Q# compiler from trusted source.
        let _ = unsafe { profiler.profile_qir(file.path()?) }?;

        let result = profiler.get_result();
        let profile_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tmp/small_profile.json");
        result.to_json(&profile_path)?;
        expect![[r#"
            0-55000: round 0
            0-55000: round 1
            0-55000: round 2
            0-55000: round 3
            0-55000: round 4
            0-5000: x 0
            0-5000: h 1
            5000-10000: h 0
            10000-15000: cx 0 1
            55000-66000: inject_t 0 0
            55000-66000: inject_tadj 1 1
            66000-121000: round 0
            66000-67000: mz_store 0 0
            66000-121000: round 1
            66000-67000: mz_store 1 1
            67000-67000: read_result 0
            67000-67000: read_result 1"#]]
        .assert_eq(&format!(
            "{:?}",
            result
                .trace
                .expect("temporal profiler should record a trace")
        ));
        Ok(())
    }

    /// Checks each instruction category against the fixed nanosecond benchmark.
    #[test]
    fn instruction_durations_use_logical_timing_assumptions() {
        for (names, expected) in [
            (
                &[
                    "cx", "cy", "cz", "h", "s", "sadj", "sx", "x", "y", "z", "swap",
                ][..],
                5_000,
            ),
            (
                &["mz", "mz_reset", "mz_store", "mz_store_reset", "reset"][..],
                1_000,
            ),
            (&["inject_t", "inject_tadj"][..], 11_000),
            (&["read_result"][..], 0),
        ] {
            for &instruction_name in names {
                assert_eq!(
                    instruction_time_nanos(&QuantumBackendInstruction {
                        instruction_name,
                        parameters: smallvec::smallvec![],
                    }),
                    expected,
                    "{instruction_name}"
                );
            }
        }
    }

    /// Checks that T and T-adjoint each consume one state while H needs none.
    #[test]
    fn schedules_optional_single_resource_requirements() -> Result<(), String> {
        let mut profiler = seeded_profiler();
        for factory in &mut profiler.factories {
            factory.probability_success = 1.0;
        }
        let instructions: Vec<_> = [("t", 0), ("tadj", 1), ("h", 2)]
            .into_iter()
            .map(|(instruction_name, qubit)| QuantumBackendInstruction {
                instruction_name,
                parameters: smallvec::smallvec![QubitId(qubit)],
            })
            .collect();

        profiler.execute_batch(&instructions)?;

        let intervals: Vec<_> = profiler
            .logs
            .iter()
            .map(|trace| {
                (
                    trace.instruction.instruction_name,
                    trace.start_time_nanos,
                    trace.end_time_nanos,
                )
            })
            .collect();
        assert_eq!(
            intervals,
            vec![
                ("round", 0, 55_000),
                ("round", 0, 55_000),
                ("round", 0, 55_000),
                ("round", 0, 55_000),
                ("round", 0, 55_000),
                ("h", 0, 5_000),
                ("inject_t", 55_000, 66_000),
                ("inject_tadj", 55_000, 66_000),
                ("round", 66_000, 121_000),
                ("round", 66_000, 121_000),
            ]
        );
        assert_eq!(
            profiler.available_resources[&ResourceType::TState],
            std::collections::VecDeque::from([FactoryId(2), FactoryId(3), FactoryId(4)])
        );
        // Five states were produced and two were consumed.
        assert_eq!(
            profiler.aggregator.total_event_count(),
            profiler.logs.len() + 7
        );
        assert!(
            profiler
                .instructions_blocked_on_resources
                .values()
                .all(std::collections::VecDeque::is_empty)
        );
        Ok(())
    }

    #[test]
    /// Keeps qubit and measurement dependencies valid across adaptive result batches.
    fn profiles_adaptive_program_across_batches() -> Result<(), String> {
        let source = r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    let result = M(q);
                    if result == One {
                        X(q);
                    } else {
                        H(q);
                    }
                }
            }
        "#;
        let file = compile_qsharp_to_qir(source, Profile::Adaptive);
        let mut profiler = seeded_profiler();

        // SAFETY: The QIR was generated by the in-tree Q# compiler from trusted source.
        let _ = unsafe { profiler.profile_qir(file.path()?) }?;
        let _ = profiler.get_result();

        Ok(())
    }

    /// All five factories retry failures and then each hold exactly one ready state.
    #[test]
    fn retries_failed_rounds_and_holds_one_ready_state() {
        let mut profiler = seeded_profiler();
        for factory in &mut profiler.factories {
            factory.probability_success = 0.0;
        }
        for factory in 0..5 {
            let Reverse((time, _, event)) = profiler.events_queue.pop().expect("initial round");
            assert_eq!(time, 55_000);
            assert_eq!(event, Event::DistillationRoundFinished(FactoryId(factory)));
            profiler.current_time_nanos = time;
            profiler.process_event(event);
            let trace = &profiler.logs[5 + factory];
            assert_eq!(
                trace.instruction.parameters.as_slice(),
                &[FactoryParameter(factory)]
            );
            assert_eq!(
                (trace.start_time_nanos, trace.end_time_nanos),
                (55_000, 110_000)
            );
        }
        assert!(profiler.available_resources.is_empty());
        assert_eq!(profiler.logs.len(), 10);

        for factory in &mut profiler.factories {
            factory.probability_success = 1.0;
        }
        for factory in 0..5 {
            let Reverse((time, _, event)) = profiler.events_queue.pop().expect("retry round");
            assert_eq!(time, 110_000);
            assert_eq!(event, Event::DistillationRoundFinished(FactoryId(factory)));
            profiler.current_time_nanos = time;
            profiler.process_event(event);
        }
        assert_eq!(
            profiler.available_resources[&ResourceType::TState],
            (0..5)
                .map(FactoryId)
                .collect::<std::collections::VecDeque<_>>()
        );
        assert!(profiler.events_queue.is_empty());
        assert_eq!(profiler.logs.len(), 10);
    }

    /// Two factories serve concurrent gates and cannot be reused until injection finishes.
    #[test]
    fn reserves_factories_until_injection_completes() -> Result<(), String> {
        let mut profiler = TemporalProfiler::new(ProfilerConfig {
            t_factories_count: 2,
            seed: Some(42),
            record_trace: true,
            ..ProfilerConfig::default()
        });
        for factory in &mut profiler.factories {
            factory.probability_success = 1.0;
        }
        let instructions: Vec<_> = [("t", 0), ("tadj", 1), ("t", 2)]
            .into_iter()
            .map(|(instruction_name, qubit)| QuantumBackendInstruction {
                instruction_name,
                parameters: smallvec::smallvec![QubitId(qubit)],
            })
            .collect();
        profiler.execute_batch(&instructions)?;
        let injections: Vec<_> = profiler
            .logs
            .iter()
            .filter(|trace| trace.instruction.instruction_name != "round")
            .collect();
        assert_eq!(injections.len(), 3);
        for (trace, name, factory, qubit, start) in [
            (injections[0], "inject_t", 0, 0, 55_000),
            (injections[1], "inject_tadj", 1, 1, 55_000),
            (injections[2], "inject_t", 0, 2, 121_000),
        ] {
            assert_eq!(trace.instruction.instruction_name, name);
            assert_eq!(
                trace.instruction.parameters.as_slice(),
                &[FactoryParameter(factory), QubitId(qubit)]
            );
            assert_eq!(
                (trace.start_time_nanos, trace.end_time_nanos),
                (start, start + 11_000)
            );
            crate::validate_instruction(&trace.instruction);
        }
        assert_eq!(profiler.current_time_nanos, 132_000);
        assert_eq!(
            profiler.available_resources[&ResourceType::TState],
            std::collections::VecDeque::from([FactoryId(1)])
        );
        assert!(
            profiler
                .logs
                .iter()
                .any(|trace| trace.instruction.instruction_name == "round"
                    && trace.instruction.parameters.as_slice() == [FactoryParameter(0)]
                    && trace.start_time_nanos == 132_000
                    && trace.end_time_nanos == 187_000)
        );

        profiler.execute_batch(&[QuantumBackendInstruction {
            instruction_name: "tadj",
            parameters: smallvec::smallvec![QubitId(0)],
        }])?;
        assert!(
            profiler
                .logs
                .iter()
                .any(|trace| trace.instruction.instruction_name == "inject_tadj"
                    && trace.instruction.parameters.as_slice()
                        == [FactoryParameter(1), QubitId(0)]
                    && trace.start_time_nanos == 132_000
                    && trace.end_time_nanos == 143_000)
        );
        // Four states have been produced and consumed, including the second batch.
        assert_eq!(
            profiler.aggregator.total_event_count(),
            profiler.logs.len() + 8
        );
        Ok(())
    }

    /// The same seed reproduces distillation, immediate results, and reads across batches.
    #[test]
    fn seeded_randomness_reproduces_execution() -> Result<(), String> {
        let mut first = seeded_profiler();
        let mut second = seeded_profiler();
        for _ in 0..8 {
            let instructions = [
                QuantumBackendInstruction {
                    instruction_name: "t",
                    parameters: smallvec::smallvec![QubitId(0)],
                },
                QuantumBackendInstruction {
                    instruction_name: "mz",
                    parameters: smallvec::smallvec![QubitId(0)],
                },
                QuantumBackendInstruction {
                    instruction_name: "read_result",
                    parameters: smallvec::smallvec![crate::Parameter::ResultId(0)],
                },
            ];
            assert_eq!(
                first.execute_batch(&instructions)?,
                second.execute_batch(&instructions)?
            );
        }
        assert_eq!(first.get_result(), second.get_result());
        Ok(())
    }

    /// Idle factories retain one state across batches without delaying resource-free work.
    #[test]
    fn holds_ready_states_across_resource_free_batches() -> Result<(), String> {
        let mut profiler = seeded_profiler();
        for factory in &mut profiler.factories {
            factory.probability_success = 1.0;
        }
        profiler.execute_batch(&[])?;
        assert_eq!(profiler.current_time_nanos, 0);
        let h = QuantumBackendInstruction {
            instruction_name: "h",
            parameters: smallvec::smallvec![QubitId(0)],
        };
        profiler.execute_batch(&vec![h; 40])?;
        assert_eq!(profiler.current_time_nanos, 200_000);
        assert!(profiler.events_queue.is_empty());
        assert_eq!(
            profiler.available_resources[&ResourceType::TState],
            (0..5)
                .map(FactoryId)
                .collect::<std::collections::VecDeque<_>>()
        );
        assert_eq!(
            profiler
                .logs
                .iter()
                .filter(|trace| trace.instruction.instruction_name == "round")
                .count(),
            5
        );

        profiler.execute_batch(&[QuantumBackendInstruction {
            instruction_name: "t",
            parameters: smallvec::smallvec![QubitId(0)],
        }])?;
        assert_eq!(profiler.current_time_nanos, 211_000);
        assert_eq!(
            profiler.available_resources[&ResourceType::TState],
            (1..5)
                .map(FactoryId)
                .collect::<std::collections::VecDeque<_>>()
        );
        let trace = profiler.logs.last().expect("restarted round");
        assert_eq!(trace.instruction.instruction_name, "round");
        assert_eq!(
            (trace.start_time_nanos, trace.end_time_nanos),
            (211_000, 266_000)
        );
        assert_eq!(profiler.get_result().totals.total_operations_count, 53);
        assert!((profiler.get_result().totals.total_runtime_sec - 211_000e-9).abs() < 1e-18);
        let metrics = profiler.get_result().profiles.profiles;
        let round_profile = metrics
            .iter()
            .find(|profile| profile.metric_name == "round rate")
            .expect("round starts are aggregated");
        assert_eq!(round_profile.series, vec![(0.0, 6.0)]);
        assert!(
            metrics
                .iter()
                .any(|profile| profile.metric_name == "inject_t rate")
        );
        assert!(
            metrics
                .iter()
                .all(|profile| profile.metric_name != "t rate")
        );
        Ok(())
    }

    /// Construction rejects configurations that could permanently block resource consumers.
    #[test]
    #[should_panic(expected = "at least one T factory is required")]
    fn rejects_zero_factories() {
        let _ = TemporalProfiler::new(ProfilerConfig {
            t_factories_count: 0,
            ..ProfilerConfig::default()
        });
    }
}
