// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    InstructionTrace, Profiler, ProfilerResult,
    models::{DemoModel1, QpuModel},
    profile_types::LoggableOperation,
};
use crate::{
    QuantumBackendResult,
    interface::{BatchResult, QuantumBackend, QuantumBackendInstruction},
    profilers::event_recorder::EventRecorder,
};
use rand::{SeedableRng, rngs::StdRng};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
};

#[cfg(test)]
mod tests;

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

/// Internal operation reference
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct OperationRef(usize);

pub enum Precondition<M: QpuModel> {
    ResourceIsAvailable(M::Resource),
}

/// Scheduling data retained until a completed operation can be retired.
struct OperationState<M: QpuModel> {
    /// Temporarily taken while invoking model callbacks.
    operation: Option<M::NativeOperation>,
    /// Simulated duration from operation start to completion.
    duration_nanos: usize,
    /// Slot in the current batch's results, or `None` for operations without a result.
    result_index: Option<usize>,
    /// Number of predecessor operations that have not yet completed.
    pending_dependencies: usize,
    /// Operations waiting for this operation to complete.
    dependents: Vec<OperationRef>,
    /// Resources that must be available before the operation can start.
    resource_requirements: Vec<M::Resource>,
    /// Whether the start callback has been invoked and completion is being scheduled or awaited.
    started: bool,
    /// Whether completion has been processed, even if the operation is not yet retired.
    done: bool,
}

pub struct TemporalProfilerState<M: QpuModel> {
    /// Current simulated time in nanoseconds.
    pub current_time_nanos: usize,
    /// Results for the current batch, stored in scheduling order.
    current_batch_result: BatchResult,
    /// Collects execution intervals, event rates, and utilization metrics.
    pub event_recorder: EventRecorder,
    /// Shared randomness for model callbacks, including measurements and distillation.
    pub rng: StdRng,

    /// For each resource, holds a list of functional units that have that resource.
    /// Models consume entries here and publish new entries through `add_resource`.
    pub(super) available_resources: FxHashMap<M::Resource, VecDeque<M::FunctionalUnitId>>,

    /// Operations whose results must be available before the current batch returns.
    pending_operations_producing_results: FxHashSet<OperationRef>,
    /// Scheduled operations retained until the completed prefix can be retired.
    operations: VecDeque<OperationState<M>>,
    /// Global operation index corresponding to the front of `operations`.
    operations_offset: usize,
    /// Latest scheduled operation touching each functional unit.
    pub(super) last_operations: FxHashMap<M::FunctionalUnitId, OperationRef>,
    /// Dependency-ready operations awaiting a resource check and start attempt.
    ready_operations: VecDeque<OperationRef>,
    /// FIFO waiters grouped by their first unavailable resource.
    operations_blocked_on_resources: FxHashMap<M::Resource, VecDeque<OperationRef>>,
    /// Completion time followed by insertion sequence for deterministic timestamp ties.
    events: BinaryHeap<Reverse<(usize, u64, OperationRef)>>,
    /// Sequence number assigned to the next completion event to break timestamp ties.
    next_event_sequence: u64,
}

impl<M: QpuModel> TemporalProfilerState<M> {
    /// Creates model-specific state with an event recorder and optional random seed.
    fn new(seed: Option<u64>, event_recorder: EventRecorder) -> Self {
        TemporalProfilerState {
            current_time_nanos: 0,
            current_batch_result: BatchResult::default(),
            event_recorder,
            rng: seed.map_or_else(|| StdRng::from_rng(&mut rand::rng()), StdRng::seed_from_u64),
            available_resources: FxHashMap::default(),
            pending_operations_producing_results: FxHashSet::default(),
            operations: VecDeque::new(),
            operations_offset: 0,
            last_operations: FxHashMap::default(),
            ready_operations: VecDeque::new(),
            operations_blocked_on_resources: FxHashMap::default(),
            events: BinaryHeap::new(),
            next_event_sequence: 0,
        }
    }

    /// Queues an operation with explicit and per-target dependencies and an optional result slot.
    /// The profiler starts eligible operations at the current simulated time.
    pub fn schedule_operation(
        &mut self,
        op: M::NativeOperation,
        preconditions: Vec<Precondition<M>>,
        targets: Vec<M::FunctionalUnitId>,
        duration_nanos: usize,
        produces_result: bool,
    ) {
        // Sequential number for the new operation.
        let op_ref = OperationRef(self.operations_offset + self.operations.len());

        let mut dependencies = Vec::new();
        let mut resource_requirements = Vec::new();
        for precondition in preconditions {
            match precondition {
                Precondition::ResourceIsAvailable(resource) => {
                    if !resource_requirements.contains(&resource) {
                        resource_requirements.push(resource);
                    }
                }
            }
        }

        // Require that last operation previously scheduled on the same target as this one is
        // completed (for each target) before this operation can start.
        dependencies.extend(
            targets
                .iter()
                .filter_map(|target| self.last_operations.get(target).copied()),
        );
        let mut seen = FxHashSet::default();
        dependencies
            .retain(|dependency| !self.is_operation_done(*dependency) && seen.insert(*dependency));

        // If operation produces result, remember where it should write the result.
        let mut result_index = None;
        if produces_result {
            self.pending_operations_producing_results.insert(op_ref);
            result_index = Some(self.current_batch_result.results.len());
            self.current_batch_result
                .results
                .push(QuantumBackendResult::Error);
        }

        self.operations.push_back(OperationState {
            operation: Some(op),
            duration_nanos,
            result_index,
            pending_dependencies: dependencies.len(),
            dependents: Vec::new(),
            resource_requirements,
            started: false,
            done: false,
        });
        if dependencies.is_empty() {
            self.ready_operations.push_back(op_ref);
        }
        for dependency in dependencies {
            self.operations[dependency.0 - self.operations_offset]
                .dependents
                .push(op_ref);
        }
        for target in targets {
            self.last_operations.insert(target, op_ref);
        }
    }

    /// Schedules an operation without resource or target dependencies.
    /// The profiler dispatches it at the current simulated time after the model callback returns.
    pub fn schedule_operation_immediately(
        &mut self,
        op: M::NativeOperation,
        duration_nanos: usize,
    ) {
        self.schedule_operation(op, vec![], vec![], duration_nanos, false);
    }

    /// Publishes a resource and retries its waiters in the order they became resource-blocked.
    pub fn add_resource(&mut self, resource: M::Resource, unit: M::FunctionalUnitId) {
        if let Some(waiters) = self.operations_blocked_on_resources.remove(&resource) {
            self.ready_operations.extend(waiters);
        }
        self.available_resources
            .entry(resource)
            .or_default()
            .push_back(unit);
    }

    /// Checks completion of a scheduled reference, including retired operations.
    fn is_operation_done(&self, op_ref: OperationRef) -> bool {
        if op_ref.0 < self.operations_offset {
            return true;
        }
        self.operations
            .get(op_ref.0 - self.operations_offset)
            .expect("dependency must refer to an already scheduled operation")
            .done
    }

    /// Unblocks dependents and retires the completed prefix without reusing operation IDs.
    fn mark_operation_done(&mut self, op_ref: OperationRef) {
        let operation = &mut self.operations[op_ref.0 - self.operations_offset];
        assert!(!operation.done, "operation completed more than once");
        operation.done = true;
        for dependent in std::mem::take(&mut operation.dependents) {
            let operation = &mut self.operations[dependent.0 - self.operations_offset];
            operation.pending_dependencies -= 1;
            if operation.pending_dependencies == 0 {
                self.ready_operations.push_back(dependent);
            }
        }
        while self
            .operations
            .front()
            .is_some_and(|operation| operation.done)
        {
            self.operations.pop_front();
            self.operations_offset += 1;
        }
    }
}

pub struct TemporalProfiler<M: QpuModel> {
    state: TemporalProfilerState<M>,
    model: M,
}

impl<M: QpuModel> TemporalProfiler<M> {
    /// Initializes the model and starts its initial operations with configured recording and randomness.
    fn new(
        model: M,
        record_trace: bool,
        aggregation_window_nanos: usize,
        seed: Option<u64>,
    ) -> Self {
        let mut profiler = TemporalProfiler {
            state: TemporalProfilerState::new(
                seed,
                EventRecorder::new(record_trace, aggregation_window_nanos),
            ),
            model,
        };
        profiler.model.on_start(&mut profiler.state);
        profiler.start_ready_operations();
        profiler
    }

    /// Starts dependency-ready operations or queues them on their first unavailable resource.
    fn start_ready_operations(&mut self) {
        while let Some(op_ref) = self.state.ready_operations.pop_front() {
            if self.state.is_operation_done(op_ref) {
                continue;
            }
            let index = op_ref.0 - self.state.operations_offset;
            let operation = &self.state.operations[index];
            if operation.started || operation.pending_dependencies != 0 {
                continue;
            }
            if let Some(resource) = operation.resource_requirements.iter().find(|resource| {
                self.state
                    .available_resources
                    .get(resource)
                    .is_none_or(VecDeque::is_empty)
            }) {
                self.state
                    .operations_blocked_on_resources
                    .entry(resource.clone())
                    .or_default()
                    .push_back(op_ref);
                continue;
            }
            let end_time_nanos = self.state.current_time_nanos + operation.duration_nanos;
            let sequence = self.state.next_event_sequence;
            self.state.next_event_sequence = sequence + 1;
            let mut op = self.state.operations[index]
                .operation
                .take()
                .expect("unstarted operation must be present");
            self.state.operations[index].started = true;
            self.model.on_operation_started(&mut op, &mut self.state);
            self.state.event_recorder.event_aggregator.record_event(
                self.state.current_time_nanos,
                op.get_name(),
                1,
            );
            if self.state.event_recorder.record_trace {
                self.state.event_recorder.logs.push(InstructionTrace {
                    start_time_nanos: self.state.current_time_nanos,
                    end_time_nanos,
                    instruction: op.to_logged_instruction(),
                });
            }
            self.state.operations[index].operation = Some(op);
            self.state
                .events
                .push(Reverse((end_time_nanos, sequence, op_ref)));
        }
    }

    /// Completes one operation, records its result, and starts work enabled by its callbacks.
    fn complete_operation(&mut self, op_ref: OperationRef) {
        let index = op_ref.0 - self.state.operations_offset;
        let op = self.state.operations[index]
            .operation
            .take()
            .expect("finishing operation must be present");
        let result_index = self.state.operations[index].result_index;
        let result = self.model.on_operation_finished(&op, &mut self.state);
        match (result_index, result) {
            (Some(index), Some(result)) => {
                self.state.current_batch_result.results[index] = result;
                assert!(
                    self.state
                        .pending_operations_producing_results
                        .remove(&op_ref)
                );
            }
            (None, None) => {}
            _ => panic!("operation result does not match its scheduled result slot"),
        }
        self.state.mark_operation_done(op_ref);
        self.start_ready_operations();
    }

    /// Advances completion events only until all requested results are available.
    fn event_loop(&mut self) -> Result<(), String> {
        while !self.state.pending_operations_producing_results.is_empty() {
            let Reverse((time, _, op_ref)) = self
                .state
                .events
                .pop()
                .ok_or("event queue exhausted before all requested results completed")?;
            assert!(
                time >= self.state.current_time_nanos,
                "event time moved backwards"
            );
            self.state.current_time_nanos = time;
            self.complete_operation(op_ref);
        }
        Ok(())
    }
}

/// Creates the demo-model profiler from the shared temporal profiling configuration.
pub fn make_profiler_for_demo_model(
    config: TemporalProfilerConfig,
) -> TemporalProfiler<DemoModel1> {
    TemporalProfiler::<DemoModel1>::new(
        DemoModel1::from_config(config),
        config.record_trace,
        config.aggregation_window_nanos,
        config.seed,
    )
}

impl<M: QpuModel> QuantumBackend for TemporalProfiler<M> {
    /// Schedules a batch and runs only until all requested results are available, without a final drain.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        if !self.state.pending_operations_producing_results.is_empty() {
            return Err(
                "cannot submit a new batch while results from a failed batch are pending".into(),
            );
        }
        self.state.current_batch_result = BatchResult::default();

        for instruction in instructions {
            self.model
                .on_instruction_received(instruction, &mut self.state)?;
        }

        self.start_ready_operations();
        self.event_loop()?;
        Ok(std::mem::take(&mut self.state.current_batch_result))
    }
}

impl<M: QpuModel> Profiler for TemporalProfiler<M> {
    /// Returns model resource counts together with recorded profiles and trace.
    fn get_result(&self) -> ProfilerResult {
        ProfilerResult {
            totals: self.model.get_total_counts(&self.state),
            profiles: self.state.event_recorder.get_profiles(),
            trace: self.state.event_recorder.get_trace(),
            infos: vec![],
        }
    }
}
