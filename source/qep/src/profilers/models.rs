use super::temporal2::{
    FUKind, FUState, FunctionalUnitId, LogicalComputeQubit, PerfomedOperation, Precondition,
    ScheduledOperation, TemporalProfilerState,
};
use crate::{
    Parameter, QuantumBackendResult,
    interface::QuantumBackendInstruction,
    profilers::{factories::TFactory, temporal2::FunctionalUnit},
};
use rand::RngExt;
use smallvec::{SmallVec, smallvec};

pub(super) trait QpuModel: Sized {
    type NativeOperation;

    fn create_functional_unit(&self, kind: FUKind) -> FunctionalUnit;

    /// Translates QSP instruction to native operation.
    ///
    /// If instruction produces result, must set result_index to Some(...), value doesn't matter.
    /// Otherwise, must set result_index to None.
    fn schedule_instruction(
        &self,
        instr: &QuantumBackendInstruction,
    ) -> Result<ScheduledOperation<Self>, String>;

    /// Given scheduled operation, "performs" it by specifying on which functional units it should
    /// be performed, and what is start and end time.
    /// This method can assume that all precoinditions are satisfied, and panic if they are not.
    fn perform_operation(
        &self,
        op: ScheduledOperation<Self>,
        state: &mut TemporalProfilerState<Self>,
    ) -> PerfomedOperation<Self>;
}

pub(super) enum DemoModel1_NativeOperation {
    SingleQubitCliffordGate(&'static str),
    CnotGate,
    Measurement,
    Reset,
    InjectT,
    InjectTAdj,
    DistillationRound,
}

/// Simplisitic model for demos:
///  * Functional units:
///    * Logical Compute Qubits, number is not fixed in advance.
///    * T-factroies, number fixed in advance.
///      * Each T-factory consists of 5 logical qubits.
///      * T-factory produces TStates.
///    * Both standalone LCQs and LCQs inside factories use Surface code with distance ecc_distance.
///  * Resources:
///    * T-states.
///  * Supported operations:
///    * Supports Clifford gates, each takes `clifford_gate_time_nanos`.
///    * Supports measurments, each takes `measurment_time_nanos`.
///      * Each measurments returns |0> or |1> with probability 50%.
///    * Supports T gates which is implemented by InjectT operation which consumes TState.
///    * Supports AdjointT gates which is implemented by InjectAdjointT operation which consumes TState.
pub(super) struct DemoModel1 {
    /// Duration of a Clifford instruction in nanoseconds.
    pub clifford_gate_time_nanos: usize,
    /// Duration of measurement, reset, or their combination in nanoseconds.
    pub measurment_time_nanos: usize,
    pub t_factories_count: usize,
    /// Distance of the error-coorecting code.
    /// Assumes that all logical qubits have the same error correcting code.
    /// TODO: change it to specify different ECCs for different qubits.
    pub ecc_distance: usize,
}

impl Default for DemoModel1 {
    /// Uses ten factories, one-second windows, unseeded randomness, and no trace recording.
    fn default() -> Self {
        Self {
            clifford_gate_time_nanos: 5_000,
            measurment_time_nanos: 1_000,
            t_factories_count: 10,
            ecc_distance: 5,
        }
    }
}

impl DemoModel1 {
    /// Time to perform "InjectT" or "InjectT Adjoint" gate.
    fn inject_t_time(&self) -> usize {
        2 * self.clifford_gate_time_nanos + self.measurment_time_nanos
    }
}

impl QpuModel for DemoModel1 {
    type NativeOperation = DemoModel1_NativeOperation;

    fn create_functional_unit(&self, kind: FUKind) -> FunctionalUnit {
        let d = self.ecc_distance;
        match kind {
            FUKind::LogicalComputeQubit => {
                FunctionalUnit::LogicalComputeQubit(LogicalComputeQubit {
                    physical_qubits: (2 * d * d - 1),
                })
            }
            FUKind::TFactory => FunctionalUnit::TFactory(TFactory::new(d)),
        }
    }

    fn schedule_instruction(
        &self,
        instruction: &QuantumBackendInstruction,
    ) -> Result<ScheduledOperation<Self>, String> {
        let targets: SmallVec<[FunctionalUnitId; 2]> = instruction
            .parameters
            .iter()
            .filter_map(|p| match p {
                Parameter::QubitId(id) => Some(FunctionalUnitId(FUKind::LogicalComputeQubit, *id)),
                _ => None,
            })
            .collect();

        let native_instruction = match instruction.instruction_name {
            "h" | "s" | "sadj" | "x" | "y" | "z" => {
                DemoModel1_NativeOperation::SingleQubitCliffordGate(instruction.instruction_name)
            }
            "cx" => DemoModel1_NativeOperation::CnotGate,
            "mz" => DemoModel1_NativeOperation::Measurement,
            "reset" => DemoModel1_NativeOperation::Reset,
            "t" => DemoModel1_NativeOperation::InjectT,
            "t_adj" => DemoModel1_NativeOperation::InjectTAdj,
            // All other instructions have to be transpiled.
            _ => {
                return Err(format!("Unsupported instruction: {instruction:?}"));
            }
        };

        let preconditions = match instruction.instruction_name {
            "t" | "t_adj" => smallvec![Precondition::ResourceAvailable(
                FUKind::TFactory,
                FUState::StateReady,
                1
            )],
            _ => smallvec![],
        };

        let result_index = match instruction.instruction_name {
            "mz" => Some(0),
            _ => None,
        };

        Ok(ScheduledOperation {
            op: native_instruction,
            targets,
            preconditions,
            result_index,
        })
    }

    fn perform_operation(
        &self,
        op: ScheduledOperation<Self>,
        state: &mut TemporalProfilerState<Self>,
    ) -> PerfomedOperation<Self> {
        let duration = match op.op {
            DemoModel1_NativeOperation::SingleQubitCliffordGate(_) => self.clifford_gate_time_nanos,
            DemoModel1_NativeOperation::CnotGate => self.clifford_gate_time_nanos,
            DemoModel1_NativeOperation::Measurement => self.measurment_time_nanos,
            DemoModel1_NativeOperation::Reset => self.measurment_time_nanos,
            DemoModel1_NativeOperation::InjectT => self.inject_t_time(),
            DemoModel1_NativeOperation::InjectTAdj => self.inject_t_time(),
            DemoModel1_NativeOperation::DistillationRound => {
                let FunctionalUnit::TFactory(factory) = &state.units.get(&op.targets[0]).unit
                else {
                    panic!("distillation round requires a TFactory");
                };
                factory.distillation_round_duration_nanos()
            }
        };
        let mut targets = op.targets;

        // Only for InjectT and its adjoint: find factory that has available T-state, consume that
        // T state, and add factory to list of units on which this operation operates.
        if matches!(
            op.op,
            DemoModel1_NativeOperation::InjectT | DemoModel1_NativeOperation::InjectTAdj
        ) {
            let available_factories = state
                .units
                .get_units_in_state(FUKind::TFactory, FUState::StateReady);
            let factory_id = available_factories.iter().next().unwrap().clone();
            state
                .units
                .change_state(&factory_id, FUState::StateReady, FUState::Idle);
            targets.push(factory_id);
        }

        // Only for measurment - write measurment result into batch.
        if matches!(op.op, DemoModel1_NativeOperation::Measurement) {
            state.current_batch_result.results[op.result_index.unwrap()] =
                QuantumBackendResult::from(state.rng.random::<bool>());
        }

        PerfomedOperation {
            op: op.op,
            targets,
            start_time_nanos: state.current_time_nanos,
            end_time_nanos: state.current_time_nanos + duration,
        }
    }
}
