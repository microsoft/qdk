// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::interpreter::Result as QsResult;
use crate::qir_simulation::{
    NoiseConfig, QirInstruction, QirInstructionId, adaptive_program_from_pydict,
    unbind_noise_config,
};
use pyo3::{
    IntoPyObjectExt, PyResult,
    exceptions::{PyOSError, PyRuntimeError, PyValueError},
    prelude::*,
    pyclass, pymethods,
    types::{PyDict, PyList},
};
use qdk_simulators::gpu_context;
use qdk_simulators::shader_types::Op;

use std::sync::Mutex;

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
    qubit_count: i32,
    result_count: i32,
    shots: i32,
    noise_config: Option<&Bound<'py, NoiseConfig>>,
    seed: Option<u32>,
) -> PyResult<Py<PyAny>> {
    // First convert the Python objects to Rust types
    let mut ops: Vec<Op> = Vec::with_capacity(input.len());
    // Result ids recorded via `result_record_output`, in record order. Used to
    // build the per-shot output record stream from the raw measurement results.
    let mut recorded_result_ids: Vec<usize> = Vec::new();
    let mut has_output_recording = false;
    for intr in input {
        // Error if the instruction can't be converted
        let item: QirInstruction = intr
            .extract()
            .map_err(|e| PyValueError::new_err(format!("expected QirInstruction: {e}")))?;
        if let QirInstruction::OutputRecording(id, value, _tag) = &item {
            has_output_recording = true;
            if *id == QirInstructionId::ResultRecordOutput {
                recorded_result_ids.push(value.parse().unwrap_or(0));
            }
        } else if let Some(op) = map_instruction(&item) {
            // However some ops can't be mapped (e.g. OutputRecording), so skip those
            ops.push(op);
        }
    }

    let noise = noise_config.map(|noise_config| unbind_noise_config(py, noise_config));

    let rng_seed = seed.unwrap_or(0xfeed_face);

    let sim_results =
        qdk_simulators::run_shots_sync(qubit_count, result_count, &ops, &noise, shots, rng_seed, 0)
            .map_err(PyRuntimeError::new_err)?;

    // Build the per-shot unified output record stream. When the program records
    // outputs, each shot yields its recorded measurement results (as `Result`
    // enum values) in record order; otherwise it falls back to every measurement
    // result in order, matching the historical raw-measurement behavior.
    let mut entries = Vec::with_capacity(sim_results.shot_results.len());
    for shot_results in &sim_results.shot_results {
        let mut values = Vec::new();
        if has_output_recording {
            for &result_id in &recorded_result_ids {
                let raw = shot_results.get(result_id).copied().unwrap_or(0);
                values.push(measurement_u32_to_py(py, raw)?);
            }
        } else {
            for &raw in shot_results {
                values.push(measurement_u32_to_py(py, raw)?);
            }
        }
        entries.push(
            PyList::new(py, values)
                .map_err(|e| PyValueError::new_err(format!("failed to create Python list: {e}")))?
                .into_py_any(py)?,
        );
    }

    PyList::new(py, entries)
        .map_err(|e| PyValueError::new_err(format!("failed to create Python list: {e}")))?
        .into_py_any(py)
}

type NativeGpuContext = gpu_context::GpuContext;
#[derive(Debug)]
#[pyclass(module = "qdk._native")]
pub struct GpuContext {
    native_context: Mutex<NativeGpuContext>,
    last_set_result_count: usize, // Needed to format results
    // Type tag (0=result, 3=bool, 4=int, 5=double) for each output record,
    // indexed by the record's ordinal. Used to coerce raw GPU values to Python.
    output_record_types: Vec<u32>,
}

#[pymethods]
impl GpuContext {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(GpuContext {
            native_context: Mutex::new(NativeGpuContext::default()),
            last_set_result_count: 0,
            output_record_types: Vec::new(),
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
        let mut gpu_context = self
            .native_context
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Unable to obtain lock on the GPU context"))?;

        gpu_context.switch_to_base();

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
        gpu_context.set_program(&ops, qubit_count, result_count);

        // Save the result count for formatting later
        self.last_set_result_count = result_count.try_into().map_err(|e| {
            PyValueError::new_err(format!("invalid result count {result_count}: {e}"))
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

    fn run_shots(&self, py: Python<'_>, shot_count: i32, seed: u32) -> PyResult<Py<PyAny>> {
        let mut gpu_context = self
            .native_context
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Unable to obtain lock on the GPU context"))?;

        if gpu_context.is_adaptive() {
            return Err(PyRuntimeError::new_err(
                "Context should be non-adaptive. Try setting a base profile program first with `.set_program()`",
            ));
        }

        let results = gpu_context
            .run_shots_sync(shot_count, seed, 0)
            .map_err(|_| PyRuntimeError::new_err("Unable to obtain lock on the GPU context"))?;

        let str_results = results
            .shot_results
            .iter()
            .map(|shot_results| {
                let mut bitstring = String::with_capacity(self.last_set_result_count);
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

        let dict = PyDict::new(py);

        dict.set_item("shot_results", PyList::new(py, str_results)?)
            .map_err(|e| PyValueError::new_err(format!("failed to set results in dict: {e}")))?;
        dict.set_item(
            "shot_result_codes",
            PyList::new(py, results.shot_result_codes)?,
        )
        .map_err(|e| PyValueError::new_err(format!("failed to set result codes in dict: {e}")))?;

        if let Some(diagnostics) = results.diagnostics {
            // DiagnosticsData doesn't implement Serialize, so use Debug formatting
            dict.set_item("diagnostics", format!("{diagnostics:?}"))
                .map_err(|e| {
                    PyValueError::new_err(format!("failed to set diagnostics in dict: {e}"))
                })?;
        }
        dict.into_py_any(py)
    }

    fn set_adaptive_program(&mut self, program: &Bound<'_, PyDict>) -> PyResult<()> {
        let mut gpu_context = self
            .native_context
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Unable to obtain lock on the GPU context"))?;

        gpu_context.swith_to_adaptive();

        let adaptive_program = adaptive_program_from_pydict(program)?;
        let num_results = adaptive_program.num_results;
        let record_types = output_record_types(&adaptive_program);

        gpu_context
            .set_adaptive_program(adaptive_program)
            .map_err(PyValueError::new_err)?;

        // Save the result count and output record types for formatting later
        self.last_set_result_count = num_results.try_into().map_err(|e| {
            PyValueError::new_err(format!("invalid result count {num_results}: {e}"))
        })?;
        self.output_record_types = record_types;

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

        if !gpu_context.is_adaptive() {
            return Err(PyRuntimeError::new_err(
                "Context should be adaptive. Try setting an adaptive program first with `.set_adaptive_program()`",
            ));
        }

        let results = gpu_context
            .run_adaptive_shots_sync(shot_count, seed, 0)
            .map_err(PyRuntimeError::new_err)?;

        Self::format_results(
            py,
            results,
            self.last_set_result_count,
            &self.output_record_types,
        )
    }
}

impl GpuContext {
    fn format_results(
        py: Python<'_>,
        results: gpu_context::RunResults,
        result_count: usize,
        record_types: &[u32],
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

        dict.set_item(
            "shot_output_records",
            output_records_to_pylist(py, record_types, &results.shot_output_records)?,
        )
        .map_err(|e| PyValueError::new_err(format!("failed to set output records in dict: {e}")))?;

        if let Some(diagnostics) = results.diagnostics {
            dict.set_item("diagnostics", format!("{diagnostics:?}"))
                .map_err(|e| {
                    PyValueError::new_err(format!("failed to set diagnostics in dict: {e}"))
                })?;
        }
        dict.into_py_any(py)
    }
}

/// Build the per-ordinal output record type table from an adaptive program.
/// Each leaf `__quantum__rt__*_record_output` is emitted with a type tag in
/// `aux1` (0=result, 3=bool, 4=int, 5=double) and a stable ordinal in `aux2`.
/// The returned vector maps ordinal -> type tag.
fn output_record_types(program: &qdk_simulators::bytecode::AdaptiveProgram<u32>) -> Vec<u32> {
    const OP_RECORD_OUTPUT: u32 = 0x14;
    let mut types: Vec<u32> = Vec::new();
    for instr in &program.instructions {
        if (instr.opcode & 0xFF) == OP_RECORD_OUTPUT && (instr.aux1 == 0 || instr.aux1 >= 3) {
            let ordinal = instr.aux2 as usize;
            if ordinal >= types.len() {
                types.resize(ordinal + 1, 0);
            }
            types[ordinal] = instr.aux1;
        }
    }
    types
}

/// Coerce a raw GPU measurement value (0=Zero, 1=One, else Loss) to its Python
/// `Result` enum counterpart.
fn measurement_u32_to_py(py: Python<'_>, value: u32) -> PyResult<Py<PyAny>> {
    let result = match value {
        0 => QsResult::Zero,
        1 => QsResult::One,
        _ => QsResult::Loss,
    };
    result.into_py_any(py)
}

/// Coerce one raw output record value (32-bit) to a native Python object based
/// on its type tag (0=result, 3=bool, 4=int, 5=double). On the GPU ints are
/// 32-bit and doubles are stored as f32, so values are reinterpreted
/// accordingly; results are the raw measurement code (0/1/2).
fn output_record_to_py(py: Python<'_>, type_tag: u32, bits: u32) -> PyResult<Py<PyAny>> {
    #[allow(clippy::cast_possible_wrap)]
    match type_tag {
        0 => measurement_u32_to_py(py, bits),
        3 => (bits != 0).into_py_any(py),
        4 => i64::from(bits as i32).into_py_any(py),
        5 => f64::from(f32::from_bits(bits)).into_py_any(py),
        _ => i64::from(bits).into_py_any(py),
    }
}

/// Convert the per-shot raw output record values into a Python list with one
/// inner list of native values (`Result`/bool/int/float) per shot.
fn output_records_to_pylist<'py>(
    py: Python<'py>,
    record_types: &[u32],
    shot_output_records: &[Vec<u32>],
) -> PyResult<Bound<'py, PyList>> {
    let mut per_shot = Vec::with_capacity(shot_output_records.len());
    for shot_records in shot_output_records {
        let mut values = Vec::with_capacity(shot_records.len());
        for (ordinal, &bits) in shot_records.iter().enumerate() {
            let type_tag = record_types.get(ordinal).copied().unwrap_or(0);
            values.push(output_record_to_py(py, type_tag, bits)?);
        }
        per_shot.push(PyList::new(py, values)?.into_py_any(py)?);
    }
    PyList::new(py, per_shot)
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

#[pyfunction]
pub fn run_adaptive_parallel_shots<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyDict>,
    shots: i32,
    noise_config: Option<&Bound<'py, NoiseConfig>>,
    seed: Option<u32>,
) -> PyResult<Py<PyAny>> {
    let noise = noise_config.map(|noise_config| unbind_noise_config(py, noise_config));
    let rng_seed = seed.unwrap_or(0xfeed_face);
    let program = adaptive_program_from_pydict(input)?;
    let record_types = output_record_types(&program);
    let sim_results = qdk_simulators::run_adaptive_shots_sync(program, &noise, shots, rng_seed, 0)
        .map_err(PyRuntimeError::new_err)?;

    // Format each shot as the ordered list of its recorded output values
    // (`Result` enum values for measurement results, plus native bool/int/float
    // for classical records), in record order.
    let records = output_records_to_pylist(py, &record_types, &sim_results.shot_output_records)?;
    records.into_py_any(py)
}
