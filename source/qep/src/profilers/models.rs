use super::{
    profile_types::{LoggedInstruction, LoggedInstructionParameter},
    temporal2::{Precondition, TemporalProfilerState},
};
use crate::{
    Parameter, QuantumBackendResult, TemporalProfilerConfig, interface::QuantumBackendInstruction,
    profilers::factories::TFactory,
};
use rand::RngExt;

pub(super) trait QpuModel: Sized {
    type NativeOperation: Into<LoggedInstruction>;
    type Resource;
    type FunctionalUnitId;

    /// Initializes model on startup.
    /// Example: initialize factories.
    fn on_start(&mut self, state: &mut TemporalProfilerState<Self>);

    /// Does necessary conversion to native operation and calls state.schedule_operation. Also computes prerequisites
    ///  For example, for T gate it will schedule InjectT with prerequisite on having resource TState.
    ///  The intention is that TemporalProfiler2.execute_batch() will call model.on_instruction_received() in a loop for all instructions in batch.
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
    fn on_operation_started(&mut self, op: &mut Self::NativeOperation);

    /// Does necessary updates to model operation completion.
    /// Returns result if this operation produces result, otherwise retuns None.
    fn on_operation_finished(
        &mut self,
        op: &Self::NativeOperation,
        state: &mut TemporalProfilerState<Self>,
    ) -> Option<QuantumBackendResult>;
}

pub(super) enum DemoModel1_NativeOperation {
    SingleQubitCliffordGate(&'static str, usize), // Gate name, QubitId.
    CnotGate(usize, usize),                       // QubitId, QubitId.
    Measurement(usize),                           // QubitId.
    Reset(usize),                                 // QubitId.
    InjectT(Option<usize>, usize),                // FactoryId, QubitId.
    InjectTAdj(Option<usize>, usize),             // FactoryId, QubitId.
    DistillationRound(usize),                     // FactoryId.
}

/// Serialization of native instructions, needed only for visualizing trace.
impl From<DemoModel1_NativeOperation> for LoggedInstruction {
    fn from(operation: DemoModel1_NativeOperation) -> Self {
        use DemoModel1_NativeOperation::{
            CnotGate, DistillationRound, InjectT, InjectTAdj, Measurement, Reset,
            SingleQubitCliffordGate,
        };
        use LoggedInstructionParameter::{FactoryId, QubitId};

        let instruction_name = match &operation {
            SingleQubitCliffordGate(name, _) => *name,
            CnotGate(..) => "cx",
            Measurement(_) => "mz",
            Reset(_) => "reset",
            InjectT(..) => "inject_t",
            InjectTAdj(..) => "inject_tadj",
            DistillationRound(_) => "round",
        };
        let parameters = match operation {
            SingleQubitCliffordGate(_, qubit) | Measurement(qubit) | Reset(qubit) => {
                vec![QubitId(qubit)]
            }
            CnotGate(control, target) => vec![QubitId(control), QubitId(target)],
            InjectT(factory, qubit) | InjectTAdj(factory, qubit) => {
                let mut parameters = Vec::with_capacity(2);
                if let Some(factory) = factory {
                    parameters.push(FactoryId(factory));
                }
                parameters.push(QubitId(qubit));
                parameters
            }
            DistillationRound(factory) => vec![FactoryId(factory)],
        };
        Self {
            instruction_name: instruction_name.into(),
            parameters,
        }
    }
}

pub(super) enum DemoModel1_Resource {
    TState,
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
    pub t_factories: Vec<TFactory>,
    /// Distance of the error-coorecting code.
    /// Assumes that all logical qubits have the same error correcting code.
    /// TODO: change it to specify different ECCs for different qubits.
    pub ecc_distance: usize,
}

impl Default for DemoModel1 {
    /// Uses ten factories, one-second windows, unseeded randomness, and no trace recording.
    fn default() -> Self {
        Self::from_config(TemporalProfilerConfig::default())
    }
}

impl DemoModel1 {
    /// Using TemporalProfilerConfig only temporary for ease of migration.
    /// TODO: remove all model-specific fields from TemporalProfilerConfig.
    fn from_config(config: TemporalProfilerConfig) -> Self {
        DemoModel1::new(
            config.clifford_gate_time_nanos,
            config.measurment_time_nanos,
            config.t_factories_count,
            config.ecc_distance,
        )
    }

    /// Creates the configured factories and sets the model's operation durations.
    fn new(
        clifford_gate_time_nanos: usize,
        measurment_time_nanos: usize,
        t_factories_count: usize,
        ecc_distance: usize,
    ) -> Self {
        Self {
            clifford_gate_time_nanos,
            measurment_time_nanos,
            t_factories: vec![TFactory::new(ecc_distance); t_factories_count],
            ecc_distance,
        }
    }

    /// Time to perform "InjectT" or "InjectT Adjoint" gate.
    fn inject_t_time(&self) -> usize {
        2 * self.clifford_gate_time_nanos + self.measurment_time_nanos
    }

    /// Schedules and records one distillation attempt on an unoccupied factory.
    fn start_distillation_round(
        &mut self,
        factory_id: usize,
        state: &mut TemporalProfilerState<Self>,
    ) {
        let op = DemoModel1_NativeOperation::DistillationRound(factory_id);
        let duration = self.t_factories[factory_id].distillation_round_duration_nanos();
        state.schedule_operation_immediately(op, duration);

        // TODO: record trace and call aggregattors similar to TemporalProfiler::start_distillation_round
    }
}

impl QpuModel for DemoModel1 {
    type NativeOperation = DemoModel1_NativeOperation;
    type Resource = DemoModel1_Resource;
    type FunctionalUnitId = usize;

    /// Starts the first distillation round on every configured T factory.
    fn on_start(&mut self, state: &mut TemporalProfilerState<Self>) {
        // Start distillation round on each factory.
        for factory_id in 0..self.t_factories.len() {
            self.start_distillation_round(factory_id, state);
        }
    }

    /// Schedules native operations with resource requirements and dependencies on each target qubit.
    fn on_instruction_received(
        &mut self,
        instruction: &QuantumBackendInstruction,
        state: &mut TemporalProfilerState<Self>,
    ) -> Result<(), String> {
        let op = match (
            instruction.instruction_name,
            instruction.parameters.as_slice(),
        ) {
            ("cx", [Parameter::QubitId(control), Parameter::QubitId(target)]) => {
                DemoModel1_NativeOperation::CnotGate(*control, *target)
            }
            (name @ ("h" | "s" | "sadj" | "x" | "y" | "z"), [Parameter::QubitId(qubit)]) => {
                DemoModel1_NativeOperation::SingleQubitCliffordGate(name, *qubit)
            }
            ("mz", [Parameter::QubitId(qubit)]) => DemoModel1_NativeOperation::Measurement(*qubit),
            ("reset", [Parameter::QubitId(qubit)]) => DemoModel1_NativeOperation::Reset(*qubit),
            ("t", [Parameter::QubitId(qubit)]) => DemoModel1_NativeOperation::InjectT(None, *qubit),
            ("tadj", [Parameter::QubitId(qubit)]) => {
                DemoModel1_NativeOperation::InjectTAdj(None, *qubit)
            }
            _ => return Err(format!("Unsupported instruction: {instruction:?}")),
        };

        let duration_nanos = match instruction.instruction_name {
            "cx" | "h" | "s" | "sadj" | "x" | "y" | "z" => self.clifford_gate_time_nanos,
            "mz" | "reset" => self.measurment_time_nanos,
            "t" | "tadj" => self.inject_t_time(),
            _ => {
                return Err(format!("Unsupported instruction: {instruction:?}"));
            }
        };

        let preconditions = match instruction.instruction_name {
            "t" | "tadj" => vec![Precondition::ResourceIsAvailable(
                DemoModel1_Resource::TState,
            )],
            _ => vec![],
        };

        let targets = instruction
            .parameters
            .iter()
            .filter_map(|parameter| match parameter {
                Parameter::QubitId(qubit) => Some(*qubit),
                _ => None,
            })
            .collect();

        state.schedule_operation(op, preconditions, targets, duration_nanos);
        Ok(())
    }

    fn on_operation_started(&mut self, op: &mut Self::NativeOperation) {
        todo!()
        // TODO: for T/TAdj, find what factory we are going to use, and mutate op to refer to that
        // factory.
    }

    fn on_operation_finished(
        &mut self,
        op: &Self::NativeOperation,
        state: &mut TemporalProfilerState<Self>,
    ) -> Option<QuantumBackendResult> {
        match op {
            DemoModel1_NativeOperation::Measurement(_) => {
                Some(QuantumBackendResult::from(state.rng.random::<bool>()))
            }
            _ => None,
        }

        // TODO: if this is InjectT/InjectTAdj, must release factory.
        // TODO: logging for profiling.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DemoModel1_NativeOperation::{
        CnotGate, DistillationRound, InjectT, InjectTAdj, Measurement, Reset,
        SingleQubitCliffordGate,
    };
    use LoggedInstructionParameter::{FactoryId, QubitId};

    /// Preserves trace names and parameter order for every native operation and optional factory.
    #[test]
    fn native_operations_convert_to_logged_instructions() {
        for (operation, name, parameters) in [
            (CnotGate(2, 3), "cx", vec![QubitId(2), QubitId(3)]),
            (Measurement(4), "mz", vec![QubitId(4)]),
            (Reset(5), "reset", vec![QubitId(5)]),
            (
                InjectT(Some(6), 7),
                "inject_t",
                vec![FactoryId(6), QubitId(7)],
            ),
            (InjectT(None, 7), "inject_t", vec![QubitId(7)]),
            (
                InjectTAdj(Some(8), 9),
                "inject_tadj",
                vec![FactoryId(8), QubitId(9)],
            ),
            (InjectTAdj(None, 9), "inject_tadj", vec![QubitId(9)]),
            (DistillationRound(10), "round", vec![FactoryId(10)]),
        ] {
            let logged: LoggedInstruction = operation.into();
            assert_eq!(logged.instruction_name, name);
            assert_eq!(logged.parameters, parameters);
        }
        for name in ["h", "s", "sadj", "x", "y", "z"] {
            let logged: LoggedInstruction = SingleQubitCliffordGate(name, 11).into();
            assert_eq!(logged.instruction_name, name);
            assert_eq!(logged.parameters, vec![QubitId(11)]);
        }
    }
}
