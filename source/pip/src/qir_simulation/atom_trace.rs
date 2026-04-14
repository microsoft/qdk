// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Neutral-atom QIR trace pass (read-only).
//!
//! Rust equivalent of `_trace.py`.  Walks the entry-point function and
//! builds a trace structure describing the sequence of parallel/serial
//! steps and operations (move, sx, rz, cz, mz).

use pyo3::{
    Bound, IntoPyObject, IntoPyObjectExt, PyResult, Python,
    exceptions::PyValueError,
    pyfunction,
    types::{PyDict, PyDictMethods, PyList, PyListMethods},
};
use qsc_llvm::{
    model::{Instruction, Operand},
    parse_module,
    qir::{find_entry_point, get_function_attribute, qis, rt},
};
use rustc_hash::FxHashMap;

use super::atom_utils::extract_id;

/// Build an execution trace for a neutral-atom QIR program.
///
/// Returns a Python dict with keys:
///   - `"qubits"`: list of `(row, col)` tuples (home locations, truncated to
///     `required_num_qubits` if present)
///   - `"steps"`: list of step dicts, each with `"id"` (int) and `"ops"` (list of str)
///
/// `home_locs` is the full device home-location list passed from Python.
#[pyfunction]
pub fn trace_atom_program<'py>(
    py: Python<'py>,
    ir: &str,
    home_locs: Vec<(i64, i64)>,
) -> PyResult<Bound<'py, PyDict>> {
    let module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("failed to parse IR: {e}")))?;

    let entry_idx = find_entry_point(&module)
        .ok_or_else(|| PyValueError::new_err("no entry point function found in IR"))?;

    // Optionally truncate home_locs to required_num_qubits
    let num_qubits: Option<usize> =
        get_function_attribute(&module, entry_idx, "required_num_qubits")
            .and_then(|s| s.parse().ok());

    let used_locs = match num_qubits {
        Some(n) if n < home_locs.len() => &home_locs[..n],
        _ => &home_locs,
    };

    // Build the Python "qubits" list
    let py_qubits = PyList::empty(py);
    for &(row, col) in used_locs {
        let tup = (row, col).into_pyobject(py)?;
        py_qubits.append(tup)?;
    }

    // Walk the entry function
    let mut steps: Vec<Step> = Vec::new();
    let mut in_parallel = false;
    let mut q_cols: FxHashMap<u32, i64> = FxHashMap::default();

    let entry_func = &module.functions[entry_idx];
    for block in &entry_func.basic_blocks {
        for instr in &block.instructions {
            if let Instruction::Call { callee, args, .. } = instr {
                match callee.as_str() {
                    s if s == rt::BEGIN_PARALLEL => {
                        steps.push(Step::new(steps.len()));
                        in_parallel = true;
                    }
                    s if s == rt::END_PARALLEL => {
                        in_parallel = false;
                    }
                    s if s == qis::MOVE => {
                        if !in_parallel {
                            steps.push(Step::new(steps.len()));
                        }
                        if let (Some(q), Some(row_val), Some(col_val)) = (
                            args.first().and_then(|(_, op)| extract_id(op)),
                            args.get(1).and_then(|(_, op)| extract_int_val(op)),
                            args.get(2).and_then(|(_, op)| extract_int_val(op)),
                        ) {
                            q_cols.insert(q, col_val);
                            if let Some(step) = steps.last_mut() {
                                step.ops.push(format!("move({row_val}, {col_val}) {q}"));
                            }
                        }
                    }
                    s if s == qis::SX => {
                        if !in_parallel {
                            steps.push(Step::new(steps.len()));
                        }
                        if let Some(q) = args.first().and_then(|(_, op)| extract_id(op)) {
                            if let Some(step) = steps.last_mut() {
                                step.ops.push(format!("sx {q}"));
                            }
                        }
                    }
                    s if s == qis::RZ => {
                        if !in_parallel {
                            steps.push(Step::new(steps.len()));
                        }
                        if let (Some(angle), Some(q)) = (
                            args.first().and_then(|(_, op)| extract_float_val(op)),
                            args.get(1).and_then(|(_, op)| extract_id(op)),
                        ) {
                            if let Some(step) = steps.last_mut() {
                                step.ops.push(format!("rz({angle}) {q}"));
                            }
                        }
                    }
                    s if s == qis::CZ => {
                        if !in_parallel {
                            steps.push(Step::new(steps.len()));
                        }
                        if let (Some(mut q1), Some(mut q2)) = (
                            args.first().and_then(|(_, op)| extract_id(op)),
                            args.get(1).and_then(|(_, op)| extract_id(op)),
                        ) {
                            // Sort by column so lower-column qubit comes first
                            let c1 = q_cols.get(&q1).copied().unwrap_or(-1);
                            let c2 = q_cols.get(&q2).copied().unwrap_or(-1);
                            if c1 > c2 {
                                std::mem::swap(&mut q1, &mut q2);
                            }
                            if let Some(step) = steps.last_mut() {
                                step.ops.push(format!("cz {q1}, {q2}"));
                            }
                        }
                    }
                    s if s == qis::MRESETZ => {
                        if !in_parallel {
                            steps.push(Step::new(steps.len()));
                        }
                        if let Some(q) = args.first().and_then(|(_, op)| extract_id(op)) {
                            if let Some(step) = steps.last_mut() {
                                step.ops.push(format!("mz {q}"));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Build final Python dict
    let py_steps = PyList::empty(py);
    for step in &steps {
        let d = PyDict::new(py);
        d.set_item("id", step.id)?;
        let ops = PyList::empty(py);
        for op in &step.ops {
            ops.append(op.into_py_any(py)?)?;
        }
        d.set_item("ops", ops)?;
        py_steps.append(d)?;
    }

    let result = PyDict::new(py);
    result.set_item("qubits", py_qubits)?;
    result.set_item("steps", py_steps)?;
    Ok(result)
}

struct Step {
    id: usize,
    ops: Vec<String>,
}

impl Step {
    fn new(id: usize) -> Self {
        Self {
            id,
            ops: Vec::new(),
        }
    }
}

fn extract_int_val(operand: &Operand) -> Option<i64> {
    match operand {
        Operand::IntConst(_, v) => Some(*v),
        _ => None,
    }
}

fn extract_float_val(operand: &Operand) -> Option<f64> {
    match operand {
        Operand::FloatConst(_, v) => Some(*v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_float_val, extract_int_val};

    /// Verify that extract helpers work correctly.
    #[test]
    fn extract_int_and_float() {
        use qsc_llvm::{model::Operand, model::Type};

        assert_eq!(
            extract_int_val(&Operand::IntConst(Type::Integer(64), 42)),
            Some(42)
        );
        assert_eq!(extract_int_val(&Operand::NullPtr), None);

        let f = extract_float_val(&Operand::float_const(Type::Double, std::f64::consts::PI));
        assert!((f.expect("should be Some") - std::f64::consts::PI).abs() < 1e-10);
        assert_eq!(extract_float_val(&Operand::NullPtr), None);
    }
}
