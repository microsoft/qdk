// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir_simulation::{NoiseConfig, QirInstruction, QirInstructionId, unbind_noise_config};

use pyo3::{IntoPyObjectExt, exceptions::PyValueError, prelude::*, types::PyList};
use qdk_simulators::{
    cpu_full_state_simulator::Simulator, stabilizer_simulator::MeasurementResult,
};
use rand::{Rng, SeedableRng, rngs::StdRng};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use std::fmt::Write;

#[pyfunction]
pub fn run_cpu_full_state<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyList>,
    num_qubits: u32,
    num_results: u32,
    shots: u32,
    noise_config: &Bound<'py, NoiseConfig>,
    seed: Option<u32>,
) -> PyResult<PyObject> {
    // Convert Python list input to Vec<QirInstruction>.
    let mut instructions: Vec<QirInstruction> = Vec::with_capacity(input.len());
    for item in input.iter() {
        let item = <QirInstruction as FromPyObject>::extract_bound(&item).map_err(|e| {
            PyValueError::new_err(format!("expected QirInstruction, got {item:?}: {e}"))
        })?;
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

    // Create a random number generator to generate the seed for each individual shot.
    let mut rng = if let Some(seed) = seed {
        StdRng::seed_from_u64(seed.into())
    } else {
        StdRng::from_entropy()
    };

    // Run the shots.
    let output = (0..shots)
        // Create a random seed for each shot.
        // We need to do this instead of just calling rng.gen() a couple of lines
        // below, since rust disallows calling a mutable in a parallel iterator.
        .map(|_| rng.r#gen())
        .collect::<Vec<_>>()
        .par_iter()
        .map(|seed| run_shot(&instructions, num_qubits, num_results, &noise, *seed))
        .collect::<Vec<_>>();

    // Convert results to a string with one line per shot.
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

fn run_shot(
    instructions: &Vec<QirInstruction>,
    num_qubits: u32,
    num_results: u32,
    noise: &qdk_simulators::noise_config::NoiseConfig,
    seed: u32,
) -> Vec<MeasurementResult> {
    let mut sim = Simulator::new(
        num_qubits as usize,
        num_results as usize,
        noise.clone(),
        seed,
    );
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
                QirInstructionId::T => sim.t(*qubit as usize),
                QirInstructionId::TAdj => sim.t_adj(*qubit as usize),
                QirInstructionId::Move => sim.mov(*qubit as usize),
                _ => panic!("unsupported one-qubit gate: {id:?}"),
            },
            QirInstruction::TwoQubitGate(id, control, target) => match id {
                QirInstructionId::CX => sim.cx(*control as usize, *target as usize),
                QirInstructionId::CZ => sim.cz(*control as usize, *target as usize),
                QirInstructionId::MResetZ | QirInstructionId::M | QirInstructionId::MZ => {
                    sim.mresetz(*control as usize, *target as usize);
                }
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
            QirInstruction::ThreeQubitGate(id, _, _, _) => {
                panic!("unsupported instruction: {id:?}")
            }
            QirInstruction::OutputRecording(_id, _s, _tag) => {
                // Ignore for now.
            }
        }
    }

    sim.take_measurements()
}
