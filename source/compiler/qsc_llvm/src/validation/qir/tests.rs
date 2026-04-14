// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::model::Type;
use crate::model::*;
use crate::text::reader::parse_module;

// -- Test helper: build a minimal valid base profile v1 module --
fn base_v1_module() -> Module {
    parse_module(
            r#"%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_r\00"

define i64 @ENTRYPOINT__main() #0 {
entry:
  call void @__quantum__rt__initialize(ptr null)
  br label %body
body:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measurements
measurements:
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  br label %output
output:
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1
declare void @__quantum__rt__tuple_record_output(i64, ptr)
declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="1" }
attributes #1 = { "irreversible" }

!llvm.module.flags = !{!0, !1, !2, !3}
!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
"#,
        )
        .expect("base_v1_module IR should parse")
}

// -- Test helper: build a minimal valid adaptive v2 module --
fn adaptive_v2_1_module() -> Module {
    parse_module(
            r#"@0 = internal constant [4 x i8] c"0_r\00"

define i64 @ENTRYPOINT__main() #0 {
entry:
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)
declare i1 @__quantum__rt__read_result(ptr)
declare void @__quantum__qis__h__body(ptr)
declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3}
!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
"#,
        )
        .expect("adaptive_v2_1_module IR should parse")
}

fn has_error<F: Fn(&QirProfileError) -> bool>(result: &QirProfileValidation, pred: F) -> bool {
    result.errors.iter().any(pred)
}

fn set_float_computations(module: &mut Module, widths: &[&str]) {
    let values = widths
        .iter()
        .map(|width| MetadataValue::String((*width).to_string()))
        .collect();

    set_flag_value(
        module,
        "float_computations",
        5,
        MetadataValue::SubList(values),
    );
}

fn set_flag_value(module: &mut Module, key: &str, behavior: i64, value: MetadataValue) {
    if let Some(node) = module.metadata_nodes.iter_mut().find(|node| {
        node.values
            .iter()
            .any(|entry| matches!(entry, MetadataValue::String(text) if text == key))
    }) {
        node.values[0] = MetadataValue::Int(Type::Integer(32), behavior);
        node.values[2] = value;
        return;
    }

    let next_id = module
        .metadata_nodes
        .iter()
        .map(|node| node.id)
        .max()
        .unwrap_or(0)
        + 1;

    module.metadata_nodes.push(MetadataNode {
        id: next_id,
        values: vec![
            MetadataValue::Int(Type::Integer(32), behavior),
            MetadataValue::String(key.to_string()),
            value,
        ],
    });

    if let Some(module_flags) = module
        .named_metadata
        .iter_mut()
        .find(|metadata| metadata.name == "llvm.module.flags")
    {
        module_flags.node_refs.push(next_id);
    } else {
        module.named_metadata.push(NamedMetadata {
            name: "llvm.module.flags".to_string(),
            node_refs: vec![next_id],
        });
    }
}

fn set_bool_flag(module: &mut Module, key: &str, value: bool) {
    set_flag_value(
        module,
        key,
        1,
        MetadataValue::Int(Type::Integer(1), i64::from(value)),
    );
}

fn set_backwards_branching_flag(module: &mut Module, value: u8) {
    set_flag_value(
        module,
        "backwards_branching",
        7,
        MetadataValue::Int(Type::Integer(8), i64::from(value)),
    );
}

fn push_declaration(module: &mut Module, name: &str, return_type: Type, params: Vec<Type>) {
    module.functions.push(Function {
        name: name.to_string(),
        return_type,
        params: params
            .into_iter()
            .map(|ty| Param { ty, name: None })
            .collect(),
        is_declaration: true,
        attribute_group_refs: Vec::new(),
        basic_blocks: Vec::new(),
    });
}

fn assert_adaptive_single_float_width_is_allowed(width_name: &str, ty: &Type) {
    let mut m = adaptive_v2_1_module();
    set_float_computations(&mut m, &[width_name]);

    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    m.functions[ep_idx].basic_blocks[0].instructions.insert(
        1,
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty: ty.clone(),
            lhs: Operand::float_const(ty.clone(), 1.0),
            rhs: Operand::float_const(ty.clone(), 2.0),
            result: format!("{width_name}_sum"),
        },
    );

    let result = validate_qir_profile(&m);
    assert_eq!(
        result.detected.capabilities.float_computations,
        vec![width_name.to_string()]
    );
    assert!(
        result.errors.is_empty(),
        "expected {width_name}-only float surface to validate, got: {:#?}",
        result.errors
    );
}

// ---- Profile detection tests ----

#[test]
fn detect_base_v1_profile() {
    let m = base_v1_module();
    let result = validate_qir_profile(&m);
    assert_eq!(result.detected.profile, QirProfile::BaseV1);
}

#[test]
fn detect_adaptive_v2_profile() {
    let m = adaptive_v2_1_module();
    let result = validate_qir_profile(&m);
    assert_eq!(result.detected.profile, QirProfile::AdaptiveV2);
}

// ---- Base profile valid module ----

#[test]
fn base_v1_valid_module_no_violations() {
    let m = base_v1_module();
    let result = validate_qir_profile(&m);
    assert!(
        result.errors.is_empty(),
        "expected no errors, got: {:#?}",
        result.errors
    );
}

// ---- Adaptive valid module ----

#[test]
fn adaptive_v2_1_valid_module_no_violations() {
    let m = adaptive_v2_1_module();
    let result = validate_qir_profile(&m);
    assert!(
        result.errors.is_empty(),
        "expected no errors, got: {:#?}",
        result.errors
    );
}

// ---- MS-01: Missing struct types ----

#[test]
fn base_v1_missing_qubit_struct() {
    let mut m = base_v1_module();
    m.struct_types.retain(|s| s.name != "Qubit");
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::MissingOpaqueType { .. }
    )));
}

// ---- MS-03: Multiple entry points ----

#[test]
fn multiple_entry_points_violation() {
    let mut m = base_v1_module();
    // Add a second entry point.
    m.functions.push(Function {
        name: "ENTRYPOINT__other".into(),
        return_type: Type::Integer(64),
        params: Vec::new(),
        is_declaration: false,
        attribute_group_refs: vec![0],
        basic_blocks: vec![BasicBlock {
            name: "entry".into(),
            instructions: vec![Instruction::Ret(Some(Operand::IntConst(
                Type::Integer(64),
                0,
            )))],
        }],
    });
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::EntryPointCount { .. }
    )));
}

// ---- MF-01: Missing module flags ----

#[test]
fn missing_module_flags_violation() {
    let mut m = base_v1_module();
    m.named_metadata.clear();
    m.metadata_nodes.clear();
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingModuleFlag { flag_name } if flag_name == "qir_major_version")
    ));
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingModuleFlag { flag_name } if flag_name == "qir_minor_version")
    ));
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingModuleFlag { flag_name } if flag_name == "dynamic_qubit_management")
    ));
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingModuleFlag { flag_name } if flag_name == "dynamic_result_management")
    ));
}

// ---- EP-01: Entry point with parameters ----

#[test]
fn entry_point_with_params_violation() {
    let mut m = base_v1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    m.functions[ep_idx].params.push(Param {
        ty: Type::Integer(32),
        name: None,
    });
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::EntryPointParams { .. }
    )));
}

// ---- EP-02: Wrong return type ----

#[test]
fn entry_point_wrong_return_type() {
    let mut m = base_v1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    m.functions[ep_idx].return_type = Type::Void;
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::EntryPointReturnType { .. }
    )));
}

// ---- EP-03: Wrong block count for base ----

#[test]
fn base_v1_wrong_block_count() {
    let mut m = base_v1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    // Remove blocks to have only 2.
    m.functions[ep_idx].basic_blocks.truncate(2);
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::BaseBlockCount { .. }
    )));
}

// ---- CI-01: BinOp in base profile ----

#[test]
fn base_v1_binop_not_allowed() {
    let mut m = base_v1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    // Insert an Add instruction in the body block.
    m.functions[ep_idx].basic_blocks[1].instructions.insert(
        0,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::IntConst(Type::Integer(64), 1),
            rhs: Operand::IntConst(Type::Integer(64), 2),
            result: "sum".into(),
        },
    );
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::InstructionNotAllowed { .. }
    )));
}

// ---- CI-04: Conditional branch in base profile ----

#[test]
fn base_v1_conditional_branch_violation() {
    let mut m = base_v1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    // Replace a jump with conditional branch.
    let last = m.functions[ep_idx].basic_blocks[0].instructions.len() - 1;
    m.functions[ep_idx].basic_blocks[0].instructions[last] = Instruction::Br {
        cond_ty: Type::Integer(1),
        cond: Operand::IntConst(Type::Integer(1), 1),
        true_dest: "body".into(),
        false_dest: "body".into(),
    };
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::BaseConditionalBranch { .. }
    )));
}

// ---- AP-CI-02: Int instructions without capability ----

#[test]
fn adaptive_int_binop_without_capability() {
    let mut m = adaptive_v2_1_module();
    // No int_computations flag in metadata → should fail.
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    m.functions[ep_idx].basic_blocks[0].instructions.insert(
        1,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::IntConst(Type::Integer(64), 1),
            rhs: Operand::IntConst(Type::Integer(64), 2),
            result: "sum".into(),
        },
    );
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingCapability { capability, .. } if capability == "int_computations")
    ));
}

// ---- AP-CI-03: Float instructions without capability ----

#[test]
fn adaptive_float_binop_without_capability() {
    let mut m = adaptive_v2_1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    m.functions[ep_idx].basic_blocks[0].instructions.insert(
        1,
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty: Type::Double,
            lhs: Operand::float_const(Type::Double, 1.0),
            rhs: Operand::float_const(Type::Double, 2.0),
            result: "fsum".into(),
        },
    );
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingCapability { capability, .. } if capability == "float_computations")
    ));
}

#[test]
fn adaptive_float_signature_without_capability_triggers_cr_02() {
    let mut m = adaptive_v2_1_module();
    m.functions.push(Function {
        name: "use_double".to_string(),
        return_type: Type::Void,
        params: vec![Param {
            ty: Type::Double,
            name: None,
        }],
        is_declaration: true,
        attribute_group_refs: Vec::new(),
        basic_blocks: Vec::new(),
    });

    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::CapabilityNotDeclared { flag_name, .. } if flag_name == "float_computations")
    ));
}

#[test]
fn adaptive_undeclared_float_width_triggers_allow_list_violation() {
    let mut m = adaptive_v2_1_module();
    set_float_computations(&mut m, &["half"]);

    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    m.functions[ep_idx].basic_blocks[0].instructions.insert(
        1,
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty: Type::Double,
            lhs: Operand::float_const(Type::Double, 1.0),
            rhs: Operand::float_const(Type::Double, 2.0),
            result: "fsum".into(),
        },
    );

    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::FloatWidthNotDeclared { width_name } if width_name == "double"
    )));
}

#[test]
fn adaptive_float_capability_without_operation_triggers_contract_violation() {
    let mut m = adaptive_v2_1_module();
    set_float_computations(&mut m, &["double"]);
    m.functions.push(Function {
        name: "use_double".to_string(),
        return_type: Type::Void,
        params: vec![Param {
            ty: Type::Double,
            name: None,
        }],
        is_declaration: true,
        attribute_group_refs: Vec::new(),
        basic_blocks: Vec::new(),
    });

    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::FloatCapabilityWithoutOperation
    )));
}

#[test]
fn adaptive_over_declared_float_widths_are_allowed() {
    let mut m = adaptive_v2_1_module();
    set_float_computations(&mut m, &["half", "double"]);

    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    m.functions[ep_idx].basic_blocks[0].instructions.insert(
        1,
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty: Type::Double,
            lhs: Operand::float_const(Type::Double, 1.0),
            rhs: Operand::float_const(Type::Double, 2.0),
            result: "fsum".into(),
        },
    );

    let result = validate_qir_profile(&m);
    assert!(
        result.errors.is_empty(),
        "expected no errors, got: {:#?}",
        result.errors
    );
}

#[test]
fn adaptive_half_only_float_width_is_allowed() {
    assert_adaptive_single_float_width_is_allowed("half", &Type::Half);
}

#[test]
fn adaptive_float_only_float_width_is_allowed() {
    assert_adaptive_single_float_width_is_allowed("float", &Type::Float);
}

#[test]
fn adaptive_double_only_float_width_is_allowed() {
    assert_adaptive_single_float_width_is_allowed("double", &Type::Double);
}

// ---- AP-CI-04: Switch without capability ----

#[test]
fn adaptive_switch_without_capability() {
    let mut m = adaptive_v2_1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    // Replace ret with switch + extra blocks.
    let last = m.functions[ep_idx].basic_blocks[0].instructions.len() - 1;
    m.functions[ep_idx].basic_blocks[0].instructions[last] = Instruction::Switch {
        ty: Type::Integer(64),
        value: Operand::IntConst(Type::Integer(64), 0),
        default_dest: "exit".into(),
        cases: Vec::new(),
    };
    m.functions[ep_idx].basic_blocks.push(BasicBlock {
        name: "exit".into(),
        instructions: vec![Instruction::Ret(Some(Operand::IntConst(
            Type::Integer(64),
            0,
        )))],
    });
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingCapability { capability, .. } if capability == "multiple_target_branching")
    ));
}

// ---- RT-01: Missing rt::initialize ----

#[test]
fn missing_rt_initialize() {
    let mut m = base_v1_module();
    m.functions
        .retain(|f| f.name != "__quantum__rt__initialize");
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingDeclaration { function_name, .. } if function_name == "__quantum__rt__initialize")
    ));
}

// ---- AP-MC-02: Missing read_result for adaptive ----

#[test]
fn adaptive_missing_read_result() {
    let mut m = adaptive_v2_1_module();
    m.functions
        .retain(|f| f.name != "__quantum__rt__read_result");
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingDeclaration { function_name, .. } if function_name == "__quantum__rt__read_result")
    ));
}

// ---- QIS-01: QIS non-void return in base ----

#[test]
fn base_v1_qis_non_void_return() {
    let mut m = base_v1_module();
    // Change h gate to return i1.
    if let Some(f) = m
        .functions
        .iter_mut()
        .find(|f| f.name == "__quantum__qis__h__body")
    {
        f.return_type = Type::Integer(1);
    }
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::QisNonVoidReturn { .. }
    )));
}

// ---- DT-04: Base profile with dynamic_qubit_management = true ----

#[test]
fn base_v1_dynamic_qubit_management_true() {
    let mut m = base_v1_module();
    // Change dynamic_qubit_management flag to 1.
    if let Some(node) = m.metadata_nodes.iter_mut().find(|n| {
        n.values
            .iter()
            .any(|v| matches!(v, MetadataValue::String(s) if s == "dynamic_qubit_management"))
    }) {
        node.values[2] = MetadataValue::Int(Type::Integer(1), 1);
    }
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::BaseDynamicMgmtEnabled { .. }
    )));
}

// ---- CF-01: Non-linear flow in base profile ----

#[test]
fn base_v1_non_linear_flow() {
    let mut m = base_v1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    // Make block 0 jump to block 2 (skipping block 1).
    let last = m.functions[ep_idx].basic_blocks[0].instructions.len() - 1;
    m.functions[ep_idx].basic_blocks[0].instructions[last] = Instruction::Jump {
        dest: "measurements".into(),
    };
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::NonLinearFlow { .. }
    )));
}

// ---- CF-03: Cycle detection ----

#[test]
fn adaptive_cycle_without_backwards_branching() {
    let mut m = adaptive_v2_1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    // Create a cycle: entry → loop → entry (back-edge).
    let last = m.functions[ep_idx].basic_blocks[0].instructions.len() - 1;
    m.functions[ep_idx].basic_blocks[0].instructions[last] = Instruction::Jump {
        dest: "loop".into(),
    };
    m.functions[ep_idx].basic_blocks.push(BasicBlock {
        name: "loop".into(),
        instructions: vec![Instruction::Jump {
            dest: "entry".into(),
        }],
    });
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::UnauthorizedCycle { .. }
    )));
}

#[test]
fn adaptive_cycle_with_backwards_branching_is_allowed() {
    let mut m = adaptive_v2_1_module();
    set_backwards_branching_flag(&mut m, 1);
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    let last = m.functions[ep_idx].basic_blocks[0].instructions.len() - 1;
    m.functions[ep_idx].basic_blocks[0].instructions[last] = Instruction::Jump {
        dest: "loop".into(),
    };
    m.functions[ep_idx].basic_blocks.push(BasicBlock {
        name: "loop".into(),
        instructions: vec![Instruction::Jump {
            dest: "entry".into(),
        }],
    });

    let result = validate_qir_profile(&m);
    assert!(
        result.errors.is_empty(),
        "expected backwards_branching-enabled cycle to validate, got: {:#?}",
        result.errors
    );
}

// ---- CF-04: Multiple ret without capability ----

#[test]
fn adaptive_multiple_ret_without_capability() {
    let mut m = adaptive_v2_1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    let last = m.functions[ep_idx].basic_blocks[0].instructions.len() - 1;
    m.functions[ep_idx].basic_blocks[0].instructions[last] = Instruction::Br {
        cond_ty: Type::Integer(1),
        cond: Operand::IntConst(Type::Integer(1), 1),
        true_dest: "then".into(),
        false_dest: "else".into(),
    };
    m.functions[ep_idx].basic_blocks.push(BasicBlock {
        name: "then".into(),
        instructions: vec![Instruction::Ret(Some(Operand::IntConst(
            Type::Integer(64),
            0,
        )))],
    });
    m.functions[ep_idx].basic_blocks.push(BasicBlock {
        name: "else".into(),
        instructions: vec![Instruction::Ret(Some(Operand::IntConst(
            Type::Integer(64),
            1,
        )))],
    });
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::UnauthorizedMultipleReturns { .. }
    )));
}

#[test]
fn adaptive_multiple_ret_with_capability_is_allowed() {
    let mut m = adaptive_v2_1_module();
    set_bool_flag(&mut m, "multiple_return_points", true);
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    let last = m.functions[ep_idx].basic_blocks[0].instructions.len() - 1;
    m.functions[ep_idx].basic_blocks[0].instructions[last] = Instruction::Br {
        cond_ty: Type::Integer(1),
        cond: Operand::IntConst(Type::Integer(1), 1),
        true_dest: "then".into(),
        false_dest: "else".into(),
    };
    m.functions[ep_idx].basic_blocks.push(BasicBlock {
        name: "then".into(),
        instructions: vec![Instruction::Ret(Some(Operand::IntConst(
            Type::Integer(64),
            0,
        )))],
    });
    m.functions[ep_idx].basic_blocks.push(BasicBlock {
        name: "else".into(),
        instructions: vec![Instruction::Ret(Some(Operand::IntConst(
            Type::Integer(64),
            1,
        )))],
    });

    let result = validate_qir_profile(&m);
    assert!(
        result.errors.is_empty(),
        "expected multiple_return_points-enabled control flow to validate, got: {:#?}",
        result.errors
    );
}

#[test]
fn unsupported_profile_version_pair_is_reported_explicitly() {
    let mut m = base_v1_module();
    set_flag_value(
        &mut m,
        "qir_major_version",
        1,
        MetadataValue::Int(Type::Integer(32), 2),
    );

    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::UnsupportedProfileMetadata {
            profile_name,
            major_version,
        } if profile_name == "base_profile" && *major_version == 2
    )));
}

#[test]
fn dangling_module_flag_reference_is_reported_without_hiding_later_flags() {
    let mut m = adaptive_v2_1_module();
    m.named_metadata[0].node_refs.insert(0, 999);

    let result = validate_qir_profile(&m);
    assert_eq!(result.detected.profile, QirProfile::AdaptiveV2);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::DanglingModuleFlagReference { node_ref } if *node_ref == 999
    )));
    assert!(
        !has_error(&result, |e| matches!(
            e,
            QirProfileError::MissingModuleFlag { flag_name } if flag_name == "qir_major_version"
        )),
        "dangling refs should not hide later valid qir_major_version flags"
    );
}

#[test]
fn malformed_float_capability_flag_is_reported_instead_of_missing_capability() {
    let mut m = adaptive_v2_1_module();
    set_flag_value(
        &mut m,
        "float_computations",
        5,
        MetadataValue::SubList(vec![MetadataValue::Int(Type::Integer(32), 1)]),
    );
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    m.functions[ep_idx].basic_blocks[0].instructions.insert(
        1,
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty: Type::Double,
            lhs: Operand::float_const(Type::Double, 1.0),
            rhs: Operand::float_const(Type::Double, 2.0),
            result: "fsum".into(),
        },
    );

    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::MalformedModuleFlag { flag_name, .. } if flag_name == "float_computations"
    )));
    assert!(!has_error(&result, |e| matches!(
        e,
        QirProfileError::MissingCapability { capability, .. } if capability == "float_computations"
    )));
    assert!(!has_error(&result, |e| matches!(
        e,
        QirProfileError::CapabilityNotDeclared { flag_name, .. } if flag_name == "float_computations"
    )));
}

#[test]
fn malformed_multiple_return_points_flag_is_reported_instead_of_defaulting_to_missing() {
    let mut m = adaptive_v2_1_module();
    set_flag_value(
        &mut m,
        "multiple_return_points",
        1,
        MetadataValue::String("true".to_string()),
    );
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    let last = m.functions[ep_idx].basic_blocks[0].instructions.len() - 1;
    m.functions[ep_idx].basic_blocks[0].instructions[last] = Instruction::Br {
        cond_ty: Type::Integer(1),
        cond: Operand::IntConst(Type::Integer(1), 1),
        true_dest: "then".into(),
        false_dest: "else".into(),
    };
    m.functions[ep_idx].basic_blocks.push(BasicBlock {
        name: "then".into(),
        instructions: vec![Instruction::Ret(Some(Operand::IntConst(
            Type::Integer(64),
            0,
        )))],
    });
    m.functions[ep_idx].basic_blocks.push(BasicBlock {
        name: "else".into(),
        instructions: vec![Instruction::Ret(Some(Operand::IntConst(
            Type::Integer(64),
            1,
        )))],
    });

    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::MalformedModuleFlag { flag_name, .. } if flag_name == "multiple_return_points"
    )));
    assert!(!has_error(&result, |e| matches!(
        e,
        QirProfileError::UnauthorizedMultipleReturns { .. }
    )));
    assert!(!has_error(&result, |e| matches!(
        e,
        QirProfileError::CapabilityNotDeclared { flag_name, .. } if flag_name == "multiple_return_points"
    )));
}

// ---- CR-01: Int instructions without capability flag ----

#[test]
fn adaptive_consistency_int_without_flag() {
    let mut m = adaptive_v2_1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    m.functions[ep_idx].basic_blocks[0].instructions.insert(
        1,
        Instruction::ICmp {
            pred: crate::model::IntPredicate::Eq,
            ty: Type::Integer(64),
            lhs: Operand::IntConst(Type::Integer(64), 0),
            rhs: Operand::IntConst(Type::Integer(64), 1),
            result: "cmp".into(),
        },
    );
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::CapabilityNotDeclared { flag_name, .. } if flag_name == "int_computations")
    ));
}

// ---- CR-05: Switch without multiple_target_branching ----

#[test]
fn adaptive_consistency_switch_without_flag() {
    let mut m = adaptive_v2_1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    let last = m.functions[ep_idx].basic_blocks[0].instructions.len() - 1;
    m.functions[ep_idx].basic_blocks[0].instructions[last] = Instruction::Switch {
        ty: Type::Integer(64),
        value: Operand::IntConst(Type::Integer(64), 0),
        default_dest: "exit".into(),
        cases: Vec::new(),
    };
    m.functions[ep_idx].basic_blocks.push(BasicBlock {
        name: "exit".into(),
        instructions: vec![Instruction::Ret(Some(Operand::IntConst(
            Type::Integer(64),
            0,
        )))],
    });
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::CapabilityNotDeclared { flag_name, .. } if flag_name == "multiple_target_branching")
    ));
}

// ---- AT-02: Missing qir_profiles attribute ----

#[test]
fn missing_qir_profiles_attribute() {
    let mut m = base_v1_module();
    // Remove qir_profiles from attribute group.
    m.attribute_groups[0]
        .attributes
        .retain(|a| !matches!(a, Attribute::KeyValue(k, _) if k == "qir_profiles"));
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingEntryPointAttr { attr_name } if attr_name == "qir_profiles")
    ));
}

// ---- AT-03: Missing required_num_qubits ----

#[test]
fn missing_required_num_qubits() {
    let mut m = base_v1_module();
    m.attribute_groups[0]
        .attributes
        .retain(|a| !matches!(a, Attribute::KeyValue(k, _) if k == "required_num_qubits"));
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingEntryPointAttr { attr_name } if attr_name == "required_num_qubits")
    ));
}

#[test]
fn dynamic_management_flags_allow_missing_required_counts() {
    let mut m = adaptive_v2_1_module();
    set_bool_flag(&mut m, "dynamic_qubit_management", true);
    set_bool_flag(&mut m, "dynamic_result_management", true);
    m.attribute_groups[0].attributes.retain(|attr| {
        !matches!(attr, Attribute::KeyValue(key, _) if key == "required_num_qubits" || key == "required_num_results")
    });

    push_declaration(&mut m, qir::rt::QUBIT_ALLOCATE, Type::Ptr, Vec::new());
    push_declaration(&mut m, qir::rt::QUBIT_RELEASE, Type::Void, vec![Type::Ptr]);
    push_declaration(&mut m, qir::rt::RESULT_ALLOCATE, Type::Ptr, Vec::new());
    push_declaration(&mut m, qir::rt::RESULT_RELEASE, Type::Void, vec![Type::Ptr]);

    let result = validate_qir_profile(&m);
    assert!(
        result.errors.is_empty(),
        "expected dynamic-management entry point counts to be optional, got: {:#?}",
        result.errors
    );
}

// ---- RT-03: Wrong output recording signature ----

#[test]
fn wrong_tuple_record_output_signature() {
    let mut m = base_v1_module();
    if let Some(f) = m
        .functions
        .iter_mut()
        .find(|f| f.name == "__quantum__rt__tuple_record_output")
    {
        f.params.clear(); // Wrong: no params.
    }
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::WrongSignature { .. }
    )));
}

// ---- RT-03: result_record_output wrong return type ----

#[test]
fn result_record_output_wrong_return_type() {
    let mut m = base_v1_module();
    if let Some(f) = m
        .functions
        .iter_mut()
        .find(|f| f.name == "__quantum__rt__result_record_output")
    {
        f.return_type = Type::Integer(64); // Wrong: should be void.
    }
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::WrongSignature { function_name, .. } if function_name == "__quantum__rt__result_record_output")
    ));
}

// ---- RT-03: result_record_output wrong param count ----

#[test]
fn result_record_output_wrong_param_count() {
    let mut m = base_v1_module();
    if let Some(f) = m
        .functions
        .iter_mut()
        .find(|f| f.name == "__quantum__rt__result_record_output")
    {
        f.params = vec![Param {
            ty: Type::Ptr,
            name: None,
        }]; // Wrong: 1 param instead of 2.
    }
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::WrongSignature { function_name, .. } if function_name == "__quantum__rt__result_record_output")
    ));
}

#[test]
fn result_array_record_output_wrong_signature() {
    let mut m = adaptive_v2_1_module();
    push_declaration(
        &mut m,
        qir::rt::RESULT_ARRAY_RECORD_OUTPUT,
        Type::Void,
        vec![Type::Ptr, Type::Ptr],
    );

    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::WrongSignature { function_name, .. }
            if function_name == qir::rt::RESULT_ARRAY_RECORD_OUTPUT
    )));
}

// ---- RT-03: array_record_output wrong return type ----

#[test]
fn array_record_output_wrong_return_type() {
    let mut m = base_v1_module();
    // Add an array_record_output with wrong return type.
    m.functions.push(Function {
        name: "__quantum__rt__array_record_output".into(),
        return_type: Type::Integer(64), // Wrong: should be void.
        params: vec![
            Param {
                ty: Type::Integer(64),
                name: None,
            },
            Param {
                ty: Type::Ptr,
                name: None,
            },
        ],
        is_declaration: true,
        attribute_group_refs: Vec::new(),
        basic_blocks: Vec::new(),
    });
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::WrongSignature { function_name, .. } if function_name == "__quantum__rt__array_record_output")
    ));
}

// ---- RT-03: array_record_output wrong param count ----

#[test]
fn array_record_output_wrong_param_count() {
    let mut m = base_v1_module();
    // Add an array_record_output with wrong param count.
    m.functions.push(Function {
        name: "__quantum__rt__array_record_output".into(),
        return_type: Type::Void,
        params: vec![Param {
            ty: Type::Integer(64),
            name: None,
        }], // Wrong: 1 param instead of 2.
        is_declaration: true,
        attribute_group_refs: Vec::new(),
        basic_blocks: Vec::new(),
    });
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::WrongSignature { function_name, .. } if function_name == "__quantum__rt__array_record_output")
    ));
}

// ---- RT-02: initialize wrong signature ----

#[test]
fn initialize_wrong_sig_triggers_rt_02() {
    let mut m = base_v1_module();
    if let Some(f) = m
        .functions
        .iter_mut()
        .find(|f| f.name == "__quantum__rt__initialize")
    {
        f.return_type = Type::Integer(32); // Wrong: should be void.
    }
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::InitializeWrongSignature { .. }
    )));
}

// ---- RT-04: bool_record_output wrong signature ----

#[test]
fn bool_record_output_wrong_sig_triggers_rt_04() {
    let mut m = adaptive_v2_1_module();
    // Add a bool_record_output with incorrect signature.
    m.functions.push(Function {
        name: "__quantum__rt__bool_record_output".into(),
        return_type: Type::Void,
        params: vec![
            Param {
                ty: Type::Integer(64), // Wrong: should be i1.
                name: None,
            },
            Param {
                ty: Type::Ptr,
                name: None,
            },
        ],
        is_declaration: true,
        attribute_group_refs: Vec::new(),
        basic_blocks: Vec::new(),
    });
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::WrongSignature { function_name, .. } if function_name == "__quantum__rt__bool_record_output")
    ));
}

// ---- RT-07: qubit_allocate missing when dynamic_qubit_management ----

#[test]
fn qubit_allocate_missing_when_dynamic_mgmt_triggers_rt_07() {
    let mut m = adaptive_v2_1_module();
    // Enable dynamic_qubit_management.
    if let Some(node) = m.metadata_nodes.iter_mut().find(|n| {
        n.values
            .iter()
            .any(|v| matches!(v, MetadataValue::String(s) if s == "dynamic_qubit_management"))
    }) {
        node.values[2] = MetadataValue::Int(Type::Integer(1), 1);
    }
    // Do not add qubit_allocate declaration.
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingDeclaration { function_name, .. } if function_name == "__quantum__rt__qubit_allocate")
    ));
}

// ---- RT-09: result_allocate missing when dynamic_result_management ----

#[test]
fn result_allocate_missing_when_dynamic_mgmt_triggers_rt_09() {
    let mut m = adaptive_v2_1_module();
    // Enable dynamic_result_management.
    if let Some(node) = m.metadata_nodes.iter_mut().find(|n| {
        n.values
            .iter()
            .any(|v| matches!(v, MetadataValue::String(s) if s == "dynamic_result_management"))
    }) {
        node.values[2] = MetadataValue::Int(Type::Integer(1), 1);
    }
    // Do not add result_allocate declaration.
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingDeclaration { function_name, .. } if function_name == "__quantum__rt__result_allocate")
    ));
}

// ---- AR-02: alloca in adaptive without arrays flag ----

#[test]
fn arrays_instructions_without_flag_trigger_ar_02() {
    let mut m = adaptive_v2_1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    // Insert an alloca instruction.
    m.functions[ep_idx].basic_blocks[0].instructions.insert(
        1,
        Instruction::Alloca {
            ty: Type::Integer(64),
            result: "alloc".into(),
        },
    );
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::ArraysNotEnabled { .. }
    )));
}

#[test]
fn arrays_instructions_with_arrays_flag_are_allowed() {
    let mut m = adaptive_v2_1_module();
    set_bool_flag(&mut m, "arrays", true);
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    m.functions[ep_idx].basic_blocks[0].instructions.insert(
        1,
        Instruction::Alloca {
            ty: Type::Integer(64),
            result: "alloc".into(),
        },
    );

    let result = validate_qir_profile(&m);
    assert!(
        result.errors.is_empty(),
        "expected arrays-enabled alloca to validate, got: {:#?}",
        result.errors
    );
}

#[test]
fn result_array_record_output_requires_declaration_when_used() {
    let mut m = adaptive_v2_1_module();
    set_bool_flag(&mut m, "arrays", true);
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    let insert_at = m.functions[ep_idx].basic_blocks[0].instructions.len() - 1;
    m.functions[ep_idx].basic_blocks[0].instructions.insert(
        insert_at,
        Instruction::Call {
            return_ty: None,
            callee: qir::rt::RESULT_ARRAY_RECORD_OUTPUT.to_string(),
            args: vec![
                (Type::Integer(64), Operand::IntConst(Type::Integer(64), 1)),
                (Type::Ptr, Operand::IntToPtr(0, Type::Ptr)),
                (Type::Ptr, Operand::GlobalRef("0".into())),
            ],
            result: None,
            attr_refs: Vec::new(),
        },
    );

    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::MissingDeclaration { function_name, .. }
            if function_name == qir::rt::RESULT_ARRAY_RECORD_OUTPUT
    )));
}

#[test]
fn result_record_output_requires_string_label_operand() {
    let mut m = base_v1_module();
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    let output_block = m.functions[ep_idx]
        .basic_blocks
        .iter_mut()
        .find(|block| block.name == "output")
        .expect("output block");

    let Instruction::Call { args, .. } = &mut output_block.instructions[0] else {
        panic!("expected result_record_output call");
    };
    args[1] = (Type::Ptr, Operand::NullPtr);

    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::InvalidOutputLabelOperand { function_name, .. }
            if function_name == qir::rt::RESULT_RECORD_OUTPUT
    )));
}

// ---- QIS-02: measurement missing irreversible ----

#[test]
fn measurement_missing_irreversible_triggers_qis_02() {
    let mut m = base_v1_module();
    // Rename measurement function to contain "measure" so it triggers QIS-02 check,
    // and remove its irreversible attribute reference.
    if let Some(f) = m
        .functions
        .iter_mut()
        .find(|f| f.name == "__quantum__qis__m__body")
    {
        f.name = "__quantum__qis__measure__body".to_string();
        f.attribute_group_refs.clear();
    }
    // Also update the call instruction in the entry point to use the new name.
    let ep_idx = qir::find_entry_point(&m).expect("entry point");
    for bb in &mut m.functions[ep_idx].basic_blocks {
        for instr in &mut bb.instructions {
            if let Instruction::Call { callee, .. } = instr
                && callee == "__quantum__qis__m__body"
            {
                *callee = "__quantum__qis__measure__body".to_string();
            }
        }
    }
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::MissingIrreversible { .. }
    )));
}

// ---- AT-06: required_num_qubits non-integer ----

#[test]
fn required_num_qubits_non_integer_triggers_at_06() {
    let mut m = base_v1_module();
    // Replace required_num_qubits value with a non-integer string.
    for ag in &mut m.attribute_groups {
        for attr in &mut ag.attributes {
            if let Attribute::KeyValue(k, v) = attr
                && k == "required_num_qubits"
            {
                *v = "not_a_number".to_string();
            }
        }
    }
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingEntryPointAttr { attr_name } if attr_name.contains("required_num_qubits"))
    ));
}

// ---- AT-05: missing output_labeling_schema ----

#[test]
fn missing_output_labeling_schema_triggers_at_05() {
    let mut m = base_v1_module();
    // Remove output_labeling_schema from attribute group.
    m.attribute_groups[0]
        .attributes
        .retain(|a| !matches!(a, Attribute::StringAttr(s) if s == "output_labeling_schema"));
    let result = validate_qir_profile(&m);
    assert!(has_error(
        &result,
        |e| matches!(e, QirProfileError::MissingEntryPointAttr { attr_name } if attr_name == "output_labeling_schema")
    ));
}

// ---- DT-05: base profile with dynamic_result_management = true ----

#[test]
fn base_dynamic_result_mgmt_enabled_triggers_dt_05() {
    let mut m = base_v1_module();
    // Change dynamic_result_management flag to 1.
    if let Some(node) = m.metadata_nodes.iter_mut().find(|n| {
        n.values
            .iter()
            .any(|v| matches!(v, MetadataValue::String(s) if s == "dynamic_result_management"))
    }) {
        node.values[2] = MetadataValue::Int(Type::Integer(1), 1);
    }
    let result = validate_qir_profile(&m);
    assert!(has_error(&result, |e| matches!(
        e,
        QirProfileError::BaseDynamicMgmtEnabled { .. }
    )));
}
