// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir_simulation::{QirInstruction, QirInstructionId};
use pyo3::{
    IntoPyObjectExt,
    exceptions::{PyOSError, PyRuntimeError, PyValueError},
    prelude::*,
    types::PyList,
};
use qdk_simulators::shader_types::Op;
use qsc::PauliNoise;
use rand::{RngCore, SeedableRng, rngs::StdRng};

/// Checks if a compatible GPU adapter is available on the system.
///
/// This function attempts to request a GPU adapter to determine if GPU-accelerated
/// quantum simulation is supported. It's useful for capability detection before
/// attempting to run GPU-based simulations.
///
/// # Errors
///
/// Returns `Err(String)` if:
/// - No compatible GPU is found
/// - GPU drivers are missing or not functioning properly
#[pyfunction]
pub fn try_create_gpu_adapter() -> PyResult<()> {
    qdk_simulators::try_create_gpu_adapter().map_err(PyOSError::new_err)?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
#[pyfunction]
pub fn run_gpu_full_state<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyList>,
    num_qubits: u32,
    shots: u32,
    noise: Option<(f64, f64, f64)>,
    seed: Option<u64>,
) -> PyResult<PyObject> {
    assert!(shots > 0, "must run at least one shot");

    // check for GPU availability.
    // This saves us a bunch of work if the GPU is not available.
    try_create_gpu_adapter()?;

    // convert Python list input to Vec<QirInstruction>
    let mut instructions: Vec<QirInstruction> = vec![];
    for item in input.iter() {
        let item = <QirInstruction as FromPyObject>::extract_bound(&item).map_err(|e| {
            PyValueError::new_err(format!("expected QirInstruction, got {item:?}: {e}"))
        })?;
        instructions.push(item);
    }

    // map the QirInstructions to GPU sim ops
    let ops = map_instructions(instructions);

    let noise = match noise {
        None => None,
        Some((px, py, pz)) => match PauliNoise::from_probabilities(px, py, pz) {
            Ok(noise_struct) => Some(noise_struct),
            Err(error_message) => return Err(PyValueError::new_err(error_message)),
        },
    };

    let mut array = Vec::with_capacity(shots as usize);
    let mut rng = StdRng::seed_from_u64(seed.unwrap_or_else(|| rand::thread_rng().next_u64()));

    for _ in 0..shots {
        let mut output = if let Some(noise) = noise {
            let ops = qdk_simulators::pauli_noise::apply_pauli_noise_with_loss(
                ops.clone(),
                &mut rng,
                noise.distribution,
                None,
            );
            qdk_simulators::run_gpu_simulator(num_qubits, ops)
        } else {
            qdk_simulators::run_gpu_simulator(num_qubits, ops.clone())
        }
        .map_err(PyRuntimeError::new_err)?;

        let mut prev_entry_idx = u32::MAX;
        let mut count = 0;
        for result in &output {
            if result.entry_idx < prev_entry_idx {
                count += 1;
                prev_entry_idx = result.entry_idx;
            }
        }
        output.truncate(count);

        // find the number of entries in the output before the entry_idx of the current result is less than the previous
        let output = get_probabilities(num_qubits, &output);

        // convert results to a string with one line per shot
        let mut values = vec![];

        for result in output {
            let buffer = format!("|{bits}⟩: {prob:.6}", bits = result.0, prob = result.1);
            values.push(buffer);
        }
        for val in values {
            array.push(val.into_py_any(py).map_err(|e| {
                PyValueError::new_err(format!("failed to create Python string: {e}"))
            })?);
        }
    }

    PyList::new(py, array)
        .map_err(|e| PyValueError::new_err(format!("failed to create Python list: {e}")))?
        .into_py_any(py)
}

fn get_probabilities(
    num_qubits: u32,
    raw_results: &[qdk_simulators::shader_types::Result],
) -> Vec<(String, f32)> {
    let mut formatted = Vec::with_capacity(raw_results.len());
    for res in raw_results {
        formatted.push((
            format!("{:0width$b}", res.entry_idx, width = num_qubits as usize)
                .chars()
                .rev()
                .collect::<String>(),
            res.probability,
        ));
    }
    formatted.sort_by_key(|r| r.0.clone());
    formatted
}

fn map_instructions(qir_inst: Vec<QirInstruction>) -> Vec<Op> {
    let mut ops = Vec::with_capacity(qir_inst.len());
    for inst in qir_inst {
        let op = map_instruction(&inst);
        if let Some(op) = op {
            ops.push(op);
        }
    }
    // Add measurements at the end for all qubits
    ops.push(Op::new_m_every_z_gate());
    ops
}

fn map_instruction(qir_inst: &QirInstruction) -> Option<qdk_simulators::shader_types::Op> {
    let op = match qir_inst {
        QirInstruction::OneQubitGate(id, qubit) => match id {
            QirInstructionId::I => Op::new_id_gate(*qubit),
            QirInstructionId::H => Op::new_h_gate(*qubit),
            QirInstructionId::X => Op::new_x_gate(*qubit),
            QirInstructionId::Y => Op::new_y_gate(*qubit),
            QirInstructionId::Z => Op::new_z_gate(*qubit),
            QirInstructionId::S => Op::new_s_gate(*qubit),
            QirInstructionId::SAdj => Op::new_s_adj_gate(*qubit),
            QirInstructionId::SX => Op::new_sx_gate(*qubit),
            QirInstructionId::SXAdj => Op::new_sx_adj_gate(*qubit),
            QirInstructionId::T => Op::new_t_gate(*qubit),
            QirInstructionId::TAdj => Op::new_t_adj_gate(*qubit),
            _ => {
                return None;
            }
        },
        QirInstruction::TwoQubitGate(id, _control, _target) => {
            if matches!(
                id,
                QirInstructionId::M | QirInstructionId::MZ | QirInstructionId::MResetZ
            ) {
                // measurement gates are not supported in the full state simulator
                return None;
            }
            Op::new_m_every_z_gate()
        }
        QirInstruction::OneQubitRotationGate(id, angle, qubit) => {
            #[allow(clippy::cast_possible_truncation)]
            let angle = *angle as f32;
            match id {
                QirInstructionId::RX => Op::new_rx_gate(angle, *qubit),
                QirInstructionId::RY => Op::new_ry_gate(angle, *qubit),
                QirInstructionId::RZ => Op::new_rz_gate(angle, *qubit),
                _ => {
                    return None;
                }
            }
        }
        QirInstruction::TwoQubitRotationGate(id, angle, qubit1, qubit2) => {
            #[allow(clippy::cast_possible_truncation)]
            let angle = *angle as f32;
            match id {
                QirInstructionId::RXX => Op::new_rxx_gate(angle, *qubit1, *qubit2),
                QirInstructionId::RYY => Op::new_ryy_gate(angle, *qubit1, *qubit2),
                QirInstructionId::RZZ => Op::new_rzz_gate(angle, *qubit1, *qubit2),
                _ => {
                    return None;
                }
            }
        }
        QirInstruction::ThreeQubitGate(QirInstructionId::CCX, c1, c2, target) => {
            unimplemented!("{c1}, {c2}, {target}") //Op::new_ccx_gate(*c1, *c2, *target),
        }
        _ => return None,
    };
    Some(op)
}
