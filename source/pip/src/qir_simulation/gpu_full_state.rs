// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir_simulation::{NoiseConfig, QirInstruction, QirInstructionId, unbind_noise_config};
use pyo3::{
    IntoPyObjectExt,
    exceptions::{PyOSError, PyRuntimeError, PyValueError},
    prelude::*,
    types::PyList,
};
use qdk_simulators::shader_types::{self, Op};

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

#[pyfunction]
pub fn run_parallel_shots<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyList>,
    shots: u32,
    qubit_count: u32,
    result_count: u32,
    noise_config: Option<&Bound<'py, NoiseConfig>>,
    seed: Option<u32>,
) -> PyResult<PyObject> {
    try_create_gpu_adapter()?;

    // Get the list of QirInstructions from the Python input list
    let mut instructions: Vec<QirInstruction> = vec![];
    for item in input.iter() {
        let item = <QirInstruction as FromPyObject>::extract_bound(&item).map_err(|e| {
            PyValueError::new_err(format!("expected QirInstruction, got {item:?}: {e}"))
        })?;
        instructions.push(item);
    }

    let mut ops = Vec::with_capacity(instructions.len() + 1);

    let rng_seed = seed.unwrap_or(0xfeed_face);

    let noise = noise_config.map(|noise_config| unbind_noise_config(py, noise_config));

    for inst in instructions {
        let op = map_instruction(&inst, true);
        if let Some(op) = op {
            let mut add_ops: Vec<Op> = vec![op];
            // If there's a NoiseConfig, and we get noise for this op, append it
            if let Some(noise) = noise
                && let Some(noise_ops) = get_noise_ops(&op, &noise)
            {
                add_ops.extend(noise_ops);
            }
            // If it's an MResetZ with noise, change to an Id with noise, followed by MResetZ
            // (This is just simpler to implement than doing noise inline with MResetZ for now)
            if op.id == shader_types::ops::MRESETZ && add_ops.len() > 1 {
                let mz_copy = add_ops[0];
                add_ops[0] = Op::new_id_gate(op.q1);
                add_ops.push(mz_copy);
            }
            // Convert 'mov' ops to identity, and don't add the ops if it's just a
            // single identity (but do add if it has noise)
            if add_ops[0].id == shader_types::ops::MOVE {
                add_ops[0].id = shader_types::ops::ID;
            }
            if add_ops.len() == 1 && add_ops[0].id == shader_types::ops::ID {
                // skip lone identity gates
            } else {
                ops.extend(add_ops);
            }
        }
    }

    // Extract the number of qubits and results needed, and a mapping of result index to output
    // array index. (Only program return type of Result[] is supported for now)

    // Run the final op sequence on the GPU for the specified number of shots
    let sim_results =
        qdk_simulators::run_parallel_shots(qubit_count, result_count, ops, shots, rng_seed)
            .map_err(PyRuntimeError::new_err)?;

    // Collect and format the results into a Python list of strings

    // Turn each shot's results into a string, with '0' for 0, '1' for 1, and 'L' for lost qubits
    // The results are a flat list of u32, with each shot's results in sequence + one error code,
    // so we need to chunk them up accordingly
    let str_results = sim_results
        .chunks((result_count + 1) as usize)
        .map(|chunk| &chunk[..result_count as usize])
        .map(|shot_results| {
            let mut bitstring = String::with_capacity(result_count as usize);
            for idx in 0..result_count {
                let res = shot_results[idx as usize];
                let char = match res {
                    0 => '0',
                    1 => '1',
                    _ => 'L', // lost qubit
                };
                bitstring.push(char);
            }
            bitstring
        })
        .collect::<Vec<String>>();

    PyList::new(py, str_results)
        .map_err(|e| PyValueError::new_err(format!("failed to create Python list: {e}")))?
        .into_py_any(py)
}

fn get_noise_ops(
    op: &Op,
    noise_config: &qdk_simulators::noise_config::NoiseConfig,
) -> Option<Vec<Op>> {
    let noise_table = match op.id {
        shader_types::ops::ID => &noise_config.i,
        shader_types::ops::X => &noise_config.x,
        shader_types::ops::Y => &noise_config.y,
        shader_types::ops::Z => &noise_config.z,
        shader_types::ops::H => &noise_config.h,
        shader_types::ops::S => &noise_config.s,
        shader_types::ops::S_ADJ => &noise_config.s_adj,
        shader_types::ops::T => &noise_config.t,
        shader_types::ops::T_ADJ => &noise_config.t_adj,
        shader_types::ops::SX => &noise_config.sx,
        shader_types::ops::SX_ADJ => &noise_config.sx_adj,
        shader_types::ops::RX => &noise_config.rx,
        shader_types::ops::RY => &noise_config.ry,
        shader_types::ops::RZ => &noise_config.rz,
        shader_types::ops::CX => &noise_config.cx,
        shader_types::ops::CZ => &noise_config.cz,
        shader_types::ops::RXX => &noise_config.rxx,
        shader_types::ops::RYY => &noise_config.ryy,
        shader_types::ops::RZZ => &noise_config.rzz,
        shader_types::ops::SWAP => &noise_config.swap,
        shader_types::ops::MOVE => &noise_config.mov,
        shader_types::ops::MRESETZ => &noise_config.mresetz,
        _ => return None,
    };
    if noise_table.is_noiseless() {
        return None;
    }
    let mut results = vec![];
    if noise_table.has_pauli_noise() {
        if shader_types::ops::is_1q_op(op.id) {
            results.push(Op::new_pauli_noise_1q(
                op.q1,
                noise_table.x,
                noise_table.y,
                noise_table.z,
            ));
        } else if shader_types::ops::is_2q_op(op.id) {
            results.push(Op::new_pauli_noise_2q(
                op.q1,
                op.q2,
                noise_table.x,
                noise_table.y,
                noise_table.z,
            ));
        } else {
            panic!("unsupported op for pauli noise: {op:?}");
        }
    }
    if noise_table.loss > 0.0 {
        if shader_types::ops::is_2q_op(op.id) {
            // For two-qubit gates, doing loss inline is hard, so just append an Id gate with loss for each qubit
            results.push(Op::new_id_gate(op.q1));
            results.push(Op::new_loss_noise(op.q1, noise_table.loss));
            results.push(Op::new_id_gate(op.q2));
            results.push(Op::new_loss_noise(op.q2, noise_table.loss));
        } else if shader_types::ops::is_1q_op(op.id) {
            // For one-qubit gates, just add the loss noise on the one qubit operation
            results.push(Op::new_loss_noise(op.q1, noise_table.loss));
        } else {
            panic!("unsupported op for loss noise: {op:?}");
        }
    }
    Some(results)
}

fn map_instruction(
    qir_inst: &QirInstruction,
    supports_mz: bool,
) -> Option<qdk_simulators::shader_types::Op> {
    let op = match qir_inst {
        QirInstruction::OneQubitGate(id, qubit) => match id {
            QirInstructionId::I => Op::new_id_gate(*qubit),
            QirInstructionId::Move => Op::new_move_gate(*qubit),
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
                panic!("unsupported one-qubit gate: {id:?} on qubit {qubit}");
            }
        },
        QirInstruction::TwoQubitGate(id, control, target) => match id {
            QirInstructionId::M | QirInstructionId::MZ | QirInstructionId::MResetZ => {
                if supports_mz {
                    Op::new_mresetz_gate(*control, *target)
                } else {
                    return None;
                }
            }
            QirInstructionId::CX => Op::new_cx_gate(*control, *target),
            QirInstructionId::CZ => Op::new_cz_gate(*control, *target),
            _ => {
                panic!("unsupported two-qubit gate: {id:?} on qubits {control}, {target}");
            }
        },
        QirInstruction::OneQubitRotationGate(id, angle, qubit) => {
            #[allow(clippy::cast_possible_truncation)]
            let angle = *angle as f32;
            match id {
                QirInstructionId::RX => Op::new_rx_gate(angle, *qubit),
                QirInstructionId::RY => Op::new_ry_gate(angle, *qubit),
                QirInstructionId::RZ => Op::new_rz_gate(angle, *qubit),
                _ => {
                    panic!("unsupported one-qubit rotation gate: {id:?} on qubit {qubit}");
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
                    panic!(
                        "unsupported two-qubit rotation gate: {id:?} on qubits {qubit1}, {qubit2}"
                    );
                }
            }
        }
        QirInstruction::ThreeQubitGate(QirInstructionId::CCX, c1, c2, target) => {
            unimplemented!("{c1}, {c2}, {target}") //Op::new_ccx_gate(*c1, *c2, *target),
        }
        QirInstruction::OutputRecording(_, _, _) => {
            // Ignore for now
            return None;
        }
        _ => panic!("unsupported instruction: {qir_inst:?}"),
    };
    Some(op)
}
