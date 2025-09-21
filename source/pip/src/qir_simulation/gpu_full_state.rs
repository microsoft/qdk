// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir_simulation::{QirInstruction, QirInstructionId};
use pyo3::{IntoPyObjectExt, exceptions::PyValueError, prelude::*, types::PyList};
use qdk_simulators::shader_types::Op;

#[allow(clippy::too_many_lines)]
#[pyfunction]
pub fn run_gpu_full_state<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyList>,
    num_qubits: u32,
    shots: u32,
) -> PyResult<PyObject> {
    assert!(shots > 0, "must run at least one shot");

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

    let mut output = qdk_simulators::run_gpu_simulator(num_qubits, ops);

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
    let mut values = Vec::with_capacity(shots as usize);

    for result in output {
        let buffer = format!("|{bits}⟩: {prob:.6}", bits = result.0, prob = result.1);
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
        QirInstruction::OneQubitRotationGate(id, angle, qubit) => match id {
            QirInstructionId::RX => Op::new_rx_gate(*angle as f32, *qubit),
            QirInstructionId::RY => Op::new_ry_gate(*angle as f32, *qubit),
            QirInstructionId::RZ => Op::new_rz_gate(*angle as f32, *qubit),
            _ => {
                return None;
            }
        },
        QirInstruction::TwoQubitRotationGate(id, angle, control, target) => {
            #[allow(clippy::cast_possible_truncation)]
            let angle = *angle as f32;
            qdk_simulators::shader_types::Op {
                id: map_ids(*id),
                q1: *control,
                q2: *target,
                q3: 0,
                angle,
                _00r: 0.0,
                _00i: 0.0,
                _01r: 0.0,
                _01i: 0.0,
                _10r: 0.0,
                _10i: 0.0,
                _11r: 0.0,
                _11i: 0.0,
                padding: [0; 204],
            }
        }
        QirInstruction::ThreeQubitGate(id, control, c2, target) => {
            qdk_simulators::shader_types::Op {
                id: map_ids(*id),
                q1: *control,
                q2: *c2,
                q3: *target,
                angle: 0.0,
                _00r: 0.0,
                _00i: 0.0,
                _01r: 0.0,
                _01i: 0.0,
                _10r: 0.0,
                _10i: 0.0,
                _11r: 0.0,
                _11i: 0.0,
                padding: [0; 204],
            }
        }
        _ => {
            return None;
        }
    };
    Some(op)
}

fn map_ids(qir_id: QirInstructionId) -> u32 {
    match qir_id {
        QirInstructionId::H => qdk_simulators::shader_types::ops::H,
        QirInstructionId::X => qdk_simulators::shader_types::ops::X,
        QirInstructionId::Y => qdk_simulators::shader_types::ops::Y,
        QirInstructionId::Z => qdk_simulators::shader_types::ops::Z,
        QirInstructionId::S => qdk_simulators::shader_types::ops::S,
        QirInstructionId::SAdj => qdk_simulators::shader_types::ops::S_ADJ,
        QirInstructionId::SX => qdk_simulators::shader_types::ops::SX,
        QirInstructionId::SXAdj => qdk_simulators::shader_types::ops::SX_ADJ,
        QirInstructionId::T => qdk_simulators::shader_types::ops::T,
        QirInstructionId::TAdj => qdk_simulators::shader_types::ops::T_ADJ,
        QirInstructionId::CZ => qdk_simulators::shader_types::ops::CZ,
        QirInstructionId::CNOT | QirInstructionId::CX => qdk_simulators::shader_types::ops::CX,
        QirInstructionId::CCX => qdk_simulators::shader_types::ops::CCX,
        QirInstructionId::SWAP => qdk_simulators::shader_types::ops::SWAP,
        QirInstructionId::RX => qdk_simulators::shader_types::ops::RX,
        QirInstructionId::RY => qdk_simulators::shader_types::ops::RY,
        QirInstructionId::RZ => qdk_simulators::shader_types::ops::RZ,
        QirInstructionId::RXX => qdk_simulators::shader_types::ops::RXX,
        QirInstructionId::RYY => qdk_simulators::shader_types::ops::RYY,
        QirInstructionId::RZZ => qdk_simulators::shader_types::ops::RZZ,
        QirInstructionId::M | QirInstructionId::MZ | QirInstructionId::MResetZ => {
            qdk_simulators::shader_types::ops::MEVERYZ
        }
        _ => panic!("unsupported gate in full state simulator, got {qir_id:?}"),
    }
}
