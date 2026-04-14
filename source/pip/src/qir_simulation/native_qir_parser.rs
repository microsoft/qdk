// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::fmt::Write;

use pyo3::{
    Bound, IntoPyObject, PyResult, Python,
    exceptions::PyValueError,
    pyfunction,
    types::{PyAnyMethods, PyDict, PyDictMethods, PyList, PyListMethods, PyTuple},
};
use qsc_llvm::{
    model::Type,
    model::{Attribute, Constant, Instruction, Module, Operand},
    parse_module,
    qir::{find_entry_point, get_function_attribute, qis, rt},
};
use rustc_hash::FxHashMap;

use super::QirInstructionId;

/// Check if a function (by name) has `qdk_noise` in its attribute groups.
fn function_has_qdk_noise(module: &Module, func_name: &str) -> bool {
    module
        .functions
        .iter()
        .find(|f| f.name == func_name)
        .is_some_and(|func| {
            func.attribute_group_refs.iter().any(|&group_ref| {
                module
                    .attribute_groups
                    .iter()
                    .find(|ag| ag.id == group_ref)
                    .is_some_and(|ag| {
                        ag.attributes
                            .iter()
                            .any(|attr| matches!(attr, Attribute::StringAttr(s) if s.contains("qdk_noise")))
                    })
            })
        })
}

/// Extract qubit/result ID from an `Operand`.
/// In QIR, qubit and result references use `inttoptr` patterns.
/// PyQIR also normalizes `inttoptr (i64 0 to %T*)` to `null`, so we
/// handle `NullPtr` as ID 0.
fn extract_id(operand: &Operand) -> PyResult<u32> {
    match operand {
        Operand::IntToPtr(val, _) => Ok(u32::try_from(*val).map_err(|_| {
            PyValueError::new_err(format!("qubit/result ID {val} out of range for u32"))
        })?),
        Operand::NullPtr => Ok(0),
        other => Err(PyValueError::new_err(format!(
            "expected inttoptr operand for qubit/result ID, got {other:?}"
        ))),
    }
}

/// Extract a float value from an operand (for rotation gate angles).
fn extract_float(operand: &Operand) -> PyResult<f64> {
    match operand {
        Operand::FloatConst(_, val) => Ok(*val),
        other => Err(PyValueError::new_err(format!(
            "expected float constant for rotation angle, got {other:?}"
        ))),
    }
}

/// Extract an integer value from an operand (for array/tuple record output count).
fn extract_int(operand: &Operand) -> PyResult<i64> {
    match operand {
        Operand::IntConst(_, val) => Ok(*val),
        other => Err(PyValueError::new_err(format!(
            "expected integer constant, got {other:?}"
        ))),
    }
}

/// Look up a global variable's string initializer by name.
fn lookup_global_string<'a>(module: &'a Module, name: &str) -> &'a str {
    for global in &module.globals {
        if global.name == name
            && let Some(Constant::CString(s)) = &global.initializer
        {
            return s.as_str();
        }
    }
    ""
}

/// Extract a tag string from an operand, which is typically a `GlobalRef`
/// pointing to a global with a `CString` initializer.
fn extract_tag(module: &Module, operand: &Operand) -> String {
    match operand {
        Operand::GlobalRef(name) => lookup_global_string(module, name).to_string(),
        _ => String::new(),
    }
}

/// Detect the QIR profile from text IR.
///
/// Parses the IR, finds the entry point function, and reads
/// the "qir_profiles" attribute value.
///
/// Returns `base_profile`, `adaptive_profile`, or `unknown`.
#[pyfunction]
pub fn get_qir_profile(ir: &str) -> PyResult<String> {
    let module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("failed to parse IR: {e}")))?;

    let entry_idx = find_entry_point(&module)
        .ok_or_else(|| PyValueError::new_err("no entry point function found in IR"))?;

    let profile = get_function_attribute(&module, entry_idx, "qir_profiles").unwrap_or("unknown");

    Ok(profile.to_string())
}

/// Parse Base Profile QIR and extract gate sequence, qubit/result counts,
/// and output format string.
///
/// Returns a tuple: `(gates_list, num_qubits, num_results, output_format_str)`
///
/// The `gates_list` contains Python tuples matching the format produced by
/// the `AggregateGatesPass` and `CorrelatedNoisePass` in the Python code.
#[pyfunction]
#[pyo3(signature = (ir, noise_intrinsics=None))]
pub fn parse_base_profile_qir<'py>(
    py: Python<'py>,
    ir: &str,
    noise_intrinsics: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyTuple>> {
    let module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("failed to parse IR: {e}")))?;

    let entry_idx = find_entry_point(&module)
        .ok_or_else(|| PyValueError::new_err("no entry point function found in IR"))?;

    let num_qubits = get_function_attribute(&module, entry_idx, "required_num_qubits")
        .ok_or_else(|| PyValueError::new_err("missing required_num_qubits attribute"))?
        .parse::<i64>()
        .map_err(|e| PyValueError::new_err(format!("invalid required_num_qubits: {e}")))?;

    let num_results = get_function_attribute(&module, entry_idx, "required_num_results")
        .ok_or_else(|| PyValueError::new_err("missing required_num_results attribute"))?
        .parse::<i64>()
        .map_err(|e| PyValueError::new_err(format!("invalid required_num_results: {e}")))?;

    // Build noise intrinsics lookup: gate_name -> table_id
    let noise_map: Option<FxHashMap<String, u32>> =
        noise_intrinsics.map(|dict: &Bound<'_, PyDict>| {
            let mut map = FxHashMap::default();
            for (key, value) in dict.iter() {
                if let (Ok(k), Ok(v)) = (key.extract::<String>(), value.extract::<u32>()) {
                    map.insert(k, v);
                }
            }
            map
        });

    let entry_func = &module.functions[entry_idx];
    let gates = PyList::empty(py);
    let mut output_str = String::new();
    let mut closers: Vec<&str> = Vec::new();
    let mut counters: Vec<i64> = Vec::new();

    for block in &entry_func.basic_blocks {
        // Check for branching control flow
        if let Some(last_instr) = block.instructions.last() {
            if matches!(last_instr, Instruction::Br { .. }) {
                return Err(PyValueError::new_err(
                    "simulation of programs with branching control flow is not supported",
                ));
            }
        }

        for instr in &block.instructions {
            if let Instruction::Call { callee, args, .. } = instr {
                process_call_instruction(
                    py,
                    &module,
                    callee,
                    args,
                    noise_map.as_ref(),
                    &gates,
                    &mut output_str,
                    &mut closers,
                    &mut counters,
                )?;
            }
        }
    }

    // Close any remaining output format closers
    while let Some(closer) = closers.pop() {
        output_str.push_str(closer);
        counters.pop();
    }

    let result = PyTuple::new(
        py,
        &[
            gates.into_any(),
            num_qubits.into_pyobject(py)?.into_any(),
            num_results.into_pyobject(py)?.into_any(),
            output_str.into_pyobject(py)?.into_any(),
        ],
    )?;

    Ok(result)
}

/// Process a single call instruction, appending to the gate list and/or
/// updating the output format string.
#[allow(clippy::too_many_arguments)]
fn process_call_instruction<'py>(
    py: Python<'py>,
    module: &Module,
    callee: &str,
    args: &[(Type, Operand)],
    noise_map: Option<&FxHashMap<String, u32>>,
    gates: &Bound<'py, PyList>,
    output_str: &mut String,
    closers: &mut Vec<&str>,
    counters: &mut Vec<i64>,
) -> PyResult<()> {
    // Check noise intrinsics first
    if let Some(map) = noise_map {
        if let Some(&table_id) = map.get(callee) {
            let qubit_ids = PyList::empty(py);
            for (_, operand) in args {
                qubit_ids.append(extract_id(operand)?)?;
            }
            let gate_tuple = PyTuple::new(
                py,
                &[
                    QirInstructionId::CorrelatedNoise
                        .into_pyobject(py)?
                        .into_any(),
                    table_id.into_pyobject(py)?.into_any(),
                    qubit_ids.into_any(),
                ],
            )?;
            gates.append(gate_tuple)?;
            return Ok(());
        }
        // If running noisy sim and callee is a noise intrinsic but not in the table, error
        if function_has_qdk_noise(module, callee) {
            return Err(PyValueError::new_err(format!(
                "Missing noise intrinsic: {callee}"
            )));
        }
    }

    if let Some(gate_tuple) = map_quantum_gate(py, callee, args)? {
        gates.append(gate_tuple)?;
    } else {
        process_output_or_runtime_call(
            py, module, callee, args, noise_map, gates, output_str, closers, counters,
        )?;
    }
    Ok(())
}

/// Map a quantum gate callee name to its Python tuple representation.
/// Returns `None` if the callee is not a recognized quantum gate.
fn map_quantum_gate<'py>(
    py: Python<'py>,
    callee: &str,
    args: &[(Type, Operand)],
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let tuple = match callee {
        qis::CCX => PyTuple::new(
            py,
            &[
                QirInstructionId::CCX.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[2].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::CX => PyTuple::new(
            py,
            &[
                QirInstructionId::CX.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::CY => PyTuple::new(
            py,
            &[
                QirInstructionId::CY.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::CZ => PyTuple::new(
            py,
            &[
                QirInstructionId::CZ.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::SWAP => PyTuple::new(
            py,
            &[
                QirInstructionId::SWAP.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::RX => PyTuple::new(
            py,
            &[
                QirInstructionId::RX.into_pyobject(py)?.into_any(),
                extract_float(&args[0].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::RXX => PyTuple::new(
            py,
            &[
                QirInstructionId::RXX.into_pyobject(py)?.into_any(),
                extract_float(&args[0].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[2].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::RY => PyTuple::new(
            py,
            &[
                QirInstructionId::RY.into_pyobject(py)?.into_any(),
                extract_float(&args[0].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::RYY => PyTuple::new(
            py,
            &[
                QirInstructionId::RYY.into_pyobject(py)?.into_any(),
                extract_float(&args[0].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[2].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::RZ => PyTuple::new(
            py,
            &[
                QirInstructionId::RZ.into_pyobject(py)?.into_any(),
                extract_float(&args[0].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::RZZ => PyTuple::new(
            py,
            &[
                QirInstructionId::RZZ.into_pyobject(py)?.into_any(),
                extract_float(&args[0].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[2].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::H => PyTuple::new(
            py,
            &[
                QirInstructionId::H.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::S => PyTuple::new(
            py,
            &[
                QirInstructionId::S.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::S_ADJ => PyTuple::new(
            py,
            &[
                QirInstructionId::SAdj.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::SX => PyTuple::new(
            py,
            &[
                QirInstructionId::SX.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::T => PyTuple::new(
            py,
            &[
                QirInstructionId::T.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::T_ADJ => PyTuple::new(
            py,
            &[
                QirInstructionId::TAdj.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::X => PyTuple::new(
            py,
            &[
                QirInstructionId::X.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::Y => PyTuple::new(
            py,
            &[
                QirInstructionId::Y.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::Z => PyTuple::new(
            py,
            &[
                QirInstructionId::Z.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::M | qis::MZ => {
            let id = if callee == qis::M {
                QirInstructionId::M
            } else {
                QirInstructionId::MZ
            };
            PyTuple::new(
                py,
                &[
                    id.into_pyobject(py)?.into_any(),
                    extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
                    extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
                ],
            )?
        }
        qis::MRESETZ => PyTuple::new(
            py,
            &[
                QirInstructionId::MResetZ.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
                extract_id(&args[1].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::RESET => PyTuple::new(
            py,
            &[
                QirInstructionId::RESET.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        qis::MOVE => PyTuple::new(
            py,
            &[
                QirInstructionId::Move.into_pyobject(py)?.into_any(),
                extract_id(&args[0].1)?.into_pyobject(py)?.into_any(),
            ],
        )?,
        _ => return Ok(None),
    };
    Ok(Some(tuple))
}

/// Process output recording and runtime calls that are not quantum gates.
#[allow(clippy::too_many_arguments)]
fn process_output_or_runtime_call<'py>(
    py: Python<'py>,
    module: &Module,
    callee: &str,
    args: &[(Type, Operand)],
    noise_map: Option<&FxHashMap<String, u32>>,
    gates: &Bound<'py, PyList>,
    output_str: &mut String,
    closers: &mut Vec<&str>,
    counters: &mut Vec<i64>,
) -> PyResult<()> {
    match callee {
        rt::RESULT_RECORD_OUTPUT => {
            let result_id = extract_id(&args[0].1)?;
            let tag = extract_tag(module, &args[1].1);
            let gate_tuple = PyTuple::new(
                py,
                &[
                    QirInstructionId::ResultRecordOutput
                        .into_pyobject(py)?
                        .into_any(),
                    result_id.to_string().into_pyobject(py)?.into_any(),
                    tag.into_pyobject(py)?.into_any(),
                ],
            )?;
            gates.append(gate_tuple)?;

            write!(output_str, "o[{result_id}]").expect("write to string should succeed");
            while !counters.is_empty() {
                output_str.push(',');
                let last = counters.last_mut().expect("counters should not be empty");
                *last -= 1;
                if *last == 0 {
                    output_str.push_str(closers.pop().expect("closers should match counters"));
                    counters.pop();
                } else {
                    break;
                }
            }
        }
        rt::TUPLE_RECORD_OUTPUT => {
            let count = extract_int(&args[0].1)?;
            let tag = extract_tag(module, &args[1].1);
            let gate_tuple = PyTuple::new(
                py,
                &[
                    QirInstructionId::TupleRecordOutput
                        .into_pyobject(py)?
                        .into_any(),
                    count.to_string().into_pyobject(py)?.into_any(),
                    tag.into_pyobject(py)?.into_any(),
                ],
            )?;
            gates.append(gate_tuple)?;

            // Output recording logic
            output_str.push('(');
            closers.push(")");
            counters.push(count);
        }
        rt::ARRAY_RECORD_OUTPUT => {
            let count = extract_int(&args[0].1)?;
            let tag = extract_tag(module, &args[1].1);
            let gate_tuple = PyTuple::new(
                py,
                &[
                    QirInstructionId::ArrayRecordOutput
                        .into_pyobject(py)?
                        .into_any(),
                    count.to_string().into_pyobject(py)?.into_any(),
                    tag.into_pyobject(py)?.into_any(),
                ],
            )?;
            gates.append(gate_tuple)?;

            // Output recording logic
            output_str.push('[');
            closers.push("]");
            counters.push(count);
        }
        rt::INITIALIZE | rt::BEGIN_PARALLEL | rt::END_PARALLEL | qis::BARRIER => {
            // Skip runtime/barrier calls
        }
        _ => {
            // For noiseless simulation, skip noise intrinsics silently
            if noise_map.is_none() && function_has_qdk_noise(module, callee) {
                return Ok(());
            }
            return Err(PyValueError::new_err(format!(
                "Unsupported call instruction: {callee}"
            )));
        }
    }
    Ok(())
}
