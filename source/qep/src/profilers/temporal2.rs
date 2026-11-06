// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
use super::models::DemoModel1;
use super::models::QpuModel;
use crate::{
    QuantumBackendResult,
    interface::{BatchResult, QuantumBackend, QuantumBackendInstruction},
    profilers::factories::TFactory,
};
use rand::{SeedableRng, rngs::StdRng};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use std::{
    cmp::{Ordering, Reverse},
    collections::{BinaryHeap, VecDeque},
    marker::PhantomData,
};
use strum::EnumCount;

/// State of functional unit.
/// Common for all units, so need to be very generic.
/// Each specific unit is a state machine over some subset of these states.
#[derive(Default, PartialEq, Copy, Clone)]
pub(super) enum FUState {
    #[default]
    Idle,
    Working,
    StateReady,
}

pub(super) struct LogicalComputeQubit {
    pub(super) physical_qubits: usize,
}

/// Types of functional units.
/// Common for all possible QPUs.
#[derive(EnumCount, Copy, Clone)]
pub(super) enum FUKind {
    LogicalComputeQubit,
    TFactory,
}

pub(super) enum FunctionalUnit {
    LogicalComputeQubit(LogicalComputeQubit),
    TFactory(TFactory),
}

impl FunctionalUnit {
    /// Logical qubits occupied by this functional unit.
    fn logical_qubits(&self) -> usize {
        match self {
            FunctionalUnit::LogicalComputeQubit(logical_compute_qubit) => 1,
            FunctionalUnit::TFactory(tfactory) => tfactory.logical_qubits,
        }
    }

    /// Physical qubits occupied by this functional unit.
    fn physical_qubits(&self) -> usize {
        match self {
            FunctionalUnit::LogicalComputeQubit(logical_compute_qubit) => 1,
            FunctionalUnit::TFactory(tfactory) => tfactory.physical_qubits(),
        }
    }
}

#[derive(Copy, Clone)]
pub(super) struct FunctionalUnitId(pub(super) FUKind, pub(super) usize);

#[derive(Eq, Ord, PartialEq, PartialOrd)]
struct EventId(usize);

enum Event<M: QpuModel> {
    OperationEnd(PerfomedOperation<M>),
}

struct QueuedEvent<M: QpuModel> {
    time_nanos: usize,
    id: EventId,
    operation_ref: ScheduledOperationRef,
    event: Event<M>,
}

impl<M: QpuModel> PartialEq for QueuedEvent<M> {
    /// Compares scheduling keys without comparing model-specific operations.
    fn eq(&self, other: &Self) -> bool {
        (self.time_nanos, &self.id) == (other.time_nanos, &other.id)
    }
}

impl<M: QpuModel> Eq for QueuedEvent<M> {}

impl<M: QpuModel> PartialOrd for QueuedEvent<M> {
    /// Uses chronological ordering with FIFO tie-breaking.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<M: QpuModel> Ord for QueuedEvent<M> {
    /// Orders events by time, then insertion sequence.
    fn cmp(&self, other: &Self) -> Ordering {
        (self.time_nanos, &self.id).cmp(&(other.time_nanos, &other.id))
    }
}

/// Contsais functional unit plus necessary metadata.
pub(super) struct FunctionalUnitWrapper {
    pub(super) unit: FunctionalUnit,
    pub(super) last_scheduled_operation: Option<ScheduledOperationRef>,
}

impl FunctionalUnitWrapper {
    fn new(unit: FunctionalUnit) -> Self {
        FunctionalUnitWrapper {
            unit,
            last_scheduled_operation: None,
        }
    }
}

/// Groups all functional units on QPU.
/// Provides access to unit by ID.
/// Keeps track of which unit are used and in the end reports logical and physical qubits usage.
pub(super) struct FunctionalUnitManager<M: QpuModel> {
    units: [Vec<FunctionalUnitWrapper>; FUKind::COUNT],
    model: PhantomData<M>,
}

impl<M: QpuModel> Default for FunctionalUnitManager<M> {
    /// Creates an empty manager without requiring a default model.
    fn default() -> Self {
        Self {
            units: Default::default(),
            model: PhantomData,
        }
    }
}

impl<M: QpuModel> FunctionalUnitManager<M> {
    /// Returns immutable unit that must exist.
    /// Panics if unit doesn't exist.
    pub(super) fn get(&self, id: &FunctionalUnitId) -> &FunctionalUnitWrapper {
        let units = &self.units[id.0 as usize];
        debug_assert!(id.1 < units.len());
        &units[id.1]
    }

    /// Returns a unit, initializing missing units of its kind as needed.
    pub(super) fn get_mut(
        &mut self,
        model: &M,
        id: &FunctionalUnitId,
    ) -> &mut FunctionalUnitWrapper {
        let units = &mut self.units[id.0 as usize];
        while units.len() <= id.1 {
            let unit = model.create_functional_unit(id.0);
            units.push(FunctionalUnitWrapper::new(unit));
        }
        &mut units[id.1]
    }

    /// Changes state of given functional unit.
    /// Asserts that old state is as given.
    pub(super) fn change_state(
        &mut self,
        id: &FunctionalUnitId,
        old_state: FUState,
        new_state: FUState,
    ) {
        //let unit = self.get_mut(id);
        //debug_assert!(unit.state == old_state);
        //unit.state = new_state;
        // TODO: state change may have triggered an event.
        // TODO: update reverse map used in get_units_in_state.
    }

    /// Returns all functional units of given kind in given state.
    /// Intentionally immutable, to add/remove units from this list, use change_state.
    pub(super) fn get_units_in_state(
        &self,
        kind: FUKind,
        state: FUState,
    ) -> &Vec<FunctionalUnitId> {
        todo!()
    }

    /// Counts logical qubits across all functional units.
    fn logical_qubits(&self) -> usize {
        self.units
            .iter()
            .flatten()
            .map(|unit| unit.unit.logical_qubits())
            .sum()
    }

    /// Counts logical qubits across all functional units.
    fn physical_qubits(&self) -> usize {
        self.units
            .iter()
            .flatten()
            .map(|unit| unit.unit.physical_qubits())
            .sum()
    }
}

pub(super) enum Precondition {
    /// This event can start if operation with given ID is finished.
    OperationDone(ScheduledOperationRef),

    /// This event can start when we have at least as many functional units of given kind in give
    /// state.
    ResourceAvailable(FUKind, FUState, usize),
}

/// Unique identifier (counter) of all operations.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct ScheduledOperationRef(usize);

pub(super) struct ScheduledOperation<M: QpuModel> {
    pub(super) op: M::NativeOperation,
    pub(super) targets: SmallVec<[FunctionalUnitId; 2]>,
    pub(super) preconditions: SmallVec<[Precondition; 2]>,

    /// If this operation produces result, this is index in batch where it should be written.
    pub(super) result_index: Option<usize>,
}

pub(super) struct PerfomedOperation<M: QpuModel> {
    pub(super) op: M::NativeOperation,
    pub(super) targets: SmallVec<[FunctionalUnitId; 2]>,
    pub(super) start_time_nanos: usize,
    pub(super) end_time_nanos: usize,
}

pub struct TemporalProfilerState<M: QpuModel> {
    pub(super) current_time_nanos: usize,
    pub(super) units: FunctionalUnitManager<M>,
    pub(super) current_batch_result: BatchResult,

    /// Randomness source.
    pub(super) rng: StdRng,
}

impl<M: QpuModel> TemporalProfilerState<M> {
    /// Creates model-specific state with an optional random seed.
    fn new(seed: Option<u64>) -> Self {
        TemporalProfilerState {
            current_time_nanos: 0,
            units: FunctionalUnitManager::<M>::default(),
            current_batch_result: BatchResult::default(),
            rng: seed.map_or_else(|| StdRng::from_rng(&mut rand::rng()), StdRng::seed_from_u64),
        }
    }
}

pub struct TemporalProfiler2<M: QpuModel> {
    state: TemporalProfilerState<M>,
    model: M,

    /// Scheduled operations. `None` means operation has started.
    scheduled_operations: VecDeque<Option<ScheduledOperation<M>>>,
    running_operations: FxHashSet<ScheduledOperationRef>,
    next_operation_ref: ScheduledOperationRef,
    events_queue: BinaryHeap<Reverse<QueuedEvent<M>>>,
    next_event_id: usize,
}

impl<M: QpuModel> TemporalProfiler2<M> {
    /// Creates a profiler with no allocated functional units.
    fn new(model: M) -> Self {
        TemporalProfiler2 {
            state: TemporalProfilerState::new(None),
            model,
            scheduled_operations: Default::default(),
            running_operations: Default::default(),
            next_operation_ref: ScheduledOperationRef(0),
            events_queue: BinaryHeap::new(),
            next_event_id: 0,
        }
    }

    /// Whether sceduled operation is completed.
    fn is_operation_done(&self, op_ref: ScheduledOperationRef) -> bool {
        let offset = self.next_operation_ref.0 - self.scheduled_operations.len();
        if op_ref.0 < offset {
            return true;
        }
        self.scheduled_operations[op_ref.0 - offset].is_none()
            && !self.running_operations.contains(&op_ref)
    }

    /// Returns a scheduled operation, or None if it has already started.
    fn get_scheduled_operation(
        &self,
        op_ref: ScheduledOperationRef,
    ) -> Option<&ScheduledOperation<M>> {
        let offset = self.next_operation_ref.0 - self.scheduled_operations.len();
        if op_ref.0 < offset {
            return None;
        }
        self.scheduled_operations[op_ref.0 - offset].as_ref()
    }

    fn is_precondition_satisfied(&self, precondition: &Precondition) -> bool {
        match precondition {
            Precondition::OperationDone(op_ref) => self.is_operation_done(*op_ref),
            Precondition::ResourceAvailable(kind, state, min_units) => {
                self.state.units.get_units_in_state(*kind, *state).len() >= *min_units
            }
        }
    }

    /// Starts an instruction when all its preconditions are satisdied.
    /// Does nothing otherwise.
    fn try_operation_start(&mut self, op_ref: ScheduledOperationRef) {
        let offset = self.next_operation_ref.0 - self.scheduled_operations.len();
        if op_ref.0 < offset {
            // Operation is done.
            return;
        }
        let index = op_ref.0 - offset;
        let Some(op) = self.scheduled_operations[index].as_ref() else {
            // Operation has already started.
            return;
        };
        let ready = op
            .preconditions
            .iter()
            .all(|p| self.is_precondition_satisfied(p));
        if !ready {
            // Operation is not ready.
            return;
        }

        // Operation is ready. Model its execution.
        let op = self.scheduled_operations[index]
            .take()
            .expect("ready operation must remain scheduled");
        let op = self.model.perform_operation(op, &mut self.state);

        self.running_operations.insert(op_ref);
        self.enqueue_operation_end(op_ref, op);
    }

    /// Queues an owned operation for completion, preserving FIFO order at equal times.
    fn enqueue_operation_end(
        &mut self,
        operation_ref: ScheduledOperationRef,
        op: PerfomedOperation<M>,
    ) {
        self.events_queue.push(Reverse(QueuedEvent {
            time_nanos: op.end_time_nanos,
            id: EventId(self.next_event_id),
            operation_ref,
            event: Event::OperationEnd(op),
        }));
        self.next_event_id += 1;
    }

    /// Notifies the model of completion and starts newly enabled operations.
    fn process_event(&mut self, operation_ref: ScheduledOperationRef, event: Event<M>) {
        match event {
            Event::OperationEnd(op) => self.model.on_operation_finished(&op, &mut self.state),
        }
        self.running_operations.remove(&operation_ref);

        let mut offset = self.next_operation_ref.0 - self.scheduled_operations.len();
        while self
            .scheduled_operations
            .front()
            .is_some_and(|op| op.is_none())
            && !self
                .running_operations
                .contains(&ScheduledOperationRef(offset))
        {
            self.scheduled_operations.pop_front();
            offset += 1;
        }

        for index in offset..self.next_operation_ref.0 {
            self.try_operation_start(ScheduledOperationRef(index));
        }
    }

    /// Processes the earliest events until all scheduled operations have completed.
    fn event_loop(&mut self) {
        while !self.scheduled_operations.is_empty() {
            let Reverse(event) = self
                .events_queue
                .pop()
                .expect("event queue exhausted before the batch completed");
            debug_assert!(event.time_nanos >= self.state.current_time_nanos);
            self.state.current_time_nanos = event.time_nanos;
            self.process_event(event.operation_ref, event.event);
        }
    }
}

impl<M: QpuModel> QuantumBackend for TemporalProfiler2<M> {
    /// Builds dependencies, simulates the batch, and returns measurement results.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        // Initialize the batch result.
        self.state.current_batch_result = BatchResult::default();

        // Schedule all instructions.
        let mut scheduled_ops = vec![];
        for instr in instructions {
            // First, map it to native operation.
            let mut op = self.model.schedule_instruction(instr)?;

            // Add precondition that for all FUs touched by this operation, previous operation on
            // that FU must complete.
            for target_id in &op.targets {
                if let Some(prev_op_ref) = self
                    .state
                    .units
                    .get_mut(&self.model, target_id)
                    .last_scheduled_operation
                {
                    op.preconditions
                        .push(Precondition::OperationDone(prev_op_ref));
                }
            }

            // If instruction produces result, add correct result_index.
            if op.result_index.is_some() {
                let index = self.state.current_batch_result.results.len();
                self.state
                    .current_batch_result
                    .results
                    .push(QuantumBackendResult::Error);
                op.result_index = Some(index);
            }

            // Assign sequential ID.
            let op_ref = self.next_operation_ref;
            scheduled_ops.push(op_ref);
            self.next_operation_ref.0 += 1;

            // Record this operation as last scheduled operation for all its targets.
            for target_id in &op.targets {
                self.state
                    .units
                    .get_mut(&self.model, &target_id)
                    .last_scheduled_operation = Some(op_ref);
            }

            // Add this operation to the queue.
            self.scheduled_operations.push_back(Some(op));
        }

        // Kick off batch execution by trying to execute every operation in the batch.
        for op_ref in scheduled_ops {
            self.try_operation_start(op_ref);
        }
        self.event_loop();

        // Return batch result.
        Ok(std::mem::take(&mut self.state.current_batch_result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic_test() {
        let model = DemoModel1::default();
        let p = TemporalProfiler2::new(model);
    }
}
