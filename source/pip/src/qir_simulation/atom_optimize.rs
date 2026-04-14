// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Neutral-atom single-qubit gate optimization pass.
//!
//! Rust equivalent of `_optimize.py`.  Combines adjacent Rz gates,
//! removes identity rotations, cancels adjoint gate pairs, replaces
//! h-s-h → sx, and converts trailing m+reset → mresetz.

use pyo3::{PyResult, exceptions::PyValueError, pyfunction};
use qsc_llvm::{
    model::Type,
    model::{Instruction, Module, Operand},
    parse_module,
    qir::{qis, qubit_op, rt, void_call},
    write_module_to_string,
};
use rustc_hash::FxHashMap;
use std::f64::consts::PI;

use super::atom_utils::{TOLERANCE, extract_float, extract_id};

/// Classify a call instruction as a named gate on a single qubit, returning
/// `(gate_name, qubit_id, is_rotation)`.
fn classify_single_qubit_gate(
    callee: &str,
    args: &[(Type, Operand)],
) -> Option<(String, u32, bool)> {
    if !callee.starts_with("__quantum__qis__") {
        return None;
    }
    let parts: Vec<&str> = callee.split("__").collect();
    if parts.len() < 5 {
        return None;
    }
    let gate_name = parts[3];
    let suffix = if parts.len() > 4 { parts[4] } else { "" };
    let full_name = if suffix == "adj" {
        format!("{gate_name}_adj")
    } else {
        gate_name.to_string()
    };
    // Single-qubit gates: the qubit is the last arg.
    let qubit_arg = args.last()?;
    if !matches!(&qubit_arg.0, Type::NamedPtr(n) if n == "Qubit") {
        return None;
    }
    let q = extract_id(&qubit_arg.1)?;
    let is_rotation = matches!(gate_name, "rx" | "ry" | "rz");
    Some((full_name, q, is_rotation))
}

/// Return the "adjoint name" for named gates that cancel with themselves/adjoints.
fn adjoint_of(name: &str) -> &str {
    match name {
        "h" => "h",
        "s" => "s_adj",
        "s_adj" => "s",
        "t" => "t_adj",
        "t_adj" => "t",
        "x" => "x",
        "y" => "y",
        "z" => "z",
        _ => "",
    }
}

/// A tracked pending operation on a single qubit.
#[derive(Clone)]
struct PendingOp {
    instr: Instruction,
    gate: String,
}

fn optimize_single_qubit_gates(module: &mut Module) {
    // Ensure SX and MResetZ declarations exist.
    super::atom_decomp::ensure_declaration(module, qis::SX);
    super::atom_decomp::ensure_declaration(module, qis::MRESETZ);

    for func in &mut module.functions {
        if func.is_declaration {
            continue;
        }
        let mut last_meas: FxHashMap<u32, (Instruction, (Type, Operand), (Type, Operand))> =
            FxHashMap::default();

        for bb in &mut func.basic_blocks {
            let mut qubit_ops: FxHashMap<u32, Vec<PendingOp>> = FxHashMap::default();
            let mut used_qubits: rustc_hash::FxHashSet<u32> = rustc_hash::FxHashSet::default();
            let mut local_last_meas: FxHashMap<
                u32,
                (Instruction, (Type, Operand), (Type, Operand)),
            > = FxHashMap::default();

            let old_instrs = std::mem::take(&mut bb.instructions);
            let mut new_instrs: Vec<Instruction> = Vec::with_capacity(old_instrs.len());

            for instr in old_instrs {
                if let Instruction::Call { callee, args, .. } = &instr {
                    // Special cases.
                    match callee.as_str() {
                        s if s == qis::SX || s == qis::MOVE => {
                            // Drop tracked ops for involved qubits.
                            if let Some(q) = extract_id(&args[0].1) {
                                qubit_ops.remove(&q);
                                local_last_meas.remove(&q);
                                used_qubits.insert(q);
                            }
                            new_instrs.push(instr);
                            continue;
                        }
                        s if s == qis::BARRIER => {
                            qubit_ops.clear();
                            local_last_meas.clear();
                            new_instrs.push(instr);
                            continue;
                        }
                        _ => {}
                    }

                    // Measurement: m, mz, mresetz.
                    if callee == qis::M || callee == qis::MZ || callee == qis::MRESETZ {
                        if let Some(q) = extract_id(&args[0].1) {
                            qubit_ops.remove(&q);
                            used_qubits.insert(q);
                            local_last_meas
                                .insert(q, (instr.clone(), args[0].clone(), args[1].clone()));
                        }
                        new_instrs.push(instr);
                        continue;
                    }

                    // Reset.
                    if callee == qis::RESET
                        && let Some(q) = extract_id(&args[0].1)
                    {
                        if let Some((meas_instr, target_op, result_op_val)) =
                            local_last_meas.remove(&q)
                        {
                            // Replace the last measurement with mresetz.
                            // First, find and replace the measurement instruction in new_instrs.
                            if let Some(pos) = new_instrs.iter().position(|i| *i == meas_instr) {
                                new_instrs[pos] = void_call(
                                    qis::MRESETZ,
                                    vec![target_op.clone(), result_op_val.clone()],
                                );
                                let new_mresetz = new_instrs[pos].clone();
                                local_last_meas.insert(q, (new_mresetz, target_op, result_op_val));
                            }
                            // Drop the reset.
                            continue;
                        } else if !used_qubits.contains(&q) {
                            // Qubit was never used; drop the reset.
                            continue;
                        } else if qubit_ops
                            .get(&q)
                            .is_some_and(|ops| ops.last().is_some_and(|op| op.gate == "reset"))
                        {
                            // Last op was also a reset; drop duplicate.
                            continue;
                        } else {
                            qubit_ops.remove(&q);
                            used_qubits.insert(q);
                            let ops = qubit_ops.entry(q).or_default();
                            ops.push(PendingOp {
                                instr: instr.clone(),
                                gate: "reset".to_string(),
                            });
                            new_instrs.push(instr);
                            continue;
                        }
                    }

                    // Two-qubit gates: drop tracked ops for both qubits.
                    if callee.starts_with("__quantum__qis__")
                        && args.len() >= 2
                        && matches!(&args[0].0, Type::NamedPtr(n) if n == "Qubit")
                        && matches!(&args[1].0, Type::NamedPtr(n) if n == "Qubit")
                    {
                        for a in args {
                            if let Some(q) = extract_id(&a.1) {
                                qubit_ops.remove(&q);
                                local_last_meas.remove(&q);
                                used_qubits.insert(q);
                            }
                        }
                        new_instrs.push(instr);
                        continue;
                    }

                    // Try to classify as a single qubit gate.
                    if let Some((gate, q, is_rotation)) = classify_single_qubit_gate(callee, args) {
                        if is_rotation {
                            // Rotation folding.
                            let angle = extract_float(&args[0].1);
                            if let Some(angle_val) = angle {
                                if let Some(ops) = qubit_ops.get_mut(&q) {
                                    if let Some(last) = ops.last() {
                                        if last.gate == gate {
                                            // Same rotation type — try to fold.
                                            if let Instruction::Call {
                                                args: prev_args, ..
                                            } = &last.instr
                                            {
                                                if let Some(prev_angle) =
                                                    extract_float(&prev_args[0].1)
                                                {
                                                    let mut new_angle = angle_val + prev_angle;
                                                    let sign =
                                                        if new_angle < 0.0 { -1.0 } else { 1.0 };
                                                    let mut abs_angle = new_angle.abs();
                                                    while abs_angle > 2.0 * PI {
                                                        abs_angle -= 2.0 * PI;
                                                    }
                                                    new_angle = sign * abs_angle;

                                                    // Remove the previous instruction from output.
                                                    let prev_instr =
                                                        ops.pop().expect("just checked").instr;
                                                    if let Some(pos) = new_instrs
                                                        .iter()
                                                        .rposition(|i| *i == prev_instr)
                                                    {
                                                        new_instrs.remove(pos);
                                                    }

                                                    if new_angle.abs() > TOLERANCE
                                                        && (new_angle.abs() - 2.0 * PI).abs()
                                                            > TOLERANCE
                                                    {
                                                        // Insert folded rotation.
                                                        let folded = void_call(
                                                            callee,
                                                            vec![
                                                                (
                                                                    Type::Double,
                                                                    Operand::float_const(
                                                                        Type::Double,
                                                                        new_angle,
                                                                    ),
                                                                ),
                                                                qubit_op(q),
                                                            ],
                                                        );
                                                        ops.push(PendingOp {
                                                            instr: folded.clone(),
                                                            gate: gate.clone(),
                                                        });
                                                        used_qubits.insert(q);
                                                        local_last_meas.remove(&q);
                                                        new_instrs.push(folded);
                                                    } else if ops.is_empty() {
                                                        qubit_ops.remove(&q);
                                                    }
                                                    continue;
                                                }
                                            }
                                        }
                                    }
                                }
                                // Can't fold — just add.
                                let ops = qubit_ops.entry(q).or_default();
                                ops.push(PendingOp {
                                    instr: instr.clone(),
                                    gate,
                                });
                                used_qubits.insert(q);
                                local_last_meas.remove(&q);
                                new_instrs.push(instr);
                                continue;
                            }
                            // Non-constant angle — keep.
                            let ops = qubit_ops.entry(q).or_default();
                            ops.push(PendingOp {
                                instr: instr.clone(),
                                gate,
                            });
                            used_qubits.insert(q);
                            local_last_meas.remove(&q);
                            new_instrs.push(instr);
                            continue;
                        }

                        // Non-rotation single qubit gate: check for cancellation / h-s-h → sx.
                        let adj = adjoint_of(&gate);
                        if let Some(ops) = qubit_ops.get_mut(&q) {
                            if let Some(last) = ops.last() {
                                if last.gate == adj {
                                    // Cancel pair.
                                    let prev_instr = ops.pop().expect("just checked").instr;
                                    if let Some(pos) =
                                        new_instrs.iter().rposition(|i| *i == prev_instr)
                                    {
                                        new_instrs.remove(pos);
                                    }
                                    if ops.is_empty() {
                                        qubit_ops.remove(&q);
                                    }
                                    continue;
                                }
                                // h-s-h → sx pattern.
                                if ops.len() >= 2
                                    && gate == "h"
                                    && last.gate == "s"
                                    && ops[ops.len() - 2].gate == "h"
                                {
                                    let s_instr = ops.pop().expect("just checked").instr;
                                    let h_instr = ops.pop().expect("just checked").instr;
                                    // Remove the s and first h from output.
                                    if let Some(pos) =
                                        new_instrs.iter().rposition(|i| *i == s_instr)
                                    {
                                        new_instrs.remove(pos);
                                    }
                                    if let Some(pos) =
                                        new_instrs.iter().rposition(|i| *i == h_instr)
                                    {
                                        new_instrs.remove(pos);
                                    }
                                    // Insert sx instead of the second h.
                                    let sx_instr = void_call(qis::SX, vec![qubit_op(q)]);
                                    new_instrs.push(sx_instr);
                                    // Don't track further — drop ops for this qubit.
                                    if ops.is_empty() {
                                        qubit_ops.remove(&q);
                                    }
                                    continue;
                                }
                            }
                            // No cancellation — append.
                            ops.push(PendingOp {
                                instr: instr.clone(),
                                gate,
                            });
                            used_qubits.insert(q);
                            local_last_meas.remove(&q);
                            new_instrs.push(instr);
                            continue;
                        }
                        // First operation on this qubit.
                        qubit_ops.insert(
                            q,
                            vec![PendingOp {
                                instr: instr.clone(),
                                gate,
                            }],
                        );
                        used_qubits.insert(q);
                        local_last_meas.remove(&q);
                        new_instrs.push(instr);
                        continue;
                    }
                }
                // Non-call instruction — keep.
                new_instrs.push(instr);
            }
            bb.instructions = new_instrs;
            // Propagate local_last_meas to function-level last_meas.
            for (k, v) in local_last_meas {
                last_meas.insert(k, v);
            }
        }

        // Post-function: convert trailing measurements to mresetz.
        for bb in &mut func.basic_blocks {
            for (_, (meas_instr, target, res)) in &last_meas {
                if let Some(pos) = bb.instructions.iter().position(|i| i == meas_instr) {
                    bb.instructions[pos] =
                        void_call(qis::MRESETZ, vec![target.clone(), res.clone()]);
                }
            }
            // Remove trailing resets.
            for (_q, ops_vec) in std::iter::empty::<(u32, Vec<PendingOp>)>() {
                // This is handled inline above.
                let _ = ops_vec;
            }
        }
    }
}

fn prune_unused_functions(module: &mut Module) {
    // Collect names of functions called from entry points.
    let mut called: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();

    // Also track entry points.
    let mut entry_points: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
    for func in &module.functions {
        if !func.is_declaration && !func.basic_blocks.is_empty() {
            // Check if this function has an entry_point attribute.
            // For simplicity, treat all non-declaration functions as potential entry points.
            entry_points.insert(func.name.clone());
        }
    }

    // Collect all function calls.
    for func in &module.functions {
        if func.is_declaration {
            continue;
        }
        for bb in &func.basic_blocks {
            for instr in &bb.instructions {
                if let Instruction::Call { callee, .. } = instr {
                    called.insert(callee.clone());
                    // Also remove __quantum__rt__initialize and __quantum__qis__barrier__body calls.
                }
            }
        }
    }

    // Remove instructions that call init/barrier.
    for func in &mut module.functions {
        if func.is_declaration {
            continue;
        }
        for bb in &mut func.basic_blocks {
            bb.instructions.retain(|instr| {
                if let Instruction::Call { callee, .. } = instr {
                    callee != rt::INITIALIZE && callee != qis::BARRIER
                } else {
                    true
                }
            });
        }
    }

    // Prune non-entry functions that are never called.
    module.functions.retain(|f| {
        if f.is_declaration {
            // Keep declarations that are called.
            called.contains(&f.name)
        } else {
            // Keep entry points always.
            true
        }
    });
}

/// Optimize single-qubit gate sequences: cancel adjoints, fold rotations,
/// replace h-s-h with sx, convert m+reset to mresetz.
#[pyfunction]
pub fn atom_optimize_single_qubit_gates(ir: &str) -> PyResult<String> {
    let mut module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("parse error: {e}")))?;
    optimize_single_qubit_gates(&mut module);
    Ok(write_module_to_string(&module))
}

/// Remove unused function declarations and calls to
/// `__quantum__rt__initialize` and `__quantum__qis__barrier__body`.
#[pyfunction]
pub fn atom_prune_unused_functions(ir: &str) -> PyResult<String> {
    let mut module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("parse error: {e}")))?;
    prune_unused_functions(&mut module);
    Ok(write_module_to_string(&module))
}
