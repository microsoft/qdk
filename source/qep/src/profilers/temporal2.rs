// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::models::{DemoModel1, QpuModel};
use crate::{
    QuantumBackendResult, TemporalProfilerConfig,
    interface::{BatchResult, QuantumBackend, QuantumBackendInstruction},
    profilers::{event_recorder::EventRecorder, factories::TFactory},
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

/// Internal operation reference
pub(super) struct OperationRef(usize);

/// Identifier used solely for dependency resolution.
struct BusyUnitKey(usize);

pub(super) enum Precondition<M: QpuModel> {
    OperationIsCompleted(OperationRef),
    ResourceIsAvailable(M::Resource),
}

pub struct TemporalProfilerState<M: QpuModel> {
    current_time_nanos: usize,
    current_batch_result: BatchResult,
    pub rng: StdRng,
    model: PhantomData<M>,
}

impl<M: QpuModel> TemporalProfilerState<M> {
    /// Creates model-specific state with an optional random seed.
    fn new(seed: Option<u64>) -> Self {
        TemporalProfilerState {
            current_time_nanos: 0,
            current_batch_result: BatchResult::default(),
            rng: seed.map_or_else(|| StdRng::from_rng(&mut rand::rng()), StdRng::seed_from_u64),
            model: PhantomData::default(),
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
    event_recorder: EventRecorder,
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
            state: TemporalProfilerState::new(seed),
            model,
            event_recorder: EventRecorder::new(record_trace, aggregation_window_nanos),
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

        // Return batch result.
        Ok(std::mem::take(&mut self.state.current_batch_result))
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
}
