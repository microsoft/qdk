// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    Profiler, ProfilerResult, TotalCounts,
    models::{DemoModel1, QpuModel},
};
use crate::{
    QuantumBackendResult, TemporalProfilerConfig,
    interface::{BatchResult, QuantumBackend, QuantumBackendInstruction},
    profilers::{event_recorder::EventRecorder, factories::TFactory},
};
use rand::{SeedableRng, rngs::StdRng};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::{
    cmp::{Ordering, Reverse},
    collections::{BinaryHeap, VecDeque},
    marker::PhantomData,
};
use strum::EnumCount;

/// Internal operation reference
pub(super) struct OperationRef(usize);

/// Identifier used solely for dependency resolution.
struct BusyUnitKey(usize);

pub(super) enum Precondition<M: QpuModel> {
    OperationIsCompleted(OperationRef),
    ResourceIsAvailable(M::Resource),
}

pub struct TemporalProfilerState<M: QpuModel> {
    pub current_time_nanos: usize,
    current_batch_result: BatchResult,
    pub event_recorder: EventRecorder,
    pub rng: StdRng,

    /// For each resource, holds a list of functional units that have that resource.
    pub(super) available_resources: FxHashMap<M::Resource, VecDeque<M::FunctionalUnitId>>,
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
        }
    }

    /// Schedules operation for execution as soon as all preconditions are satisifed.
    /// If all preconditions are satisfied now (or there is no preconditions), operation starts immediately.
    pub fn schedule_operation(
        &mut self,
        op: M::NativeOperation,
        preconditions: Vec<Precondition<M>>,
        targets: Vec<M::FunctionalUnitId>,
        duration_nanos: usize,
    ) {
        //- typically called from model.on_instruction_received()
        // - Computes additional prerequisites of type "OperationIsCompleted" for
        //     each operation that is last scheduled operation on each target of this operation.
        //- Computes sequential OperationRef for this instruction.
        //- Adds instruction to queue.
        //- Updates last scheduled instruction for all functional units.
    }

    /// Schedules an operation without resource or target dependencies.
    pub fn schedule_operation_immediately(
        &mut self,
        op: M::NativeOperation,
        duration_nanos: usize,
    ) {
        self.schedule_operation(op, vec![], vec![], duration_nanos);
    }
}

pub struct TemporalProfiler2<M: QpuModel> {
    state: TemporalProfilerState<M>,
    model: M,
}

impl<M: QpuModel> TemporalProfiler2<M> {
    /// Creates a profiler with configured recording and randomness, without allocated functional units.
    fn new(
        model: M,
        record_trace: bool,
        aggregation_window_nanos: usize,
        seed: Option<u64>,
    ) -> Self {
        TemporalProfiler2 {
            state: TemporalProfilerState::new(
                seed,
                EventRecorder::new(record_trace, aggregation_window_nanos),
            ),
            model,
        }
    }
}

/// Temporary helper to create TemporalProfiler2 with observable behaviour equivalent to
/// TemporalProfiler.
/// In the future, we will split model and config.
fn make_profiler(config: TemporalProfilerConfig) -> TemporalProfiler2<DemoModel1> {
    TemporalProfiler2::<DemoModel1>::new(
        DemoModel1::from_config(config),
        config.record_trace,
        config.aggregation_window_nanos,
        config.seed,
    )
}

impl<M: QpuModel> QuantumBackend for TemporalProfiler2<M> {
    /// Builds dependencies, simulates the batch, and returns measurement results.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        // Initialize the batch result.
        self.state.current_batch_result = BatchResult::default();

        for instruction in instructions {
            self.model
                .on_instruction_received(instruction, &mut self.state)?;
        }

        // TODO: process pending events until all results are ready.

        // Return batch result.
        Ok(std::mem::take(&mut self.state.current_batch_result))
    }
}

impl<M: QpuModel> Profiler for TemporalProfiler2<M> {
    /// Returns recorded profiles and trace with placeholder zero totals.
    fn get_result(&self) -> ProfilerResult {
        ProfilerResult {
            totals: TotalCounts::default(),
            profiles: self.state.event_recorder.get_profiles(),
            trace: self.state.event_recorder.get_trace(),
            infos: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a profiler instance.
    #[test]
    fn basic_test() {
        let _profiler = make_profiler(TemporalProfilerConfig::default());
    }

    /// Injections consume factories in FIFO order while resource-free operations leave them untouched.
    #[test]
    fn starting_injections_reserves_available_factories() {
        use super::super::models::{DemoModel1_NativeOperation, DemoModel1_Resource};

        let mut profiler = make_profiler(TemporalProfilerConfig::default());
        profiler
            .state
            .available_resources
            .insert(DemoModel1_Resource::TState, VecDeque::from([3, 5]));
        let mut clifford = DemoModel1_NativeOperation::SingleQubitCliffordGate("h", 7);
        profiler
            .model
            .on_operation_started(&mut clifford, &mut profiler.state);
        for (factory, adjoint) in [(3, false), (5, true)] {
            let mut op = DemoModel1_NativeOperation::InjectT(None, 7, adjoint);
            profiler
                .model
                .on_operation_started(&mut op, &mut profiler.state);
            assert!(matches!(
                op,
                DemoModel1_NativeOperation::InjectT(Some(id), 7, is_adjoint)
                    if id == factory && is_adjoint == adjoint
            ));
        }
        assert!(profiler.state.available_resources[&DemoModel1_Resource::TState].is_empty());
    }
}
