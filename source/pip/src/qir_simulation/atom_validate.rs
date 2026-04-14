// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Neutral-atom QIR validation passes (read-only).
//!
//! Rust equivalent of `_validate.py`.  Two checks:
//!
//! 1. **`validate_allowed_intrinsics`** – every non-entry-point function
//!    must be in the allow-list.
//! 2. **`validate_no_conditional_branches`** – the entry point must not
//!    contain any conditional `br` instructions.

use pyo3::{PyResult, exceptions::PyValueError, pyfunction};
use qsc_llvm::{
    model::{Instruction, Module},
    parse_module,
    qir::{find_entry_point, qis, rt},
};

/// Allowed function names that are not the entry point.
const ALLOWED_INTRINSICS: &[&str] = &[
    rt::BEGIN_PARALLEL,
    rt::END_PARALLEL,
    qis::READ_RESULT,
    rt::READ_RESULT,
    qis::MOVE,
    qis::CZ,
    qis::SX,
    qis::RZ,
    qis::MRESETZ,
];

/// Check all defined functions are allowed intrinsics.
/// Returns `Err(function_name)` on the first disallowed function.
fn check_allowed_intrinsics(module: &Module) -> Result<(), String> {
    let entry_idx = find_entry_point(module);

    for (idx, func) in module.functions.iter().enumerate() {
        if func.is_declaration {
            continue;
        }
        if Some(idx) == entry_idx {
            continue;
        }
        let name = &func.name;
        if name.ends_with("_record_output") {
            continue;
        }
        if !ALLOWED_INTRINSICS.contains(&name.as_str()) {
            return Err(name.clone());
        }
    }
    Ok(())
}

/// Check that the entry-point has no conditional branches.
/// Returns `Ok(())` or `Err(message)`.
fn check_no_conditional_branches(module: &Module) -> Result<(), &'static str> {
    let entry_idx = find_entry_point(module).ok_or("no entry point function found in IR")?;
    let entry_func = &module.functions[entry_idx];

    for block in &entry_func.basic_blocks {
        for instr in &block.instructions {
            if matches!(instr, Instruction::Br { .. }) {
                return Err("programs with branching control flow are not supported");
            }
        }
    }
    Ok(())
}

/// Validate that the module only contains allowed intrinsics.
///
/// Raises `ValueError` if a function is found that is not the entry point,
/// not an output-recording intrinsic, and not in the allow-list.
#[pyfunction]
pub fn validate_allowed_intrinsics(ir: &str) -> PyResult<()> {
    let module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("failed to parse IR: {e}")))?;
    check_allowed_intrinsics(&module)
        .map_err(|name| PyValueError::new_err(format!("{name} is not a supported intrinsic")))
}

/// Validate that the entry-point function contains only unconditional branches.
///
/// Raises `ValueError` if any basic block has a conditional `br`.
#[pyfunction]
pub fn validate_no_conditional_branches(ir: &str) -> PyResult<()> {
    let module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("failed to parse IR: {e}")))?;
    check_no_conditional_branches(&module).map_err(|msg| PyValueError::new_err(msg))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal base-profile IR with no branching and only allowed intrinsics.
    const VALID_IR: &str = r#"
; ModuleID = 'test'
source_filename = "test"

%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
entry:
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  ret void
}

declare void @__quantum__qis__sx__body(%Qubit*)
declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

attributes #0 = { "entry_point" "required_num_qubits"="2" "required_num_results"="1" }
"#;

    #[test]
    fn valid_ir_passes_both_checks() {
        let module = parse_module(VALID_IR).expect("parse");
        check_allowed_intrinsics(&module).expect("should pass");
        check_no_conditional_branches(&module).expect("should pass");
    }

    #[test]
    fn disallowed_intrinsic_is_rejected() {
        let ir = r#"
; ModuleID = 'test'
source_filename = "test"

%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
entry:
  ret void
}

define void @__quantum__qis__h__body(%Qubit* %q) {
entry:
  ret void
}

declare void @__quantum__qis__sx__body(%Qubit*)

attributes #0 = { "entry_point" "required_num_qubits"="1" "required_num_results"="0" }
"#;
        let err = check_allowed_intrinsics(&parse_module(ir).expect("parse")).unwrap_err();
        assert!(
            err.contains("h__body"),
            "error should mention the disallowed function: {err}"
        );
    }

    #[test]
    fn conditional_branch_is_rejected() {
        let ir = r#"
; ModuleID = 'test'
source_filename = "test"

%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
entry:
  br i1 true, label %then, label %else

then:
  ret void

else:
  ret void
}

attributes #0 = { "entry_point" "required_num_qubits"="1" "required_num_results"="0" }
"#;
        let err = check_no_conditional_branches(&parse_module(ir).expect("parse")).unwrap_err();
        assert!(
            err.contains("branching control flow"),
            "unexpected error: {err}"
        );
    }
}
