// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Profiler, ProfilerResult, TemporalProfiles, TotalCounts};
use crate::{
    Parameter, QuantumBackendResult, TumblingWindowAggregator,
    interface::{BatchResult, QuantumBackend, QuantumBackendInstruction},
    profilers::{ExecutionTrace, InstructionTrace, ProfilerConfig},
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
};

/// Returns the simulated duration of a supported instruction.
fn instruction_time_nanos(instruction: &QuantumBackendInstruction) -> usize {
    match instruction.instruction_name {
        "h" | "x" | "t" | "cx" | "mz" | "mz_store" | "mz_store_reset" => 1,
        "read_result" => 0,
        _ => panic!("Unsupported temporal-profiler instruction: {instruction:?}"),
    }
}

/// Returns the constant production interval for a resource.
fn resource_production_time_nanos(resource_type: ResourceType) -> usize {
    match resource_type {
        ResourceType::TState => 2,
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
    TStateProduced,
    InstructionFinished(InstructionRef),
}

/// Stable reference to an instruction across all received batches.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct InstructionRef(usize);

/// Mutable scheduling data retained for an instruction until it can be retired.
struct InstructionState {
    instruction: QuantumBackendInstruction,
    done: bool,
    started: bool,
    pending_dependencies: usize,
    dependents: Vec<InstructionRef>,
    resource_requirements: Vec<(ResourceType, usize)>,
    waiting_on_resource: Option<ResourceType>,
    result_index: Option<usize>,
}

impl InstructionState {
    /// Creates an instruction state with no dependencies or scheduled work.
    fn new(instruction: QuantumBackendInstruction, result_index: Option<usize>) -> Self {
        let resource_requirements = if instruction.instruction_name == "t" {
            vec![(ResourceType::TState, 1)]
        } else {
            Vec::new()
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
/// A simulated event clock schedules instruction completions and recurring T-state production.
/// Events at the same timestamp are handled in insertion order. `h`, `x`, `t`, `cx`, `mz`,
/// `mz_store`, and `mz_store_reset` each take 1 ms; `read_result` takes no time. One T-state is
/// produced every 1 ms, starting at 1 ms, and each `t` consumes one available state before it
/// starts. Other instructions are currently unsupported.
///
/// Measurements return random bits to satisfy the backend interface. The implementation records
/// instruction start/end messages in `infos`; it does not yet aggregate totals or temporal
/// profiles. Instructions with no resource dependency may start while earlier resource-production
/// events remain scheduled, and each batch runs the event loop only until that batch completes.
pub struct TemporalProfiler {
    qubits: Vec<Option<InstructionRef>>,
    result_writers: FxHashMap<usize, InstructionRef>,
    current_batch_result: BatchResult,
    /// Events are ordered by time and then by insertion sequence.
    events_queue: BinaryHeap<Reverse<(usize, u64, Event)>>,
    next_event_sequence: u64,
    current_time_nanos: usize,
    resource_counts: FxHashMap<ResourceType, usize>,
    instructions_blocked_on_resources: FxHashMap<ResourceType, VecDeque<InstructionRef>>,
    all_instructions: VecDeque<InstructionState>,
    all_instructions_offset: usize,
    logs: Vec<InstructionTrace>,
    aggregator: TumblingWindowAggregator,
}

impl TemporalProfiler {
    /// Creates a profiler and schedules the first T-state production event.
    #[must_use]
    pub fn new(config: ProfilerConfig) -> Self {
        let mut profiler = Self {
            qubits: Vec::new(),
            result_writers: FxHashMap::default(),
            current_batch_result: BatchResult::default(),
            events_queue: BinaryHeap::new(),
            next_event_sequence: 0,
            current_time_nanos: 0,
            resource_counts: FxHashMap::from_iter([(ResourceType::TState, 0)]),
            instructions_blocked_on_resources: FxHashMap::default(),
            all_instructions: VecDeque::new(),
            all_instructions_offset: 0,
            logs: Vec::new(),
            aggregator: TumblingWindowAggregator::new(config.aggregation_window_nanos),
        };
        profiler.enqueue_event(
            resource_production_time_nanos(ResourceType::TState),
            Event::TStateProduced,
        );
        profiler
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
                if let Some(dependency) = self.qubits[qubit] {
                    dependencies.insert(dependency);
                }
                self.qubits[qubit] = Some(instruction_ref);
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

    /// Returns whether dependencies and all required resources are available.
    fn instruction_can_start(&self, instruction_ref: InstructionRef) -> bool {
        if self.is_instruction_done(instruction_ref) {
            return false;
        }
        let index = instruction_ref.0 - self.all_instructions_offset;
        let instruction = &self.all_instructions[index];
        instruction.pending_dependencies == 0
            && instruction
                .resource_requirements
                .iter()
                .all(|(resource, quantity)| {
                    self.resource_counts
                        .get(resource)
                        .copied()
                        .unwrap_or_default()
                        >= *quantity
                })
    }

    /// Starts an instruction, consumes its resources, and schedules its completion.
    fn instruction_start(&mut self, instruction_ref: InstructionRef) {
        debug_assert!(self.instruction_can_start(instruction_ref));
        let index = instruction_ref.0 - self.all_instructions_offset;
        let instruction = self.get_instruction(instruction_ref).clone();
        let requirements = self.all_instructions[index].resource_requirements.clone();
        self.all_instructions[index].started = true;
        self.all_instructions[index].waiting_on_resource = None;

        for (resource, quantity) in requirements {
            let count = self.resource_counts.entry(resource).or_default();
            *count -= quantity;
        }

        let end_time_nanos = self
            .current_time_nanos
            .checked_add(instruction_time_nanos(&instruction))
            .expect("simulated instruction time overflow");
        self.logs.push(InstructionTrace {
            start_time_nanos: self.current_time_nanos,
            end_time_nanos,
            instruction,
        });
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

        let resource = self.all_instructions[index]
            .resource_requirements
            .iter()
            .find_map(|(resource, quantity)| {
                (self
                    .resource_counts
                    .get(resource)
                    .copied()
                    .unwrap_or_default()
                    < *quantity)
                    .then_some(*resource)
            });
        if let Some(resource) = resource
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
            let enough_resources = self.all_instructions[index]
                .resource_requirements
                .iter()
                .all(|(required_resource, quantity)| {
                    self.resource_counts
                        .get(required_resource)
                        .copied()
                        .unwrap_or_default()
                        >= *quantity
                });
            if !enough_resources {
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
            Event::TStateProduced => {
                // Increase available resource count.
                *self
                    .resource_counts
                    .entry(ResourceType::TState)
                    .or_default() += 1;
                // Schedule next resource production event.
                let next_time = self
                    .current_time_nanos
                    .checked_add(resource_production_time_nanos(ResourceType::TState))
                    .expect("simulated resource production time overflow");
                self.enqueue_event(next_time, Event::TStateProduced);
                self.start_resource_waiters(ResourceType::TState);
            }
            Event::InstructionFinished(instruction_ref) => {
                let index = instruction_ref.0 - self.all_instructions_offset;
                let dependents = self.all_instructions[index].dependents.clone();
                let result_index = self.all_instructions[index].result_index;
                if let Some(result_index) = result_index {
                    self.current_batch_result.results[result_index] =
                        QuantumBackendResult::from(rand::random::<bool>());
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
    fn default() -> Self {
        Self::new(ProfilerConfig {
            aggregation_window_nanos: 1000_000_000,
        })
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
            self.qubits.resize(max_qubit_id + 1, None);
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
        ProfilerResult {
            totals: TotalCounts::default(),
            profiles: TemporalProfiles::default(),
            trace: Some(ExecutionTrace {
                instructions: self.logs.clone(),
            }),
            infos: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TemporalProfiler;
    use crate::Profiler;
    use crate::tests::compile_qsharp_to_qir;
    use expect_test::expect;
    use qsc::target::Profile;

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
                    T(q[1]);
                    [M(q[0]), M(q[1])]
                }
            }
        "#;
        let file = compile_qsharp_to_qir(source, Profile::Base);
        let mut profiler = TemporalProfiler::default();

        // SAFETY: The QIR was generated by the in-tree Q# compiler from trusted source.
        let _ = unsafe { profiler.profile_qir(file.path()?) }?;

        let result = profiler.get_result();
        expect![[r#"
            0-1: x 0
            0-1: h 1
            1-2: h 0
            2-3: cx 0 1
            3-4: t 0
            4-5: t 1
            4-5: mz_store 0 0
            5-6: mz_store 1 1
            5-5: read_result 0
            6-6: read_result 1"#]]
        .assert_eq(&format!("{:?}", result.trace.unwrap()));
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
        let mut profiler = TemporalProfiler::default();

        // SAFETY: The QIR was generated by the in-tree Q# compiler from trusted source.
        let _ = unsafe { profiler.profile_qir(file.path()?) }?;
        let _ = profiler.get_result();

        Ok(())
    }
}
