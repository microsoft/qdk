// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir_simulation::{NoiseConfig, QirInstruction, QirInstructionId, unbind_noise_config};
use pyo3::{IntoPyObjectExt, exceptions::PyValueError, prelude::*, types::PyList};
use pyo3::{PyResult, pyfunction};
use qdk_simulators::{
    MeasurementResult, Simulator,
    cpu_full_state_simulator::{NoiselessSimulator, NoisySimulator},
    noise_config::{self, CumulativeNoiseConfig},
    stabilizer_simulator::StabilizerSimulator,
};
use rand::{Rng, SeedableRng, rngs::StdRng};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::{fmt::Write, sync::Arc};

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
    use qdk_simulators::cpu_full_state_simulator::noise::Fault;
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
        let make_simulator =
            |num_qubits, num_results, seed, _noise: Arc<CumulativeNoiseConfig<Fault>>| {
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
    let mut array = Vec::with_capacity(shots as usize);
    for val in output {
        array.push(
            val.into_py_any(py).map_err(|e| {
                PyValueError::new_err(format!("failed to create Python string: {e}"))
            })?,
        );
    }

    PyList::new(py, array)
        .map_err(|e| PyValueError::new_err(format!("failed to create Python list: {e}")))?
        .into_py_any(py)
}

fn run<SimulatorBuilder, Noise, S>(
    instructions: &[QirInstruction],
    num_qubits: u32,
    num_results: u32,
    shots: u32,
    seed: Option<u32>,
    mut noise: noise_config::NoiseConfig<f64, f64>,
    make_simulator: SimulatorBuilder,
) -> Vec<String>
where
    SimulatorBuilder: Fn(u32, u32, u32, Arc<Noise>) -> S,
    SimulatorBuilder: Send + Sync,
    Noise: From<noise_config::NoiseConfig<f64, f64>> + Send + Sync,
    S: Simulator,
{
    if !noise.rz.is_noiseless() {
        if noise.s.is_noiseless() {
            noise.s = noise.rz.clone();
        }
        if noise.z.is_noiseless() {
            noise.z = noise.rz.clone();
        }
        if noise.s_adj.is_noiseless() {
            noise.s_adj = noise.rz.clone();
        }
    }

    let noise: Noise = noise.into();
    let noise = Arc::new(noise);

    // Create a random number generator to generate the seed for each individual shot.
    let mut rng = if let Some(seed) = seed {
        StdRng::seed_from_u64(seed.into())
    } else {
        StdRng::from_entropy()
    };

    // run the shots
    let output = (0..shots)
        .map(|_| rng.r#gen())
        .collect::<Vec<u32>>()
        .par_iter()
        .map(|shot_seed| {
            let simulator = make_simulator(num_qubits, num_results, *shot_seed, noise.clone());
            run_shot(instructions, simulator)
        })
        .collect::<Vec<_>>();

    // Convert results to a list of strings.
    let mut values = Vec::with_capacity(shots as usize);
    for shot_result in output {
        let mut buffer = String::with_capacity(shot_result.len());
        for measurement in shot_result {
            match measurement {
                MeasurementResult::Zero => write!(&mut buffer, "0").expect("write should succeed"),
                MeasurementResult::One => write!(&mut buffer, "1").expect("write should succeed"),
                MeasurementResult::Loss => write!(&mut buffer, "L").expect("write should succeed"),
            }
        }
        values.push(buffer);
    }
    values
}

fn run_shot(instructions: &[QirInstruction], mut sim: impl Simulator) -> Vec<MeasurementResult> {
    for qir_inst in instructions {
        match qir_inst {
            QirInstruction::OneQubitGate(id, qubit) => match id {
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
            QirInstruction::OutputRecording(_id, _s, _tag) => {
                // Ignore for now.
            }
            QirInstruction::ThreeQubitGate(..) => {
                panic!("unsupported instruction: {qir_inst:?}")
            }
        }
    }

    sim.take_measurements()
}
