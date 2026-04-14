// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Neutral-atom instruction reordering pass.
//!
//! Rust equivalent of `_reorder.py`.  Reorders instructions within each
//! basic block to find contiguous sequences of the same gate on different
//! qubits, enabling better scheduling during execution.

use pyo3::{PyResult, exceptions::PyValueError, pyfunction};
use qsc_llvm::{
    model::Type,
    model::{Instruction, Module},
    parse_module,
    qir::{operand_key, qis, rt},
    write_module_to_string,
};
use rustc_hash::FxHashSet;

use super::atom_utils::extract_id;

/// A stand-in for value identity.  In the Python code every `Value` object
/// has identity; here we use the operand's debug representation as a key
/// (sufficient for inttoptr-based qubit/result pointers).
type ValKey = String;

/// Return (value_keys, result_keys) used by an instruction — mirrors `get_used_values`.
fn get_used_values(instr: &Instruction) -> (Vec<ValKey>, Vec<ValKey>) {
    let mut vals = Vec::new();
    let mut meas = Vec::new();

    if let Instruction::Call { callee, args, .. } = instr {
        match callee.as_str() {
            s if s == qis::MRESETZ || s == qis::M || s == qis::MZ => {
                // First arg is qubit value, rest are result values.
                if let Some(first) = args.first() {
                    vals.push(operand_key(&first.1));
                }
                for a in args.iter().skip(1) {
                    meas.push(operand_key(&a.1));
                }
            }
            s if s == qis::READ_RESULT || s == rt::READ_RESULT || s == rt::READ_ATOM_RESULT => {
                for a in args {
                    meas.push(operand_key(&a.1));
                }
            }
            _ => {
                for a in args {
                    vals.push(operand_key(&a.1));
                }
            }
        }
    }
    // Also include the instruction itself as a produced value (for result-producing calls).
    // In the Python code this is `vals.append(instr)` — we approximate with a unique repr.
    vals.push(format!("{instr:?}"));
    (vals, meas)
}

fn uses_any_value(used: &[ValKey], existing: &FxHashSet<ValKey>) -> bool {
    used.iter().any(|v| existing.contains(v))
}

fn is_output_recording(instr: &Instruction) -> bool {
    if let Instruction::Call { callee, .. } = instr {
        callee.ends_with("_record_output")
    } else {
        false
    }
}

/// Compute a sort key for an instruction based on its first qubit's home ordering.
/// `ordering_fn` maps qubit_id → device ordering index (passed from Python via device config).
fn instr_sort_key(instr: &Instruction, ordering: &[u32]) -> u32 {
    if let Instruction::Call { callee, args, .. } = instr {
        if callee.starts_with("__quantum__qis__") {
            // Find the first qubit argument and use its ordering.
            for (ty, op) in args {
                if matches!(ty, Type::NamedPtr(n) if n == "Qubit") {
                    if let Some(id) = extract_id(op) {
                        return ordering.get(id as usize).copied().unwrap_or(0);
                    }
                }
            }
        }
    }
    0
}

fn reorder_block_instructions(instrs: Vec<Instruction>, ordering: &[u32]) -> Vec<Instruction> {
    // Separate instructions into steps, preserving dependencies.
    let mut steps: Vec<Vec<Instruction>> = Vec::new();
    let mut vals_per_step: Vec<FxHashSet<ValKey>> = Vec::new();
    let mut results_per_step: Vec<FxHashSet<ValKey>> = Vec::new();
    let mut outputs: Vec<Instruction> = Vec::new();
    let mut terminator: Option<Instruction> = None;

    let mut to_process: Vec<Instruction> = Vec::with_capacity(instrs.len());
    for instr in instrs {
        // Check if this is a terminator.
        if matches!(
            instr,
            Instruction::Ret(_)
                | Instruction::Jump { .. }
                | Instruction::Br { .. }
                | Instruction::Switch { .. }
                | Instruction::Unreachable
        ) {
            terminator = Some(instr);
            continue;
        }
        if is_output_recording(&instr) {
            outputs.push(instr);
            continue;
        }
        to_process.push(instr);
    }

    for instr in to_process {
        let (used_vals, used_results) = get_used_values(&instr);

        // Find the last step this instruction depends on.
        let mut last_dep = steps.len() as i64 - 1;
        while last_dep >= 0 {
            let idx = last_dep as usize;
            if uses_any_value(&used_vals, &vals_per_step[idx])
                || uses_any_value(&used_results, &results_per_step[idx])
            {
                break;
            }
            last_dep -= 1;
        }

        // For Call instructions, push forward past steps with different callees
        // to group same-gate operations together.
        if let Instruction::Call { callee, .. } = &instr {
            while (last_dep as usize) < steps.len().saturating_sub(1) {
                let next_idx = (last_dep + 1) as usize;
                if let Some(first) = steps[next_idx].first() {
                    if let Instruction::Call {
                        callee: other_callee,
                        ..
                    } = first
                    {
                        if callee != other_callee {
                            last_dep += 1;
                            continue;
                        }
                    }
                }
                break;
            }
        }

        let target_step = (last_dep + 1) as usize;
        if target_step >= steps.len() {
            steps.push(vec![instr]);
            vals_per_step.push(used_vals.into_iter().collect());
            results_per_step.push(used_results.into_iter().collect());
        } else {
            steps[target_step].push(instr);
            vals_per_step[target_step].extend(used_vals);
            results_per_step[target_step].extend(used_results);
        }
    }

    // Flatten steps, sorting within each step by qubit ordering.
    let mut result = Vec::new();
    for step in &mut steps {
        step.sort_by_key(|i| instr_sort_key(i, ordering));
        result.extend(step.drain(..));
    }
    result.extend(outputs);
    if let Some(term) = terminator {
        result.push(term);
    }
    result
}

fn reorder_module(module: &mut Module, ordering: &[u32]) {
    for func in &mut module.functions {
        if func.is_declaration {
            continue;
        }
        for bb in &mut func.basic_blocks {
            let instrs = std::mem::take(&mut bb.instructions);
            bb.instructions = reorder_block_instructions(instrs, ordering);
        }
    }
}

/// Reorder instructions within each basic block to group contiguous
/// sequences of the same gate on different qubits.
///
/// `ordering` is a list of u32 values mapping qubit ID → device ordering
/// index, passed from the Python `Device.get_ordering()` method.
#[pyfunction]
pub fn atom_reorder(ir: &str, ordering: Vec<u32>) -> PyResult<String> {
    let mut module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("parse error: {e}")))?;
    reorder_module(&mut module, &ordering);
    Ok(write_module_to_string(&module))
}
