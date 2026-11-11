use super::{
    QpuModel,
    factories::TFactory,
    qubits::LogicalQubit,
    surface_code::{CODE_CYCLE_NANOS, logical_step_nanos},
};
use crate::{
    Parameter, QuantumBackendResult, TotalCounts,
    interface::QuantumBackendInstruction,
    profilers::{
        LoggableOperation, LoggedInstruction, LoggedInstructionParameter, Precondition,
        TemporalProfilerState,
    },
};
use rand::RngExt;
use std::collections::VecDeque;

pub enum DemoModel1NativeOperation {
    SingleQubitCliffordGate(&'static str, usize), // Gate name, QubitId.
    CnotGate(usize, usize),                       // QubitId, QubitId.
    Measurement(usize),                           // QubitId.
    Reset(usize),                                 // QubitId.
    InjectT(Option<usize>, usize, bool),          // FactoryId, QubitId, is_adjoint.
    DistillationRound(usize),                     // FactoryId.
}

impl TryFrom<&QuantumBackendInstruction> for DemoModel1NativeOperation {
    type Error = String;

    /// Validates the backend instruction and maps it to a native operation.
    fn try_from(instruction: &QuantumBackendInstruction) -> Result<Self, Self::Error> {
        match (
            instruction.instruction_name,
            instruction.parameters.as_slice(),
        ) {
            ("cx", [Parameter::QubitId(control), Parameter::QubitId(target)]) => {
                Ok(Self::CnotGate(*control, *target))
            }
            (name @ ("h" | "s" | "sadj" | "x" | "y" | "z"), [Parameter::QubitId(qubit)]) => {
                Ok(Self::SingleQubitCliffordGate(name, *qubit))
            }
            ("mz", [Parameter::QubitId(qubit)]) => Ok(Self::Measurement(*qubit)),
            ("reset", [Parameter::QubitId(qubit)]) => Ok(Self::Reset(*qubit)),
            ("t", [Parameter::QubitId(qubit)]) => Ok(Self::InjectT(None, *qubit, false)),
            ("tadj", [Parameter::QubitId(qubit)]) => Ok(Self::InjectT(None, *qubit, true)),
            _ => Err(format!("Unsupported instruction: {instruction:?}")),
        }
    }
}

impl LoggableOperation for DemoModel1NativeOperation {
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
pub enum DemoModel1Resource {
    TState,
}

/// Simplisitic model for demos:
///  * Functional units:
///    * Logical Compute Qubits, number is not fixed in advance.
///    * T-factroies, number fixed in advance.
///      * Each T-factory uses five logical patches.
///      * T-factory produces `TStates`.
///    * Both standalone LCQs and LCQs inside factories use Surface code with distance `ecc_distance`.
///  * Resources:
///    * T-states.
///  * Supported operations:
///    * Supports Clifford gates, each takes `clifford_gate_time_nanos`.
///    * Supports measurments, each takes `measurment_time_nanos`.
///      * Each measurments returns |0> or |1> with probability 50%.
///    * Supports T gates which is implemented by `InjectT` operation which consumes `TState`.
///    * Supports `AdjointT` gates which is implemented by `InjectAdjointT` operation which consumes `TState`.
///  * Error model:
///    * Each qubit has error 0 at start and after reset.
///    * Qubit accrues proportional to time.
///    * When `TState` is injected, add output T state error to qubit error.
///    * On measurement, previously unreported qubit error is added to the program budget.
///    * This is a linear fault budget, not an exact probability or a noisy simulation.
pub struct DemoModel1 {
    /// Duration of a Clifford instruction in nanoseconds.
    pub clifford_gate_time_nanos: usize,
    /// Duration of measurement, reset, or their combination in nanoseconds.
    pub measurment_time_nanos: usize,
    /// Distance of the error-coorecting code.
    /// Assumes that all logical qubits have the same error correcting code.
    /// TODO: change it to specify different ECCs for different qubits.
    pub ecc_distance: usize,

    /// Functional units.
    qubits: Vec<LogicalQubit>,
    pub(crate) t_factories: Vec<TFactory>,
}

impl Default for DemoModel1 {
    /// Uses distance nine, ten factories, and a conservative three-step Clifford latency.
    fn default() -> Self {
        let distance = 9;
        Self::new(
            3 * logical_step_nanos(distance),
            CODE_CYCLE_NANOS,
            10,
            distance,
        )
    }
}

impl DemoModel1 {
    /// Creates the configured factories and sets the model's operation durations.
    #[must_use]
    pub fn new(
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
            qubits: vec![],
        }
    }

    /// Time to perform "`InjectT`" or "`InjectT` Adjoint" gate.
    fn inject_t_time(&self) -> usize {
        2 * self.clifford_gate_time_nanos + self.measurment_time_nanos
    }

    /// Schedules one distillation attempt on an unoccupied factory.
    fn start_distillation_round(
        &mut self,
        factory_id: usize,
        state: &mut TemporalProfilerState<Self>,
    ) {
        let op = DemoModel1NativeOperation::DistillationRound(factory_id);
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
            self.t_factories[factory_id].output_ready_nanos = Some(state.current_time_nanos);
            // State generated successfully, can use it.
            state.event_recorder.event_aggregator.record_event(
                state.current_time_nanos,
                "T state produced",
                1,
            );
            state.add_resource(DemoModel1Resource::TState, factory_id);
        } else {
            // State generation failed, need retry.
            self.start_distillation_round(factory_id, state);
        }
    }
}

impl QpuModel for DemoModel1 {
    type NativeOperation = DemoModel1NativeOperation;
    type Resource = DemoModel1Resource;

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
        let op = DemoModel1NativeOperation::try_from(instruction)?;

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
                DemoModel1Resource::TState,
            )],
            _ => vec![],
        };

        let targets: Vec<usize> = instruction
            .parameters
            .iter()
            .filter_map(|parameter| match parameter {
                Parameter::QubitId(qubit) => Some(*qubit),
                _ => None,
            })
            .collect();

        // Grow `qubits` as needed.
        let max_qubit_id: usize = *targets.iter().max().unwrap_or(&0);
        while self.qubits.len() <= max_qubit_id {
            self.qubits.push(LogicalQubit::new(
                self.ecc_distance,
                state.current_time_nanos,
            ));
        }

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
        if let DemoModel1NativeOperation::InjectT(opt_factory_id, _, _) = op {
            assert!(
                opt_factory_id.is_none(),
                "injection already has an assigned factory"
            );
            let factory_id: usize = state
                .available_resources
                .get_mut(&DemoModel1Resource::TState)
                .and_then(VecDeque::pop_front)
                .expect("instruction requires an available factory");
            *opt_factory_id = Some(factory_id);
        }
        if let DemoModel1NativeOperation::DistillationRound(factory_id) = op {
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
            DemoModel1NativeOperation::Measurement(qubit_id) => {
                state.error += self.qubits[*qubit_id].take_error(state.current_time_nanos);
                Some(QuantumBackendResult::from(state.rng.random::<bool>()))
            }
            DemoModel1NativeOperation::Reset(qubit_id) => {
                self.qubits[*qubit_id].reset(state.current_time_nanos);
                None
            }
            DemoModel1NativeOperation::DistillationRound(factory_id) => {
                self.on_distillation_round_finished(*factory_id, state);
                None
            }
            DemoModel1NativeOperation::InjectT(factory_id, qubit_id, _) => {
                let factory_id =
                    factory_id.expect("finished injection must have an assigned factory");

                // Add produced T-state error to qubit error.
                self.qubits[*qubit_id].accumulated_error +=
                    self.t_factories[factory_id].take_output_error(state.current_time_nanos);

                state.event_recorder.event_aggregator.record_event(
                    state.current_time_nanos,
                    "T state consumed",
                    1,
                );

                // Can start next distillation round using this factory immediately.
                self.start_distillation_round(factory_id, state);
                None
            }
            _ => None,
        }
    }

    /// Counts dense compute-qubit IDs, factory resources, elapsed time, and recorded events.
    fn get_total_counts(&self, state: &TemporalProfilerState<Self>) -> TotalCounts {
        let logical_compute_qubits = self.qubits.len();
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
            error: state.error,
        }
    }
}
