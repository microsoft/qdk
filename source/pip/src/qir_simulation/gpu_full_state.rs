// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir_simulation::{NoiseConfig, QirInstruction, QirInstructionId, unbind_noise_config};
use pyo3::{
    IntoPyObjectExt,
    exceptions::{PyOSError, PyRuntimeError, PyValueError},
    prelude::*,
    types::PyList,
};
use qdk_simulators::shader_types::Op;

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
pub fn try_create_gpu_adapter() -> PyResult<String> {
    let name = qdk_simulators::try_create_gpu_adapter().map_err(PyOSError::new_err)?;
    Ok(name)
}

#[pyfunction]
pub fn run_parallel_shots<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyList>,
    shots: i32,
    qubit_count: i32,
    result_count: i32,
    noise_config: Option<&Bound<'py, NoiseConfig>>,
    seed: Option<u32>,
) -> PyResult<Py<PyAny>> {
    // First convert the Python objects to Rust types
    let mut ops: Vec<Op> = Vec::with_capacity(input.len());
    for intr in input {
        // Error if the instruction can't be converted
        let item: QirInstruction = intr
            .extract()
            .map_err(|e| PyValueError::new_err(format!("expected QirInstruction: {e}")))?;
        // However some ops can't be mapped (e.g. OutputRecording), so skip those
        if let Some(op) = map_instruction(&item) {
            ops.push(op);
        }
    }

    let noise = noise_config.map(|noise_config| unbind_noise_config(py, noise_config));

    let rng_seed = seed.unwrap_or(0xfeed_face);

    let sim_results = qdk_simulators::run_shots_with_noise(
        qubit_count,
        result_count,
        ops,
        shots,
        rng_seed,
        &noise,
    )
    .map_err(PyRuntimeError::new_err)?;

    // Collect and format the results into a Python list of strings
    let result_count: usize = result_count
        .try_into()
        .map_err(|e| PyValueError::new_err(format!("invalid result count {result_count}: {e}")))?;

    // Turn each shot's results into a string, with '0' for 0, '1' for 1, and 'L' for lost qubits
    // The results are a flat list of u32, with each shot's results in sequence + one error code,
    // so we need to chunk them up accordingly
    let str_results = sim_results
        .chunks(result_count + 1)
        .map(|chunk| &chunk[..result_count])
        .map(|shot_results| {
            let mut bitstring = String::with_capacity(result_count);
            for res in shot_results {
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

fn map_instruction(qir_inst: &QirInstruction) -> Option<Op> {
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
                Op::new_mresetz_gate(*control, *target)
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
        QirInstruction::ThreeQubitGate(..) => panic!("unsupported instruction: {qir_inst:?}"),
    };
    Some(op)
}
