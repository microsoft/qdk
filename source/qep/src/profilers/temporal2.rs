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
}

impl<M: QpuModel> TemporalProfiler2<M> {
    /// Creates a profiler with no allocated functional units.
    fn new(model: M) -> Self {
        TemporalProfiler2 {
            state: TemporalProfilerState::new(None),
            model,
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
