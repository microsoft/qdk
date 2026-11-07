use super::{
    profile_types::{LoggableOperation, LoggedInstruction, LoggedInstructionParameter},
    temporal2::{Precondition, TemporalProfilerState},
};
use crate::{
    Parameter, QuantumBackendResult, TemporalProfilerConfig, TotalCounts,
    interface::QuantumBackendInstruction, profilers::factories::TFactory,
};
use rand::RngExt;
use std::{collections::VecDeque, hash::Hash};

pub(super) trait QpuModel: Sized {
    /// Retained until completion and borrowed for start-time trace conversion.
    type NativeOperation: LoggableOperation;
    type Resource: Clone + Eq + Hash;
    type FunctionalUnitId: Eq + Hash;

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

pub(super) enum DemoModel1_NativeOperation {
    SingleQubitCliffordGate(&'static str, usize), // Gate name, QubitId.
    CnotGate(usize, usize),                       // QubitId, QubitId.
    Measurement(usize),                           // QubitId.
    Reset(usize),                                 // QubitId.
    InjectT(Option<usize>, usize, bool),          // FactoryId, QubitId, is_adjoint.
    DistillationRound(usize),                     // FactoryId.
}

impl LoggableOperation for DemoModel1_NativeOperation {
    /// Returns the native operation's static metric and trace name.
    fn get_name(&self) -> &str {
        match self {
            Self::SingleQubitCliffordGate(name, _) => name,
            Self::CnotGate(..) => "cx",
            Self::Measurement(_) => "mz",
            Self::Reset(_) => "reset",
            Self::InjectT(_, _, false) => "inject_t",
            Self::InjectT(_, _, true) => "inject_tadj",
            Self::DistillationRound(_) => "round",
        }
    }

    /// Preserves trace names and parameter order for native operations.
    fn to_logged_instruction(&self) -> LoggedInstruction {
        use LoggedInstructionParameter::{FactoryId, QubitId};

        let parameters = match self {
            Self::SingleQubitCliffordGate(_, qubit)
            | Self::Measurement(qubit)
            | Self::Reset(qubit) => {
                vec![QubitId(*qubit)]
            }
            Self::CnotGate(control, target) => vec![QubitId(*control), QubitId(*target)],
            Self::InjectT(factory, qubit, _) => {
                let mut parameters = Vec::with_capacity(2);
                if let Some(factory) = factory {
                    parameters.push(FactoryId(*factory));
                }
                parameters.push(QubitId(*qubit));
                parameters
            }
            Self::DistillationRound(factory) => vec![FactoryId(*factory)],
        };
        LoggedInstruction {
            instruction_name: self.get_name().to_owned().into(),
            parameters,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
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
    pub fn from_config(config: TemporalProfilerConfig) -> Self {
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
        assert!(t_factories_count > 0, "at least one T factory is required");
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

    /// Schedules one distillation attempt on an unoccupied factory.
    fn start_distillation_round(
        &mut self,
        factory_id: usize,
        state: &mut TemporalProfilerState<Self>,
    ) {
        let op = DemoModel1_NativeOperation::DistillationRound(factory_id);
        let duration = self.t_factories[factory_id].distillation_round_duration_nanos();
        state.schedule_operation_immediately(op, duration);
    }

    /// Publishes a successful T state or schedules another distillation attempt after failure.
    fn on_distillation_round_finished(
        &mut self,
        factory_id: usize,
        state: &mut TemporalProfilerState<Self>,
    ) {
        if state.rng.random::<f64>() < self.t_factories[factory_id].probability_success {
            // State generated successfully, can use it.
            state.event_recorder.event_aggregator.record_event(
                state.current_time_nanos,
                "T state produced",
                1,
            );
            state.add_resource(DemoModel1_Resource::TState, factory_id);
        } else {
            // State generation failed, need retry.
            self.start_distillation_round(factory_id, state);
        }
    }
}

impl QpuModel for DemoModel1 {
    type NativeOperation = DemoModel1_NativeOperation;
    type Resource = DemoModel1_Resource;

    /// Hack! We use the same "address space" for factroies and qubits, which needs to be fixed.
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
            ("t", [Parameter::QubitId(qubit)]) => {
                DemoModel1_NativeOperation::InjectT(None, *qubit, false)
            }
            ("tadj", [Parameter::QubitId(qubit)]) => {
                DemoModel1_NativeOperation::InjectT(None, *qubit, true)
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

        state.schedule_operation(
            op,
            preconditions,
            targets,
            duration_nanos,
            instruction.instruction_name == "mz",
        );
        Ok(())
    }

    /// Reserves the oldest available T-state factory for an injection.
    fn on_operation_started(
        &mut self,
        op: &mut Self::NativeOperation,
        state: &mut TemporalProfilerState<Self>,
    ) {
        if let DemoModel1_NativeOperation::InjectT(opt_factory_id, _, _) = op {
            assert!(
                opt_factory_id.is_none(),
                "injection already has an assigned factory"
            );
            let factory_id: usize = state
                .available_resources
                .get_mut(&DemoModel1_Resource::TState)
                .and_then(VecDeque::pop_front)
                .expect("instruction requires an available factory");
            *opt_factory_id = Some(factory_id);
        }
        if let DemoModel1_NativeOperation::DistillationRound(factory_id) = op {
            state.event_recorder.utilization_aggregator.record_event(
                state.current_time_nanos,
                "Factory utilization",
                self.t_factories[*factory_id].distillation_round_duration_nanos(),
            );
        }
    }

    /// Produces measurement results and advances the factory lifecycle after each operation.
    fn on_operation_finished(
        &mut self,
        op: &Self::NativeOperation,
        state: &mut TemporalProfilerState<Self>,
    ) -> Option<QuantumBackendResult> {
        match op {
            DemoModel1_NativeOperation::Measurement(_) => {
                Some(QuantumBackendResult::from(state.rng.random::<bool>()))
            }
            DemoModel1_NativeOperation::DistillationRound(factory_id) => {
                self.on_distillation_round_finished(*factory_id, state);
                None
            }
            DemoModel1_NativeOperation::InjectT(factory_id, _, _) => {
                let factory_id =
                    factory_id.expect("finished injection must have an assigned factory");
                state.event_recorder.event_aggregator.record_event(
                    state.current_time_nanos,
                    "T state consumed",
                    1,
                );
                self.start_distillation_round(factory_id, state);
                None
            }
            _ => None,
        }
    }

    /// Counts dense compute-qubit IDs, factory resources, elapsed time, and recorded events.
    fn get_total_counts(&self, state: &TemporalProfilerState<Self>) -> TotalCounts {
        let logical_compute_qubits = state
            .last_operations
            .keys()
            .max()
            .map_or(0, |qubit| qubit + 1);
        let factory_logical_qubits: usize = self
            .t_factories
            .iter()
            .map(|factory| factory.logical_qubits)
            .sum();
        let factory_physical_qubits: usize =
            self.t_factories.iter().map(TFactory::physical_qubits).sum();
        let d = self.ecc_distance;
        #[allow(clippy::cast_precision_loss)]
        let total_runtime_sec = state.current_time_nanos as f64 / 1e9;
        TotalCounts {
            logical_qubits: logical_compute_qubits + factory_logical_qubits,
            logical_compute_qubits,
            physical_qubits: logical_compute_qubits * (2 * d * d - 1) + factory_physical_qubits,
            total_runtime_sec,
            total_operations_count: state.event_recorder.event_aggregator.total_event_count(),
            calls_per_operation: Default::default(),
            runtime_sec_per_operation: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DemoModel1_NativeOperation::{
        CnotGate, DistillationRound, InjectT, Measurement, Reset, SingleQubitCliffordGate,
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
                InjectT(Some(6), 7, false),
                "inject_t",
                vec![FactoryId(6), QubitId(7)],
            ),
            (InjectT(None, 7, false), "inject_t", vec![QubitId(7)]),
            (
                InjectT(Some(8), 9, true),
                "inject_tadj",
                vec![FactoryId(8), QubitId(9)],
            ),
            (InjectT(None, 9, true), "inject_tadj", vec![QubitId(9)]),
            (DistillationRound(10), "round", vec![FactoryId(10)]),
        ] {
            assert_eq!(operation.get_name(), name);
            let logged = operation.to_logged_instruction();
            assert_eq!(logged.instruction_name, name);
            assert_eq!(logged.parameters, parameters);
        }
        for name in ["h", "s", "sadj", "x", "y", "z"] {
            let operation = SingleQubitCliffordGate(name, 11);
            assert_eq!(operation.get_name(), name);
            let logged = operation.to_logged_instruction();
            assert_eq!(logged.instruction_name, name);
            assert_eq!(logged.parameters, vec![QubitId(11)]);
        }
    }
}
