// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Neutral-atom gate decomposition pass.
//!
//! Rust equivalent of `_decomp.py`.  Decomposes multi-qubit gates to CZ
//! primitives, single rotations to Rz, single-qubit gates to Rz+SX,
//! Rz Clifford angles to named gates, and Reset to MResetZ.

use pyo3::{PyResult, exceptions::PyValueError, pyfunction};
use qsc_llvm::{
    model::Type,
    model::{Function, Instruction, Module, Operand, Param},
    parse_module,
    qir::{self, double_op, qis, qubit_op, result_op, rt, void_call},
    write_module_to_string,
};
use std::f64::consts::PI;

use super::atom_utils::{TOLERANCE, extract_float, extract_id};

/// Shorthand: `call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 q to %Qubit*))`.
fn h(q: u32) -> Instruction {
    void_call(qis::H, vec![qubit_op(q)])
}

fn s(q: u32) -> Instruction {
    void_call(qis::S, vec![qubit_op(q)])
}

fn s_adj(q: u32) -> Instruction {
    void_call(qis::S_ADJ, vec![qubit_op(q)])
}

fn t(q: u32) -> Instruction {
    void_call(qis::T, vec![qubit_op(q)])
}

fn t_adj(q: u32) -> Instruction {
    void_call(qis::T_ADJ, vec![qubit_op(q)])
}

fn rz(angle: f64, q: u32) -> Instruction {
    void_call(qis::RZ, vec![double_op(angle), qubit_op(q)])
}

fn rz_passthrough(angle_op: (Type, Operand), q: u32) -> Instruction {
    void_call(qis::RZ, vec![angle_op, qubit_op(q)])
}

fn cz(q1: u32, q2: u32) -> Instruction {
    void_call(qis::CZ, vec![qubit_op(q1), qubit_op(q2)])
}

fn sx(q: u32) -> Instruction {
    void_call(qis::SX, vec![qubit_op(q)])
}

fn z(q: u32) -> Instruction {
    void_call(qis::Z, vec![qubit_op(q)])
}

fn mresetz(q: u32, r: u32) -> Instruction {
    void_call(qis::MRESETZ, vec![qubit_op(q), result_op(r)])
}

/// Ensure a declaration for given function name exists in the module.
/// If missing, add a void(...) declaration with the appropriate signature.
pub(crate) fn ensure_declaration(module: &mut Module, name: &str) {
    if module.functions.iter().any(|f| f.name == name) {
        return;
    }
    let (ret, params) = match name {
        qis::H | qis::S | qis::S_ADJ | qis::T | qis::T_ADJ | qis::SX | qis::Z => (
            Type::Void,
            vec![Type::NamedPtr(qir::QUBIT_TYPE_NAME.to_string())],
        ),
        qis::RZ => (
            Type::Void,
            vec![
                Type::Double,
                Type::NamedPtr(qir::QUBIT_TYPE_NAME.to_string()),
            ],
        ),
        qis::CZ => (
            Type::Void,
            vec![
                Type::NamedPtr(qir::QUBIT_TYPE_NAME.to_string()),
                Type::NamedPtr(qir::QUBIT_TYPE_NAME.to_string()),
            ],
        ),
        qis::MRESETZ => (
            Type::Void,
            vec![
                Type::NamedPtr(qir::QUBIT_TYPE_NAME.to_string()),
                Type::NamedPtr(qir::RESULT_TYPE_NAME.to_string()),
            ],
        ),
        rt::BEGIN_PARALLEL | rt::END_PARALLEL => (Type::Void, Vec::new()),
        qis::MOVE => (
            Type::Void,
            vec![
                Type::NamedPtr(qir::QUBIT_TYPE_NAME.to_string()),
                Type::Integer(64),
                Type::Integer(64),
            ],
        ),
        _ => (Type::Void, Vec::new()),
    };
    module.functions.push(Function {
        name: name.to_string(),
        return_type: ret,
        params: params
            .into_iter()
            .map(|ty| Param { ty, name: None })
            .collect(),
        is_declaration: true,
        attribute_group_refs: Vec::new(),
        basic_blocks: Vec::new(),
    });
}

fn decompose_multi_qubit_to_cz(module: &mut Module) {
    let needed = [
        qis::H,
        qis::S,
        qis::S_ADJ,
        qis::T,
        qis::T_ADJ,
        qis::RZ,
        qis::CZ,
    ];
    for n in &needed {
        ensure_declaration(module, n);
    }

    for func in &mut module.functions {
        if func.is_declaration {
            continue;
        }
        for bb in &mut func.basic_blocks {
            let old_instrs = std::mem::take(&mut bb.instructions);
            let mut new_instrs = Vec::with_capacity(old_instrs.len());
            for instr in old_instrs {
                if let Instruction::Call { callee, args, .. } = &instr {
                    match callee.as_str() {
                        name if name == qis::CCX => {
                            // CCX(ctrl1, ctrl2, target)
                            if let (Some(c1), Some(c2), Some(tgt)) = (
                                extract_id(&args[0].1),
                                extract_id(&args[1].1),
                                extract_id(&args[2].1),
                            ) {
                                new_instrs.extend([
                                    h(tgt),
                                    t_adj(c1),
                                    t_adj(c2),
                                    h(c1),
                                    cz(tgt, c1),
                                    h(c1),
                                    t(c1),
                                    h(tgt),
                                    cz(c2, tgt),
                                    h(tgt),
                                    h(c1),
                                    cz(c2, c1),
                                    h(c1),
                                    t(tgt),
                                    t_adj(c1),
                                    h(tgt),
                                    cz(c2, tgt),
                                    h(tgt),
                                    h(c1),
                                    cz(tgt, c1),
                                    h(c1),
                                    t_adj(tgt),
                                    t(c1),
                                    h(c1),
                                    cz(c2, c1),
                                    h(c1),
                                    h(tgt),
                                ]);
                                continue;
                            }
                        }
                        name if name == qis::CX => {
                            if let (Some(ctrl), Some(tgt)) =
                                (extract_id(&args[0].1), extract_id(&args[1].1))
                            {
                                new_instrs.extend([h(tgt), cz(ctrl, tgt), h(tgt)]);
                                continue;
                            }
                        }
                        name if name == qis::CY => {
                            if let (Some(ctrl), Some(tgt)) =
                                (extract_id(&args[0].1), extract_id(&args[1].1))
                            {
                                new_instrs.extend([
                                    s_adj(tgt),
                                    h(tgt),
                                    cz(ctrl, tgt),
                                    h(tgt),
                                    s(tgt),
                                ]);
                                continue;
                            }
                        }
                        name if name == qis::RXX => {
                            // rxx(angle, q1, q2)
                            if let (Some(q1), Some(q2)) =
                                (extract_id(&args[1].1), extract_id(&args[2].1))
                            {
                                let angle_op = args[0].clone();
                                new_instrs.extend([
                                    h(q2),
                                    cz(q2, q1),
                                    h(q1),
                                    rz_passthrough(angle_op, q1),
                                    h(q1),
                                    cz(q2, q1),
                                    h(q2),
                                ]);
                                continue;
                            }
                        }
                        name if name == qis::RYY => {
                            if let (Some(q1), Some(q2)) =
                                (extract_id(&args[1].1), extract_id(&args[2].1))
                            {
                                let angle_op = args[0].clone();
                                new_instrs.extend([
                                    s_adj(q1),
                                    s_adj(q2),
                                    h(q2),
                                    cz(q2, q1),
                                    h(q1),
                                    rz_passthrough(angle_op, q1),
                                    h(q1),
                                    cz(q2, q1),
                                    h(q2),
                                    s(q2),
                                    s(q1),
                                ]);
                                continue;
                            }
                        }
                        name if name == qis::RZZ => {
                            if let (Some(q1), Some(q2)) =
                                (extract_id(&args[1].1), extract_id(&args[2].1))
                            {
                                let angle_op = args[0].clone();
                                new_instrs.extend([
                                    h(q1),
                                    cz(q2, q1),
                                    h(q1),
                                    rz_passthrough(angle_op, q1),
                                    h(q1),
                                    cz(q2, q1),
                                    h(q1),
                                ]);
                                continue;
                            }
                        }
                        name if name == qis::SWAP => {
                            if let (Some(q1), Some(q2)) =
                                (extract_id(&args[0].1), extract_id(&args[1].1))
                            {
                                new_instrs.extend([
                                    h(q2),
                                    cz(q1, q2),
                                    h(q2),
                                    h(q1),
                                    cz(q2, q1),
                                    h(q1),
                                    h(q2),
                                    cz(q1, q2),
                                    h(q2),
                                ]);
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
                // Instruction not decomposed — keep as-is.
                new_instrs.push(instr);
            }
            bb.instructions = new_instrs;
        }
    }
}

fn decompose_single_rotation_to_rz(module: &mut Module) {
    let needed = [qis::H, qis::S, qis::S_ADJ, qis::RZ];
    for n in &needed {
        ensure_declaration(module, n);
    }

    for func in &mut module.functions {
        if func.is_declaration {
            continue;
        }
        for bb in &mut func.basic_blocks {
            let old_instrs = std::mem::take(&mut bb.instructions);
            let mut new_instrs = Vec::with_capacity(old_instrs.len());
            for instr in old_instrs {
                if let Instruction::Call { callee, args, .. } = &instr {
                    match callee.as_str() {
                        name if name == qis::RX => {
                            // rx(angle, target)
                            if let Some(tgt) = extract_id(&args[1].1) {
                                let angle_op = args[0].clone();
                                new_instrs.extend([h(tgt), rz_passthrough(angle_op, tgt), h(tgt)]);
                                continue;
                            }
                        }
                        name if name == qis::RY => {
                            if let Some(tgt) = extract_id(&args[1].1) {
                                let angle_op = args[0].clone();
                                new_instrs.extend([
                                    s_adj(tgt),
                                    h(tgt),
                                    rz_passthrough(angle_op, tgt),
                                    h(tgt),
                                    s(tgt),
                                ]);
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
                new_instrs.push(instr);
            }
            bb.instructions = new_instrs;
        }
    }
}

fn decompose_single_qubit_to_rz_sx(module: &mut Module) {
    let needed = [qis::SX, qis::RZ];
    for n in &needed {
        ensure_declaration(module, n);
    }

    for func in &mut module.functions {
        if func.is_declaration {
            continue;
        }
        for bb in &mut func.basic_blocks {
            let old_instrs = std::mem::take(&mut bb.instructions);
            let mut new_instrs = Vec::with_capacity(old_instrs.len());
            for instr in old_instrs {
                if let Instruction::Call { callee, args, .. } = &instr {
                    if let Some(first_arg) = args.first() {
                        if let Some(tgt) = extract_id(&first_arg.1) {
                            match callee.as_str() {
                                name if name == qis::H => {
                                    new_instrs.extend([
                                        rz(PI / 2.0, tgt),
                                        sx(tgt),
                                        rz(PI / 2.0, tgt),
                                    ]);
                                    continue;
                                }
                                name if name == qis::S => {
                                    new_instrs.push(rz(PI / 2.0, tgt));
                                    continue;
                                }
                                name if name == qis::S_ADJ => {
                                    new_instrs.push(rz(-PI / 2.0, tgt));
                                    continue;
                                }
                                name if name == qis::T => {
                                    new_instrs.push(rz(PI / 4.0, tgt));
                                    continue;
                                }
                                name if name == qis::T_ADJ => {
                                    new_instrs.push(rz(-PI / 4.0, tgt));
                                    continue;
                                }
                                name if name == qis::X => {
                                    new_instrs.extend([sx(tgt), sx(tgt)]);
                                    continue;
                                }
                                name if name == qis::Y => {
                                    new_instrs.extend([sx(tgt), sx(tgt), rz(PI, tgt)]);
                                    continue;
                                }
                                name if name == qis::Z => {
                                    new_instrs.push(rz(PI, tgt));
                                    continue;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                new_instrs.push(instr);
            }
            bb.instructions = new_instrs;
        }
    }
}

fn decompose_rz_angles_to_clifford(module: &mut Module) {
    let needed = [qis::S, qis::S_ADJ, qis::Z];
    for n in &needed {
        ensure_declaration(module, n);
    }

    let three_pi_over_2 = 3.0 * PI / 2.0;
    let pi_over_2 = PI / 2.0;
    let two_pi = 2.0 * PI;

    for func in &mut module.functions {
        if func.is_declaration {
            continue;
        }
        for bb in &mut func.basic_blocks {
            let old_instrs = std::mem::take(&mut bb.instructions);
            let mut new_instrs = Vec::with_capacity(old_instrs.len());
            for instr in old_instrs {
                if let Instruction::Call { callee, args, .. } = &instr {
                    if callee == qis::RZ {
                        if let (Some(angle), Some(tgt)) =
                            (extract_float(&args[0].1), extract_id(&args[1].1))
                        {
                            if (angle - three_pi_over_2).abs() < TOLERANCE
                                || (angle + pi_over_2).abs() < TOLERANCE
                            {
                                new_instrs.push(s_adj(tgt));
                            } else if (angle - PI).abs() < TOLERANCE
                                || (angle + PI).abs() < TOLERANCE
                            {
                                new_instrs.push(z(tgt));
                            } else if (angle - pi_over_2).abs() < TOLERANCE
                                || (angle + three_pi_over_2).abs() < TOLERANCE
                            {
                                new_instrs.push(s(tgt));
                            } else if angle.abs() < TOLERANCE
                                || (angle - two_pi).abs() < TOLERANCE
                                || (angle + two_pi).abs() < TOLERANCE
                            {
                                // Identity — drop.
                            } else {
                                // Non-Clifford angle — keep instruction as is.
                                new_instrs.push(instr);
                            }
                            continue;
                        }
                    }
                }
                new_instrs.push(instr);
            }
            bb.instructions = new_instrs;
        }
    }
}

fn replace_reset_with_mresetz(module: &mut Module) {
    ensure_declaration(module, qis::MRESETZ);

    for func in &mut module.functions {
        if func.is_declaration {
            continue;
        }
        // Find the maximum result id used in this function so we can allocate new ones.
        let mut max_result_id: u32 = 0;
        for bb in &func.basic_blocks {
            for instr in &bb.instructions {
                if let Instruction::Call { args, .. } = &instr {
                    for (ty, op) in args {
                        if matches!(ty, Type::NamedPtr(n) if n == qir::RESULT_TYPE_NAME) {
                            if let Some(id) = extract_id(op) {
                                if id >= max_result_id {
                                    max_result_id = id + 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        // Also check entry_point attribute for required_num_results.
        let mut next_result_id = max_result_id;

        for bb in &mut func.basic_blocks {
            let old_instrs = std::mem::take(&mut bb.instructions);
            let mut new_instrs = Vec::with_capacity(old_instrs.len());
            for instr in old_instrs {
                if let Instruction::Call { callee, args, .. } = &instr {
                    if callee == qis::RESET {
                        if let Some(q) = extract_id(&args[0].1) {
                            new_instrs.push(mresetz(q, next_result_id));
                            next_result_id += 1;
                            continue;
                        }
                    }
                }
                new_instrs.push(instr);
            }
            bb.instructions = new_instrs;
        }
    }
}

/// Decompose multi-qubit gates to CZ, single rotations to Rz, and
/// single-qubit gates to Rz+SX. Also replace Reset with MResetZ.
///
/// This chains the four decomposition sub-passes that `_decomp.py` exposes.
/// The caller (`__init__.py`) invokes individual decompositions via the
/// Python pipeline; this function exposes them as a single native function
/// or individually.
#[pyfunction]
pub fn atom_decompose_multi_qubit_to_cz(ir: &str) -> PyResult<String> {
    let mut module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("parse error: {e}")))?;
    decompose_multi_qubit_to_cz(&mut module);
    Ok(write_module_to_string(&module))
}

#[pyfunction]
pub fn atom_decompose_single_rotation_to_rz(ir: &str) -> PyResult<String> {
    let mut module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("parse error: {e}")))?;
    decompose_single_rotation_to_rz(&mut module);
    Ok(write_module_to_string(&module))
}

#[pyfunction]
pub fn atom_decompose_single_qubit_to_rz_sx(ir: &str) -> PyResult<String> {
    let mut module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("parse error: {e}")))?;
    decompose_single_qubit_to_rz_sx(&mut module);
    Ok(write_module_to_string(&module))
}

#[pyfunction]
pub fn atom_decompose_rz_to_clifford(ir: &str) -> PyResult<String> {
    let mut module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("parse error: {e}")))?;
    decompose_rz_angles_to_clifford(&mut module);
    Ok(write_module_to_string(&module))
}

#[pyfunction]
pub fn atom_replace_reset_with_mresetz(ir: &str) -> PyResult<String> {
    let mut module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("parse error: {e}")))?;
    replace_reset_with_mresetz(&mut module);
    Ok(write_module_to_string(&module))
}
