// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Utility helpers for inspecting neutral-atom QIR instructions.
//!
//! Rust equivalent of `_utils.py`.  These are used internally by
//! `atom_validate` and `atom_trace`; they are *not* exposed to Python.

use qsc_llvm::{model::Operand, model::Type, qir};

pub(crate) const TOLERANCE: f64 = 1.192_092_9e-7; // Machine epsilon for f32

/// Description of a `__quantum__qis__*` gate call.
pub(crate) struct QisGate {
    pub gate: String,
    pub qubit_args: Vec<u32>,
    pub result_args: Vec<u32>,
    pub other_args: Vec<OtherArg>,
}

/// Catch-all for non-qubit, non-result arguments (angles, ints, …).
#[derive(Clone)]
pub(crate) enum OtherArg {
    Float(f64),
    Int(i64),
}

/// If the callee name matches `__quantum__qis__<gate>__<suffix>`, return
/// a [`QisGate`] with qubit/result IDs extracted from `args`.
pub(crate) fn as_qis_gate(callee: &str, args: &[(Type, Operand)]) -> Option<QisGate> {
    if !callee.starts_with("__quantum__qis__") {
        return None;
    }
    let parts: Vec<&str> = callee.split("__").collect();
    // parts: ["", "", "quantum", "", "qis", "", "<gate>", "", "<suffix>", ...]
    if parts.len() < 5 {
        return None;
    }
    let gate_name = parts[3];
    let suffix = if parts.len() > 4 { parts[4] } else { "" };
    let gate = if suffix == "adj" {
        format!("{gate_name}_adj")
    } else {
        gate_name.to_string()
    };

    let mut qubit_args = Vec::new();
    let mut result_args = Vec::new();
    let mut other_args = Vec::new();

    for (ty, operand) in args {
        if is_qubit_type(ty) {
            if let Some(id) = extract_id(operand) {
                qubit_args.push(id);
            }
        } else if is_result_type(ty) {
            if let Some(id) = extract_id(operand) {
                result_args.push(id);
            }
        } else {
            match operand {
                Operand::FloatConst(_, v) => other_args.push(OtherArg::Float(*v)),
                Operand::IntConst(_, v) => other_args.push(OtherArg::Int(*v)),
                _ => {}
            }
        }
    }

    Some(QisGate {
        gate,
        qubit_args,
        result_args,
        other_args,
    })
}

/// Check whether a type is `%Qubit*` (pointer to opaque `Qubit` struct).
fn is_qubit_type(ty: &Type) -> bool {
    matches!(ty, Type::NamedPtr(n) if n == qir::QUBIT_TYPE_NAME)
}

/// Check whether a type is `%Result*` (pointer to opaque `Result` struct).
fn is_result_type(ty: &Type) -> bool {
    matches!(ty, Type::NamedPtr(n) if n == qir::RESULT_TYPE_NAME)
}

/// Extract an integer ID from an `inttoptr` or `null` operand.
/// `NullPtr` is treated as ID 0 (pyqir normalizes `inttoptr(i64 0)` to `null`).
pub(crate) fn extract_id(operand: &Operand) -> Option<u32> {
    match operand {
        Operand::IntToPtr(val, _) => u32::try_from(*val).ok(),
        Operand::NullPtr => Some(0),
        _ => None,
    }
}

/// Extract a float constant from an operand.
pub(crate) fn extract_float(operand: &Operand) -> Option<f64> {
    match operand {
        Operand::FloatConst(_, val) => Some(*val),
        _ => None,
    }
}

/// Check if a callee is a measurement gate.
pub(crate) fn is_measurement(callee: &str) -> bool {
    matches!(callee, s if s == qir::qis::MRESETZ || s == qir::qis::M || s == qir::qis::MZ)
}

/// Check if a callee is a quantum instruction (starts with `__quantum__qis__`).
pub(crate) fn is_qubit_instruction(callee: &str) -> bool {
    callee.starts_with("__quantum__qis__")
}
