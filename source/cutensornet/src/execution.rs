use crate::{
    AvailabilityError, discover,
    simulation::{
        Circuit, CuTensorNetMpsConsumerError, CuTensorNetSampleMatrix, Gate, SamplingRequest,
        SimulationError, UnitaryOperationConversionError,
    },
};
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use crate::{
    library::Session,
    simulation::{ExecutionPolicy, collect_sampled_shots},
};
use qdk_simulators::{
    MeasurementResult, OutputRecord, QubitID,
    execution::{
        MeasurementRequest, PreparedAdaptiveProgram, QuantumEvolutionRegion, RegionConsumer,
        drive_prepared_shot,
    },
};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use std::fmt;
use thiserror::Error;

const SAMPLER_HYPER_SAMPLES: i32 = 8;

#[derive(Debug, Error)]
#[error("{message}")]
pub struct MpsExecutionError {
    message: String,
    environment: bool,
}

impl MpsExecutionError {
    #[must_use]
    pub const fn is_environment_error(&self) -> bool {
        self.environment
    }

    fn program(error: impl fmt::Display) -> Self {
        Self {
            message: error.to_string(),
            environment: false,
        }
    }

    fn environment(error: impl fmt::Display) -> Self {
        Self {
            message: error.to_string(),
            environment: true,
        }
    }
}

#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(
        dead_code,
        reason = "host-independent preflight prepares native inputs before reporting an unsupported target"
    )
)]
#[derive(Debug)]
struct PreparedMpsRun {
    circuit: Circuit,
    sampled_qubits: Box<[QubitID]>,
    shot_count: usize,
    sampling_request: SamplingRequest,
}

#[derive(Debug, Error)]
enum CircuitPreparationError {
    #[error(transparent)]
    Consumer(#[from] CuTensorNetMpsConsumerError),

    #[error(transparent)]
    Conversion(#[from] UnitaryOperationConversionError),

    #[error(transparent)]
    Circuit(#[from] SimulationError),
}

struct CircuitPreparationConsumer {
    circuit: Circuit,
    last_measured_qubit: Option<QubitID>,
}

impl CircuitPreparationConsumer {
    fn new(qubit_count: u32) -> Result<Self, CircuitPreparationError> {
        Ok(Self {
            circuit: Circuit::new(qubit_count)?,
            last_measured_qubit: None,
        })
    }
}

impl RegionConsumer for CircuitPreparationConsumer {
    type PreparedRegion<'region> = &'region QuantumEvolutionRegion;
    type RegionReport = ();
    type ExecutionReport = ();
    type Error = CircuitPreparationError;

    fn prepare_region<'region>(
        &mut self,
        region: &'region QuantumEvolutionRegion,
    ) -> Result<Self::PreparedRegion<'region>, Self::Error> {
        for &operation in region.operations() {
            if let Some(gate) = Gate::from_unitary_operation(operation)? {
                self.circuit.push(gate)?;
            }
        }
        Ok(region)
    }

    fn execute_region(
        &mut self,
        _region: Self::PreparedRegion<'_>,
    ) -> Result<Self::RegionReport, Self::Error> {
        if let Some(qubit) = self.last_measured_qubit {
            return Err(CuTensorNetMpsConsumerError::UnsupportedFeedforward { qubit }.into());
        }
        Ok(())
    }

    fn measure(&mut self, request: MeasurementRequest) -> Result<MeasurementResult, Self::Error> {
        self.last_measured_qubit = Some(request.qubit);
        Ok(MeasurementResult::Zero)
    }

    fn finish_execution(&mut self) -> Result<Self::ExecutionReport, Self::Error> {
        Ok(())
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Executes a prepared Base-profile program through one cuTensorNet MPS session.
///
/// This is an internal cross-crate entrypoint for the Python native module.
#[doc(hidden)]
pub fn run_mps_shots(
    prepared_program: &PreparedAdaptiveProgram<u64>,
    shots: u32,
    seed: Option<u32>,
) -> Result<Vec<Vec<OutputRecord>>, MpsExecutionError> {
    let prepared_run = prepare_mps_run(prepared_program, shots, seed)?;
    execute_mps_run(prepared_program, &prepared_run)
}

fn prepare_mps_run(
    prepared_program: &PreparedAdaptiveProgram<u64>,
    shots: u32,
    seed: Option<u32>,
) -> Result<PreparedMpsRun, MpsExecutionError> {
    let region_count = prepared_program.regions().len();
    if region_count > 1 {
        return Err(MpsExecutionError::program(
            CuTensorNetMpsConsumerError::UnsupportedRegionCount {
                actual: region_count,
            },
        ));
    }

    let mut consumer = CircuitPreparationConsumer::new(prepared_program.program().num_qubits)
        .map_err(MpsExecutionError::program)?;
    drive_prepared_shot(prepared_program, &mut consumer).map_err(MpsExecutionError::program)?;
    if region_count != 1 {
        return Err(MpsExecutionError::program(
            CuTensorNetMpsConsumerError::UnsupportedRegionCount {
                actual: region_count,
            },
        ));
    }

    let measured_qubits = prepared_program.measured_qubits().map_err(|error| {
        MpsExecutionError::program(CuTensorNetMpsConsumerError::InvalidMeasurementMetadata {
            error,
        })
    })?;
    let sampled_qubits = CuTensorNetSampleMatrix::sampled_qubits(measured_qubits);
    let shot_count =
        usize::try_from(shots).map_err(|error| MpsExecutionError::program(error.to_string()))?;
    let derived_seed = derive_sampler_seed(seed);
    let sampling_request = SamplingRequest::new(
        shot_count,
        SAMPLER_HYPER_SAMPLES,
        Some(derived_seed),
        derived_seed,
    )
    .map_err(MpsExecutionError::program)?;

    Ok(PreparedMpsRun {
        circuit: consumer.circuit,
        sampled_qubits,
        shot_count,
        sampling_request,
    })
}

fn derive_sampler_seed(seed: Option<u32>) -> i32 {
    let mut rng = if let Some(seed) = seed {
        StdRng::seed_from_u64(seed.into())
    } else {
        StdRng::from_rng(&mut rand::rng())
    };
    rng.random_range(1..=i32::MAX)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn execute_mps_run(
    prepared_program: &PreparedAdaptiveProgram<u64>,
    prepared_run: &PreparedMpsRun,
) -> Result<Vec<Vec<OutputRecord>>, MpsExecutionError> {
    let availability = discover().map_err(MpsExecutionError::environment)?;
    let mut session = Session::new(
        availability.libraries,
        ExecutionPolicy::base_qualification(),
    )
    .map_err(MpsExecutionError::environment)?;
    let execution = session
        .sample(
            &prepared_run.circuit,
            &prepared_run.sampled_qubits,
            prepared_run.sampling_request,
        )
        .map_err(MpsExecutionError::environment)
        .and_then(|samples| {
            collect_sampled_shots(prepared_program, prepared_run.shot_count, samples)
                .map_err(MpsExecutionError::program)
        });
    combine_execution_and_session_cleanup(execution, session.close())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn execute_mps_run(
    _prepared_program: &PreparedAdaptiveProgram<u64>,
    prepared_run: &PreparedMpsRun,
) -> Result<Vec<Vec<OutputRecord>>, MpsExecutionError> {
    let _ = prepared_run;
    let error = discover().expect_err("cuTensorNet discovery is unsupported on this target");
    Err(MpsExecutionError::environment(error))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn combine_execution_and_session_cleanup<T>(
    execution: Result<T, MpsExecutionError>,
    cleanup: Result<(), SimulationError>,
) -> Result<T, MpsExecutionError> {
    match (execution, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup)) => Err(MpsExecutionError::environment(cleanup)),
        (Err(execution), Err(cleanup)) => Err(MpsExecutionError::environment(format_args!(
            "execution failed ({execution}); cleanup also failed ({cleanup})"
        ))),
    }
}

impl From<AvailabilityError> for MpsExecutionError {
    fn from(error: AvailabilityError) -> Self {
        Self::environment(error)
    }
}

#[cfg(test)]
mod tests {
    use super::prepare_mps_run;
    use qdk_simulators::{
        bytecode::{AdaptiveProgram, Block, Instruction, Op},
        execution::PreparedAdaptiveProgram,
    };

    const IMMEDIATE_AUX1: u64 = 1 << 20;
    const IMMEDIATE_AUX2: u64 = 1 << 21;

    fn operation(operation_id: u64) -> Op<u64> {
        Op {
            op_id: operation_id,
            q1: 0,
            q2: 0,
            q3: 0,
            angle: 0,
        }
    }

    fn prepared_program(
        instructions: Vec<Instruction<u64>>,
        quantum_ops: Vec<Op<u64>>,
    ) -> PreparedAdaptiveProgram<u64> {
        PreparedAdaptiveProgram::new(AdaptiveProgram {
            num_qubits: 2,
            num_results: 1,
            num_registers: 0,
            entry_block: 0,
            block_table: vec![Block {
                instr_offset: 0,
                instr_count: instructions.len() as u64,
            }],
            instructions,
            function_table: Vec::new(),
            phi_entries: Vec::new(),
            switch_cases: Vec::new(),
            call_args: Vec::new(),
            constant_data: Vec::new(),
            quantum_ops,
        })
        .expect("test program should prepare")
    }

    fn gate(target: u64) -> Instruction<u64> {
        Instruction {
            opcode: 0x10 | (1 << 16) | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
            aux1: target,
            ..Instruction::default()
        }
    }

    fn measure(qubit: u64) -> Instruction<u64> {
        Instruction {
            opcode: 0x11 | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
            aux0: 1,
            aux1: qubit,
            aux2: 0,
            ..Instruction::default()
        }
    }

    fn ret() -> Instruction<u64> {
        Instruction {
            opcode: 0x02,
            ..Instruction::default()
        }
    }

    #[test]
    fn preflight_rejects_multiple_regions_before_device_discovery() {
        let program = prepared_program(
            vec![gate(0), measure(0), gate(1), ret()],
            vec![operation(5), operation(21)],
        );

        let error = prepare_mps_run(&program, 1, Some(42))
            .expect_err("multiple regions should be rejected");

        assert_eq!(
            error.to_string(),
            "cuTensorNet batch sampling requires exactly one quantum evolution region, found 2"
        );
        assert!(!error.is_environment_error());
    }

    #[test]
    fn preflight_rejects_feedforward_before_device_discovery() {
        let program = prepared_program(
            vec![measure(0), gate(1), ret()],
            vec![operation(5), operation(21)],
        );

        let error =
            prepare_mps_run(&program, 1, Some(42)).expect_err("feedforward should be rejected");

        assert!(
            error.to_string().contains(
                "cuTensorNet batch sampling cannot execute a quantum evolution region after measuring qubit 0"
            )
        );
        assert!(!error.is_environment_error());
    }

    #[test]
    fn preflight_rejects_unsupported_unitary_before_device_discovery() {
        let program = prepared_program(vec![gate(0), ret()], vec![operation(3)]);

        let error = prepare_mps_run(&program, 1, Some(42))
            .expect_err("unsupported unitary should be rejected");

        assert_eq!(
            error.to_string(),
            "consumer execution failed: unitary operation Y is not supported by cuTensorNet"
        );
        assert!(!error.is_environment_error());
    }
}
