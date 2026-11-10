// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub(crate) mod demo_model1;
mod factories;
mod qubits;
mod surface_code;

use crate::{
    QuantumBackendInstruction, QuantumBackendResult, TotalCounts,
    profilers::{LoggableOperation, TemporalProfilerState},
};
use std::hash::Hash;

pub use demo_model1::DemoModel1;

pub trait QpuModel: Sized {
    /// Retained until completion and borrowed for start-time trace conversion.
    type NativeOperation: LoggableOperation;
    type Resource: Clone + Eq + Hash;
    type FunctionalUnitId: Eq + Hash;

    /// Initializes model on startup.
    /// Example: initialize factories.
    fn on_start(&mut self, state: &mut TemporalProfilerState<Self>);

    /// Does necessary conversion to native operation and calls state.schedule_operation. Also computes prerequisites
    ///  For example, for T gate it will schedule InjectT with prerequisite on having resource TState.
    ///  The intention is that TemporalProfiler.execute_batch() will call model.on_instruction_received() in a loop for all instructions in batch.
    fn on_instruction_received(
        &mut self,
        instruction: &QuantumBackendInstruction,
        state: &mut TemporalProfilerState<Self>,
    ) -> Result<(), String>;

    /// Does necessary updates to model operation start.
    /// Allowed to mutate operation.
    /// For example, for "InjectT" it will:
    ///   - find T factory with available T State.
    ///   - internally mark it as such that cannot be used (consume state).
    ///   - change op to contain selected factory_id.
    /// It is guaranteed that all prerequisites are met, and it's allowed to panic if they are not
    /// met.
    fn on_operation_started(
        &mut self,
        op: &mut Self::NativeOperation,
        state: &mut TemporalProfilerState<Self>,
    );

    /// Does necessary updates to model operation completion.
    /// Returns result if this operation produces result, otherwise retuns None.
    /// Publish generated resources through `state.add_resource` to wake waiting operations.
    fn on_operation_finished(
        &mut self,
        op: &Self::NativeOperation,
        state: &mut TemporalProfilerState<Self>,
    ) -> Option<QuantumBackendResult>;

    /// Returns resource counts for resource estimation.
    fn get_total_counts(&self, state: &TemporalProfilerState<Self>) -> TotalCounts;
}
