// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::interpreter::Result as QsResult;
use crate::qir_simulation::{
    NoiseConfig, QirInstruction, QirInstructionId, adaptive_program_from_pydict,
    unbind_noise_config,
};
use pyo3::{IntoPyObjectExt, exceptions::PyValueError, prelude::*, types::PyList};
use pyo3::{PyResult, pyfunction, types::PyDict};
use qdk_simulators::{
    MeasurementResult, OutputRecord, Simulator,
    bytecode::{self, runtime::run_shot as adaptive_run_shot},
    cpu_full_state_simulator::{NoiselessSimulator, NoisySimulator},
    noise_config::{self, CumulativeNoiseConfig},
    stabilizer_simulator::StabilizerSimulator,
};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::sync::Arc;

/// Map a raw [`MeasurementResult`] to its Python `Result` enum counterpart.
fn measurement_result_to_py(value: MeasurementResult) -> QsResult {
    match value {
        MeasurementResult::Zero => QsResult::Zero,
        MeasurementResult::One => QsResult::One,
        MeasurementResult::Loss => QsResult::Loss,
    }
}

/// Convert a single [`OutputRecord`] to a native Python object. Measurement
/// results become `Result` enum values; classical records become the matching
/// native type (`bool` → `bool`, `int` → `int`, `double` → `float`).
fn output_record_to_py(py: Python<'_>, record: OutputRecord) -> PyResult<Py<PyAny>> {
    match record {
        OutputRecord::Result(value) => measurement_result_to_py(value).into_py_any(py),
        OutputRecord::Bool(b) => b.into_py_any(py),
        OutputRecord::Int(i) => i.into_py_any(py),
        OutputRecord::Double(d) => d.into_py_any(py),
    }
}

/// Build the Python return value for a run: a list with one entry per shot,
/// where each entry is the ordered list of that shot's recorded output values.
fn output_records_to_pylist(py: Python<'_>, output: Vec<Vec<OutputRecord>>) -> PyResult<Py<PyAny>> {
    let mut array = Vec::with_capacity(output.len());
    for shot_records in output {
        let mut values = Vec::with_capacity(shot_records.len());
        for record in shot_records {
            values.push(output_record_to_py(py, record)?);
        }
        array.push(
            PyList::new(py, values)
                .map_err(|e| PyValueError::new_err(format!("failed to create Python list: {e}")))?
                .into_py_any(py)?,
        );
    }
    PyList::new(py, array)
        .map_err(|e| PyValueError::new_err(format!("failed to create Python list: {e}")))?
        .into_py_any(py)
}

#[pyfunction]
pub fn run_clifford<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyList>,
    num_qubits: u32,
    num_results: u32,
    shots: u32,
    noise_config: Option<&Bound<'py, NoiseConfig>>,
    seed: Option<u32>,
) -> PyResult<Py<PyAny>> {
    let make_simulator = |num_qubits, num_results, seed, noise| {
        StabilizerSimulator::new(num_qubits as usize, num_results as usize, seed, noise)
    };
    py_run(
        py,
        input,
        num_qubits,
        num_results,
        shots,
        noise_config,
        seed,
        make_simulator,
    )
}

#[pyfunction]
pub fn run_cpu_full_state<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyList>,
    num_qubits: u32,
    num_results: u32,
    shots: u32,
    noise_config: Option<&Bound<'py, NoiseConfig>>,
    seed: Option<u32>,
) -> PyResult<Py<PyAny>> {
    if noise_config.is_some() {
        let make_simulator = |num_qubits, num_results, seed, noise| {
            NoisySimulator::new(num_qubits as usize, num_results as usize, seed, noise)
        };
        py_run(
            py,
            input,
            num_qubits,
            num_results,
            shots,
            noise_config,
            seed,
            make_simulator,
        )
    } else {
        let make_simulator = |num_qubits, num_results, seed, _noise: Arc<CumulativeNoiseConfig>| {
            NoiselessSimulator::new(num_qubits as usize, num_results as usize, seed, ())
        };
        py_run(
            py,
            input,
            num_qubits,
            num_results,
            shots,
            noise_config,
            seed,
            make_simulator,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn py_run<'py, SimulatorBuilder, Noise, S>(
    py: Python<'py>,
    input: &Bound<'py, PyList>,
    num_qubits: u32,
    num_results: u32,
    shots: u32,
    noise_config: Option<&Bound<'py, NoiseConfig>>,
    seed: Option<u32>,
    make_simulator: SimulatorBuilder,
) -> PyResult<Py<PyAny>>
where
    SimulatorBuilder: Fn(u32, u32, u32, Arc<Noise>) -> S,
    SimulatorBuilder: Send + Sync,
    Noise: From<qdk_simulators::noise_config::NoiseConfig<f64, f64>> + Send + Sync,
    S: Simulator,
{
    // Convert Python list to Vec<QirInstruction>.
    let mut instructions: Vec<QirInstruction> = Vec::with_capacity(input.len());
    for item in input.iter() {
        let item: QirInstruction = item
            .extract()
            .map_err(|e| PyValueError::new_err(format!("expected QirInstruction: {e}")))?;
        instructions.push(item);
    }

    // Convert NoiseConfig to a rust NoiseConfig.
    let noise: qdk_simulators::noise_config::NoiseConfig<f64, f64> =
        if let Some(noise_config) = noise_config {
            unbind_noise_config(py, noise_config)
        } else {
            qdk_simulators::noise_config::NoiseConfig::NOISELESS
        };

    // Run the simulation.
    let output = run(
        &instructions,
        num_qubits,
        num_results,
        shots,
        seed,
        noise,
        make_simulator,
    );

    // Convert results back to Python.
    output_records_to_pylist(py, output)
}

fn run<SimulatorBuilder, Noise, S>(
    instructions: &[QirInstruction],
    num_qubits: u32,
    num_results: u32,
    shots: u32,
    seed: Option<u32>,
    noise: noise_config::NoiseConfig<f64, f64>,
    make_simulator: SimulatorBuilder,
) -> Vec<Vec<OutputRecord>>
where
    SimulatorBuilder: Fn(u32, u32, u32, Arc<Noise>) -> S,
    SimulatorBuilder: Send + Sync,
    Noise: From<noise_config::NoiseConfig<f64, f64>> + Send + Sync,
    S: Simulator,
{
    let noise: Noise = noise.into();
    let noise = Arc::new(noise);

    // Programs without any output-recording calls fall back to reporting every
    // measurement result in order, matching the historical raw-measurement
    // behavior.
    let has_output_recording = instructions
        .iter()
        .any(|inst| matches!(inst, QirInstruction::OutputRecording(..)));

    // Create a random number generator to generate the seed for each individual shot.
    let mut rng = if let Some(seed) = seed {
        StdRng::seed_from_u64(seed.into())
    } else {
        StdRng::from_rng(&mut rand::rng())
    };

    // run the shots
    (0..shots)
        .map(|_| rng.random())
        .collect::<Vec<u32>>()
        .par_iter()
        .map(|shot_seed| {
            let mut simulator = make_simulator(num_qubits, num_results, *shot_seed, noise.clone());
            let records = run_shot(instructions, &mut simulator);
            if has_output_recording {
                records
            } else {
                simulator
                    .take_measurements()
                    .into_iter()
                    .map(OutputRecord::Result)
                    .collect()
            }
        })
        .collect::<Vec<_>>()
}

fn run_shot<S: Simulator>(instructions: &[QirInstruction], sim: &mut S) -> Vec<OutputRecord> {
    let mut records: Vec<OutputRecord> = Vec::new();
    for qir_inst in instructions {
        match qir_inst {
            QirInstruction::OneQubitGate(id, qubit) => match id {
                QirInstructionId::I => {} // Identity gate is a no-op
                QirInstructionId::H => sim.h(*qubit as usize),
                QirInstructionId::X => sim.x(*qubit as usize),
                QirInstructionId::Y => sim.y(*qubit as usize),
                QirInstructionId::Z => sim.z(*qubit as usize),
                QirInstructionId::S => sim.s(*qubit as usize),
                QirInstructionId::SAdj => sim.s_adj(*qubit as usize),
                QirInstructionId::SX => sim.sx(*qubit as usize),
                QirInstructionId::SXAdj => sim.sx_adj(*qubit as usize),
                QirInstructionId::T => sim.t(*qubit as usize),
                QirInstructionId::TAdj => sim.t_adj(*qubit as usize),
                QirInstructionId::Move => sim.mov(*qubit as usize),
                QirInstructionId::RESET => sim.resetz(*qubit as usize),
                _ => panic!("unsupported one-qubit gate: {id:?}"),
            },
            QirInstruction::TwoQubitGate(id, q1, q2) => match id {
                QirInstructionId::CX => sim.cx(*q1 as usize, *q2 as usize),
                QirInstructionId::CY => sim.cy(*q1 as usize, *q2 as usize),
                QirInstructionId::CZ => sim.cz(*q1 as usize, *q2 as usize),
                QirInstructionId::MZ | QirInstructionId::M => sim.mz(*q1 as usize, *q2 as usize),
                QirInstructionId::MResetZ => sim.mresetz(*q1 as usize, *q2 as usize),
                QirInstructionId::SWAP => sim.swap(*q1 as usize, *q2 as usize),
                _ => panic!("unsupported two-qubits gate: {id:?}"),
            },
            QirInstruction::OneQubitRotationGate(id, angle, qubit) => match id {
                QirInstructionId::RX => sim.rx(*angle, *qubit as usize),
                QirInstructionId::RY => sim.ry(*angle, *qubit as usize),
                QirInstructionId::RZ => sim.rz(*angle, *qubit as usize),
                _ => {
                    panic!("unsupported one-qubit rotation gate: {id:?}");
                }
            },
            QirInstruction::TwoQubitRotationGate(id, angle, qubit1, qubit2) => match id {
                QirInstructionId::RXX => sim.rxx(*angle, *qubit1 as usize, *qubit2 as usize),
                QirInstructionId::RYY => sim.ryy(*angle, *qubit1 as usize, *qubit2 as usize),
                QirInstructionId::RZZ => sim.rzz(*angle, *qubit1 as usize, *qubit2 as usize),
                _ => panic!("unsupported two-qubit rotation gate: {id:?}"),
            },
            QirInstruction::CorrelatedNoise(_id, intrinsic_id, qubits) => {
                sim.correlated_noise_intrinsic(
                    *intrinsic_id,
                    &qubits.iter().map(|q| *q as usize).collect::<Vec<_>>(),
                );
            }
            QirInstruction::OutputRecording(id, value, _tag) => {
                // Capture result records into the unified output stream in the
                // order they are recorded. Array and tuple records are purely
                // structural and reconstructed by the host from the static QIR,
                // so nothing is captured for them here.
                if *id == QirInstructionId::ResultRecordOutput {
                    let result_id: usize = value.parse().unwrap_or(0);
                    let measurement = sim
                        .measurements()
                        .get(result_id)
                        .copied()
                        .unwrap_or(MeasurementResult::Zero);
                    records.push(OutputRecord::Result(measurement));
                }
            }
            QirInstruction::ThreeQubitGate(..) => {
                panic!("unsupported instruction: {qir_inst:?}")
            }
        }
    }
    records
}

// ---------------------------------------------------------------------------
// Adaptive Profile CPU simulation
// ---------------------------------------------------------------------------

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn run_cpu_adaptive<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyDict>,
    shots: u32,
    noise_config: Option<&Bound<'py, NoiseConfig>>,
    seed: Option<u32>,
) -> PyResult<Py<PyAny>> {
    let program: bytecode::AdaptiveProgram<u64> = adaptive_program_from_pydict(input)?;

    let noise: noise_config::NoiseConfig<f64, f64> = if let Some(nc) = noise_config {
        unbind_noise_config(py, nc)
    } else {
        noise_config::NoiseConfig::NOISELESS
    };

    let output = if noise_config.is_some() {
        let make_simulator = |num_qubits, num_results, seed, noise: Arc<CumulativeNoiseConfig>| {
            NoisySimulator::new(num_qubits, num_results, seed, noise)
        };
        run_adaptive(&program, shots, seed, noise, make_simulator)
    } else {
        let make_simulator = |num_qubits, num_results, seed, _noise: Arc<CumulativeNoiseConfig>| {
            NoiselessSimulator::new(num_qubits, num_results, seed, ())
        };
        run_adaptive(&program, shots, seed, noise, make_simulator)
    };

    output_records_to_pylist(py, output)
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn run_clifford_adaptive<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyDict>,
    shots: u32,
    noise_config: Option<&Bound<'py, NoiseConfig>>,
    seed: Option<u32>,
) -> PyResult<Py<PyAny>> {
    let program: bytecode::AdaptiveProgram<u64> = adaptive_program_from_pydict(input)?;

    let noise: noise_config::NoiseConfig<f64, f64> = if let Some(nc) = noise_config {
        unbind_noise_config(py, nc)
    } else {
        noise_config::NoiseConfig::NOISELESS
    };

    let make_simulator = |num_qubits, num_results, seed, noise: Arc<CumulativeNoiseConfig>| {
        StabilizerSimulator::new(num_qubits, num_results, seed, noise)
    };
    let output = run_adaptive(&program, shots, seed, noise, make_simulator);

    output_records_to_pylist(py, output)
}

fn run_adaptive<SimulatorBuilder, Noise, S>(
    program: &bytecode::AdaptiveProgram<u64>,
    shots: u32,
    seed: Option<u32>,
    noise: noise_config::NoiseConfig<f64, f64>,
    make_simulator: SimulatorBuilder,
) -> Vec<Vec<OutputRecord>>
where
    SimulatorBuilder: Fn(usize, usize, u32, Arc<Noise>) -> S + Send + Sync,
    Noise: From<noise_config::NoiseConfig<f64, f64>> + Send + Sync,
    S: Simulator,
{
    const OP_RECORD_OUTPUT: u8 = 0x14;

    let noise: Noise = noise.into();
    let noise = Arc::new(noise);

    let num_qubits = program.num_qubits as usize;
    let num_results = program.num_results as usize;

    // Programs without any output-recording calls fall back to reporting every
    // measurement result in order, matching the historical raw-measurement
    // behavior.
    let has_output_recording = program
        .instructions
        .iter()
        .any(|inst| inst.primary_opcode() == OP_RECORD_OUTPUT);

    let mut rng = if let Some(seed) = seed {
        StdRng::seed_from_u64(seed.into())
    } else {
        StdRng::from_rng(&mut rand::rng())
    };

    (0..shots)
        .map(|_| rng.random())
        .collect::<Vec<u32>>()
        .par_iter()
        .map(|shot_seed| {
            let mut simulator = make_simulator(num_qubits, num_results, *shot_seed, noise.clone());
            let records = adaptive_run_shot(program, &mut simulator);
            if has_output_recording {
                records
            } else {
                simulator
                    .take_measurements()
                    .into_iter()
                    .map(OutputRecord::Result)
                    .collect()
            }
        })
        .collect::<Vec<_>>()
}
