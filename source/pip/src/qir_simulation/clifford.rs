// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir_simulation::{NoiseConfig, QirInstruction, QirInstructionId, unbind_noise_config};

use pyo3::{IntoPyObjectExt, exceptions::PyValueError, prelude::*, types::PyList};
use qdk_simulators::stabilizer_simulator::{MeasurementResult, Simulator};

use std::fmt::Write;

#[pyfunction]
pub fn run_clifford<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyList>,
    num_qubits: u32,
    num_results: u32,
    shots: u32,
    noise_config: &Bound<'py, NoiseConfig>,
    _seed: Option<u32>,
) -> PyResult<Py<PyAny>> {
    use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
    assert!(shots > 0, "must run at least one shot");

    // convert Python list input to Vec<QirInstruction>
    let mut instructions: Vec<QirInstruction> = Vec::with_capacity(input.len());
    for item in input.iter() {
        let item: QirInstruction = item
            .extract()
            .map_err(|e| PyValueError::new_err(format!("expected QirInstruction: {e}")))?;
        instructions.push(item);
    }

    let mut noise = unbind_noise_config(py, noise_config);

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

    // run the shots
    let output = (0..shots)
        .collect::<Vec<_>>()
        .par_iter()
        .map(|_| run_clifford_shot(&instructions, num_qubits, num_results, &noise))
        .collect::<Vec<_>>();

    // convert results to a string with one line per shot
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

    let mut array = Vec::with_capacity(shots as usize);
    for val in values {
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

fn run_clifford_shot(
    instructions: &Vec<QirInstruction>,
    num_qubits: u32,
    num_results: u32,
    noise: &qdk_simulators::noise_config::NoiseConfig,
) -> Vec<MeasurementResult> {
    let mut sim = Simulator::new(num_qubits as usize, num_results as usize, noise.clone());
    for op in instructions {
        match op {
            QirInstruction::OneQubitGate(id, qubit) => match id {
                QirInstructionId::H => sim.h(*qubit as usize),
                QirInstructionId::X => sim.x(*qubit as usize),
                QirInstructionId::Y => sim.y(*qubit as usize),
                QirInstructionId::Z => sim.z(*qubit as usize),
                QirInstructionId::S => sim.s(*qubit as usize),
                QirInstructionId::SAdj => sim.s_adj(*qubit as usize),
                QirInstructionId::SX => sim.sx(*qubit as usize),
                QirInstructionId::SXAdj => sim.sx_adj(*qubit as usize),
                QirInstructionId::Move => sim.mov(*qubit as usize),
                _ => panic!(
                    "only one qubit gates H, X, Y, Z, S, SAdj, SX, and Move are supported in Clifford simulator"
                ),
            },
            QirInstruction::TwoQubitGate(id, control, target) => match id {
                QirInstructionId::CX => sim.cx(*control as usize, *target as usize),
                QirInstructionId::CZ => sim.cz(*control as usize, *target as usize),
                QirInstructionId::MResetZ | QirInstructionId::M | QirInstructionId::MZ => {
                    sim.mresetz(*control as usize, *target as usize);
                }
                _ => panic!(
                    "only CZ, M, MZ, and MResetZ are supported in Clifford simulator, got {id:?}"
                ),
            },
            QirInstruction::OneQubitRotationGate(id, _, _)
            | QirInstruction::TwoQubitRotationGate(id, _, _, _)
            | QirInstruction::ThreeQubitGate(id, _, _, _) => {
                panic!("unsupported gate in Clifford simulator, got {id:?}")
            }
            QirInstruction::OutputRecording(_id, _s, _tag) => {
                // todo: handle output recording
                //println!("output recording: {id:?}, {s}, {tag}");
            }
            QirInstruction::CorrelatedNoise(_, _table_id, _qubit_args) => {
                unimplemented!(
                    "Correlated noise tables are not supported in the CPU full state simulator yet"
                );
            }
        }
    }

    sim.take_measurements()
}
