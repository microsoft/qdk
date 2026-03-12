// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir_simulation::{NoiseConfig, QirInstruction, QirInstructionId, unbind_noise_config};
use pyo3::{
    IntoPyObjectExt, PyResult,
    exceptions::{PyKeyError, PyOSError, PyRuntimeError, PyValueError},
    prelude::*,
    pyclass, pymethods,
    types::{PyDict, PyList},
};
use qdk_simulators::adaptive_bytecode::{self, AdaptiveProgram, Instruction};
use qdk_simulators::gpu_context;
use qdk_simulators::shader_types::Op;

use std::sync::Mutex;

/// Tuple representation of bytecode instructions from the Python adaptive pass.
/// Fields: (opcode, dst, src0, src1, src2, src3, src4, src5)
type RawInstructionTuple = (u32, u32, u32, u32, u32, u32, u32, u32);

/// Checks if a compatible GPU adapter is available on the system.
///
/// This function attempts to request a GPU adapter to determine if GPU-accelerated
/// quantum simulation is supported. It's useful for capability detection before
/// attempting to run GPU-based simulations.
///
/// # Errors
///
/// Raises `OSError` if:
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

    let sim_results =
        qdk_simulators::run_shots_sync(qubit_count, result_count, &ops, &noise, shots, rng_seed, 0)
            .map_err(PyRuntimeError::new_err)?;

    // Collect and format the results into a Python list of strings
    let result_count: usize = result_count
        .try_into()
        .map_err(|e| PyValueError::new_err(format!("invalid result count {result_count}: {e}")))?;

    // Turn each shot's results into a string, with '0' for 0, '1' for 1, and 'L' for lost qubits
    // The results are a flat list of u32, with each shot's results in sequence + one error code,
    // so we need to chunk them up accordingly
    let str_results = sim_results
        .shot_results
        .iter()
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

type NativeGpuContext = gpu_context::GpuContext;
#[derive(Debug)]
#[pyclass(module = "qsharp._native")]
pub struct GpuContext {
    native_context: Mutex<NativeGpuContext>,
    last_set_result_count: usize, // Needed to format results
}

#[pymethods]
impl GpuContext {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(GpuContext {
            native_context: Mutex::new(NativeGpuContext::default()),
            last_set_result_count: 0,
        })
    }

    fn load_noise_tables(&mut self, dir_path: &str) -> PyResult<Vec<(u32, String, u32)>> {
        let mut gpu_context = self
            .native_context
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Unable to obtain lock on the GPU context"))?;

        gpu_context.clear_correlated_noise_tables();
        for entry in std::fs::read_dir(dir_path)? {
            let entry = entry?;
            let path = entry.path();
            let is_file = path.is_file();
            // let ends_with_csv = path.extension().map_or(false, |ext| ext == "csv");
            let ends_with_csv = path.extension() == Some("csv".as_ref());

            if is_file && ends_with_csv {
                let contents = std::fs::read_to_string(&path)?;
                let filename = path
                    .file_stem()
                    .expect("file should have a name")
                    .to_str()
                    .expect("file name should be a valid unicode string");
                gpu_context.add_correlated_noise_table(filename, &contents);
            }
        }
        Ok(gpu_context.get_correlated_noise_tables())
    }

    fn get_noise_table_ids(&self) -> PyResult<Vec<(u32, String, u32)>> {
        self.native_context
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Unable to obtain lock on the GPU context"))
            .map(|context| Ok(context.get_correlated_noise_tables()))?
    }

    fn set_program(
        &mut self,
        input: &Bound<'_, PyList>,
        qubit_count: i32,
        result_count: i32,
    ) -> PyResult<()> {
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
        self.native_context
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Unable to obtain lock on the GPU context"))?
            .set_program(&ops, qubit_count, result_count);

        // Save the result count for formatting later
        self.last_set_result_count = result_count.try_into().map_err(|e| {
            PyValueError::new_err(format!("invalid result count {result_count}: {e}"))
        })?;
        Ok(())
    }

    fn set_adaptive_program(&mut self, program: &Bound<'_, PyDict>) -> PyResult<()> {
        // Extract scalar fields
        let version: u32 = program
            .get_item("version")?
            .ok_or_else(|| PyKeyError::new_err("version"))?
            .extract()?;
        if version != 1 {
            return Err(PyValueError::new_err(format!(
                "unsupported adaptive program version: {version}, expected 1"
            )));
        }

        let num_qubits: u32 = program
            .get_item("num_qubits")?
            .ok_or_else(|| PyKeyError::new_err("num_qubits"))?
            .extract()?;
        let num_results: u32 = program
            .get_item("num_results")?
            .ok_or_else(|| PyKeyError::new_err("num_results"))?
            .extract()?;
        let num_registers: u32 = program
            .get_item("num_registers")?
            .ok_or_else(|| PyKeyError::new_err("num_registers"))?
            .extract()?;
        let entry_block: u32 = program
            .get_item("entry_block")?
            .ok_or_else(|| PyKeyError::new_err("entry_block"))?
            .extract()?;

        // Extract array fields
        let blocks: Vec<(u32, u32, u32, u32)> = program
            .get_item("blocks")?
            .ok_or_else(|| PyKeyError::new_err("blocks"))?
            .extract()?;
        let instructions: Vec<RawInstructionTuple> = program
            .get_item("instructions")?
            .ok_or_else(|| PyKeyError::new_err("instructions"))?
            .extract()?;
        let quantum_ops_raw: Vec<(u32, u32, u32, u32, f64)> = program
            .get_item("quantum_ops")?
            .ok_or_else(|| PyKeyError::new_err("quantum_ops"))?
            .extract()?;
        let functions: Vec<(u32, u32, u32, u32)> = program
            .get_item("functions")?
            .ok_or_else(|| PyKeyError::new_err("functions"))?
            .extract()?;
        let phi_entries: Vec<(u32, u32)> = program
            .get_item("phi_entries")?
            .ok_or_else(|| PyKeyError::new_err("phi_entries"))?
            .extract()?;
        let switch_cases: Vec<(u32, u32)> = program
            .get_item("switch_cases")?
            .ok_or_else(|| PyKeyError::new_err("switch_cases"))?
            .extract()?;
        let call_args: Vec<u32> = program
            .get_item("call_args")?
            .ok_or_else(|| PyKeyError::new_err("call_args"))?
            .extract()?;

        // Build quantum Op pool using existing gate constructors
        let op_pool = adaptive_bytecode::build_op_pool(&quantum_ops_raw);

        // Convert instructions to Instruction structs
        let bytecode: Vec<Instruction> = instructions
            .iter()
            .map(|t| Instruction::from_tuple(*t))
            .collect();

        // Convert block table: strip block_id and pred_count, keep (instr_offset, instr_count)
        let block_table: Vec<[u32; 2]> = blocks
            .iter()
            .map(|&(_block_id, instr_offset, instr_count, _pred_count)| [instr_offset, instr_count])
            .collect();

        // Convert function table
        let function_table: Vec<[u32; 4]> = functions
            .iter()
            .map(|&(entry_block_id, param_count, param_base_reg, reserved)| {
                [entry_block_id, param_count, param_base_reg, reserved]
            })
            .collect();

        // Convert phi entries and switch cases
        let phi_table: Vec<[u32; 2]> = phi_entries
            .iter()
            .map(|&(pred_block, value_reg)| [pred_block, value_reg])
            .collect();
        let switch_table: Vec<[u32; 2]> = switch_cases
            .iter()
            .map(|&(match_val, target_block)| [match_val, target_block])
            .collect();

        let adaptive_program = AdaptiveProgram {
            bytecode,
            block_table,
            function_table,
            quantum_ops: op_pool,
            phi_entries: phi_table,
            switch_cases: switch_table,
            call_args,
            num_qubits,
            num_results,
            num_registers,
            entry_block,
        };

        self.native_context
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Unable to obtain lock on the GPU context"))?
            .set_adaptive_program(adaptive_program);

        // Save the result count for formatting later
        self.last_set_result_count = num_results.try_into().map_err(|e| {
            PyValueError::new_err(format!("invalid result count {num_results}: {e}"))
        })?;

        Ok(())
    }

    fn set_noise<'py>(
        &mut self,
        py: Python<'py>,
        noise_config: &Bound<'py, NoiseConfig>,
    ) -> PyResult<()> {
        let noise = unbind_noise_config(py, noise_config);
        self.native_context
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Unable to obtain lock on the GPU context"))?
            .set_noise_config(noise);

        Ok(())
    }

    fn run_adaptive_shots(
        &self,
        py: Python<'_>,
        shot_count: i32,
        seed: u32,
    ) -> PyResult<Py<PyAny>> {
        let mut gpu_context = self
            .native_context
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Unable to obtain lock on the GPU context"))?;

        let results = gpu_context
            .run_adaptive_shots_sync(shot_count, seed)
            .map_err(PyRuntimeError::new_err)?;

        Self::format_results(py, results, self.last_set_result_count)
    }

    fn run_shots(&self, py: Python<'_>, shot_count: i32, seed: u32) -> PyResult<Py<PyAny>> {
        let mut gpu_context = self
            .native_context
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Unable to obtain lock on the GPU context"))?;

        let results = gpu_context
            .run_shots_sync(shot_count, seed, 0)
            .map_err(|_| PyRuntimeError::new_err("Unable to obtain lock on the GPU context"))?;

        Self::format_results(py, results, self.last_set_result_count)
    }
}

impl GpuContext {
    fn format_results(
        py: Python<'_>,
        results: gpu_context::RunResults,
        result_count: usize,
    ) -> PyResult<Py<PyAny>> {
        let str_results = results
            .shot_results
            .iter()
            .map(|shot_results| {
                let mut bitstring = String::with_capacity(result_count);
                for res in shot_results {
                    let char = match res {
                        0 => '0',
                        1 => '1',
                        _ => 'L',
                    };
                    bitstring.push(char);
                }
                bitstring
            })
            .collect::<Vec<String>>();

        let dict = PyDict::new(py);
        dict.set_item("shot_results", PyList::new(py, str_results)?)
            .map_err(|e| PyValueError::new_err(format!("failed to set results in dict: {e}")))?;
        dict.set_item(
            "shot_result_codes",
            PyList::new(py, results.shot_result_codes)?,
        )
        .map_err(|e| PyValueError::new_err(format!("failed to set result codes in dict: {e}")))?;

        if let Some(diagnostics) = results.diagnostics {
            dict.set_item("diagnostics", format!("{diagnostics:?}"))
                .map_err(|e| {
                    PyValueError::new_err(format!("failed to set diagnostics in dict: {e}"))
                })?;
        }
        dict.into_py_any(py)
    }
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
            QirInstructionId::RESET => Op::new_resetz_gate(*qubit),
            _ => {
                panic!("unsupported one-qubit gate: {id:?} on qubit {qubit}");
            }
        },
        QirInstruction::TwoQubitGate(id, control, target) => match id {
            QirInstructionId::M | QirInstructionId::MZ => Op::new_mz_gate(*control, *target),
            QirInstructionId::MResetZ => Op::new_mresetz_gate(*control, *target),
            QirInstructionId::CX | QirInstructionId::CNOT => Op::new_cx_gate(*control, *target),
            QirInstructionId::CY => Op::new_cy_gate(*control, *target),
            QirInstructionId::CZ => Op::new_cz_gate(*control, *target),
            QirInstructionId::SWAP => Op::new_swap_gate(*control, *target),
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
        QirInstruction::CorrelatedNoise(_, table_id, qubit_args) => {
            Op::new_correlated_noise_gate(*table_id, qubit_args)
        }
    };
    Some(op)
}
