// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::model::{BasicBlock, BinOpKind, Function, Instruction, Operand, Param, Type};
use crate::{
    GeneratedArtifact, Module, QirProfilePreset, ReadDiagnostic, ReadDiagnosticKind, ReadPolicy,
    ReadReport, parse_bitcode_compatibility_report, parse_bitcode_detailed, parse_module_detailed,
    validate_ir, validate_qir_profile,
};

#[derive(Copy, Clone)]
pub enum MutationKind {
    ReferenceOrdering,
    PhiStructure,
    Dominance,
    InvalidBranchTarget,
    CallShape,
    GepShape,
    AttributeRef,
}

impl MutationKind {
    pub fn from_data(data: &[u8]) -> Self {
        match mutation_selector(data, 0) % 7 {
            0 => Self::ReferenceOrdering,
            1 => Self::PhiStructure,
            2 => Self::Dominance,
            3 => Self::InvalidBranchTarget,
            4 => Self::CallShape,
            5 => Self::GepShape,
            _ => Self::AttributeRef,
        }
    }
}

pub type SeedMutator = fn(&Module, &[u8]) -> Module;

pub fn mutation_selector(data: &[u8], index: usize) -> u8 {
    data.get(index).copied().unwrap_or_default()
}

fn validate_seed_module(module: &Module) {
    // Seed modules continue to exercise the existing no-panic validation path.
    let _profile_result = validate_qir_profile(module);
    let _ir_result = validate_ir(module);
}

pub fn validate_seed_artifact(artifact: &GeneratedArtifact) {
    validate_seed_module(&artifact.module);
}

pub fn validate_mutated_module(module: &Module) {
    // Mutated modules intentionally violate structural rules, so only the LLVM
    // IR validator is relevant on this lane.
    let _mutated_ir_result = validate_ir(module);
}

fn assert_meaningful_diagnostics(label: &str, diagnostics: &[ReadDiagnostic]) {
    assert!(
        !diagnostics.is_empty(),
        "{label} should report at least one diagnostic"
    );

    for diagnostic in diagnostics {
        assert!(
            matches!(
                diagnostic.kind,
                ReadDiagnosticKind::MalformedInput
                    | ReadDiagnosticKind::UnsupportedSemanticConstruct
            ),
            "{label} returned an unexpected diagnostic kind: {diagnostic:?}"
        );
        assert!(
            !diagnostic.context.is_empty(),
            "{label} returned a diagnostic without context: {diagnostic:?}"
        );
        assert!(
            !diagnostic.message.trim().is_empty(),
            "{label} returned a diagnostic without a message: {diagnostic:?}"
        );
    }
}

fn assert_detailed_result_stable<T>(
    label: &str,
    first: &Result<T, Vec<ReadDiagnostic>>,
    second: &Result<T, Vec<ReadDiagnostic>>,
) where
    T: PartialEq + std::fmt::Debug,
{
    assert_eq!(
        first, second,
        "{label} changed outcome between repeated detailed parses"
    );

    if let Err(diagnostics) = first {
        assert_meaningful_diagnostics(label, diagnostics);
    }
}

fn assert_report_result_stable(
    label: &str,
    first: &Result<ReadReport, Vec<ReadDiagnostic>>,
    second: &Result<ReadReport, Vec<ReadDiagnostic>>,
) {
    assert_eq!(
        first, second,
        "{label} changed outcome between repeated compatibility reports"
    );

    match first {
        Ok(report) if !report.diagnostics.is_empty() => {
            assert_meaningful_diagnostics(label, &report.diagnostics);
        }
        Err(diagnostics) => assert_meaningful_diagnostics(label, diagnostics),
        Ok(_) => {}
    }
}

fn compile_raw_bitcode(data: &[u8]) {
    let strict_first = parse_bitcode_detailed(data, ReadPolicy::QirSubsetStrict);
    let strict_second = parse_bitcode_detailed(data, ReadPolicy::QirSubsetStrict);
    assert_detailed_result_stable("raw bitcode strict", &strict_first, &strict_second);

    let compatibility_first = parse_bitcode_detailed(data, ReadPolicy::Compatibility);
    let compatibility_second = parse_bitcode_detailed(data, ReadPolicy::Compatibility);
    assert_detailed_result_stable(
        "raw bitcode compatibility",
        &compatibility_first,
        &compatibility_second,
    );

    let report_first = parse_bitcode_compatibility_report(data);
    let report_second = parse_bitcode_compatibility_report(data);
    assert_report_result_stable(
        "raw bitcode compatibility report",
        &report_first,
        &report_second,
    );

    if compatibility_first.is_err() {
        assert!(
            strict_first.is_err(),
            "strict mode should not salvage raw bitcode after compatibility diagnostics"
        );
    }

    assert!(
        !(strict_first.is_err() && compatibility_first.is_ok()),
        "compatibility mode accepted raw bitcode without diagnostics while strict mode rejected it"
    );

    if let Ok(report) = &report_first {
        if !report.diagnostics.is_empty() {
            assert!(
                strict_first.is_err(),
                "strict mode should not salvage raw bitcode that compatibility recovered with diagnostics"
            );
            assert!(
                compatibility_first.is_err(),
                "detailed compatibility parsing should surface recovery diagnostics as an error"
            );
        }
    } else {
        assert!(
            strict_first.is_err(),
            "strict mode should not salvage raw bitcode after compatibility report failure"
        );
        assert!(
            compatibility_first.is_err(),
            "detailed compatibility parsing should fail when compatibility reporting fails"
        );
    }
}

fn compile_raw_utf8_text(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    let strict_first = parse_module_detailed(text, ReadPolicy::QirSubsetStrict);
    let strict_second = parse_module_detailed(text, ReadPolicy::QirSubsetStrict);
    assert_detailed_result_stable("raw utf-8 text strict", &strict_first, &strict_second);

    let compatibility_first = parse_module_detailed(text, ReadPolicy::Compatibility);
    let compatibility_second = parse_module_detailed(text, ReadPolicy::Compatibility);
    assert_detailed_result_stable(
        "raw utf-8 text compatibility",
        &compatibility_first,
        &compatibility_second,
    );

    if compatibility_first.is_err() {
        assert!(
            strict_first.is_err(),
            "strict mode should not salvage malformed UTF-8 text after compatibility diagnostics"
        );
    }

    assert!(
        !(strict_first.is_err() && compatibility_first.is_ok()),
        "compatibility mode accepted raw UTF-8 text without diagnostics while strict mode rejected it"
    );
}

pub fn compile_raw_parser_lanes(data: &[u8]) {
    compile_raw_bitcode(data);
    compile_raw_utf8_text(data);
}

pub fn dispatch_mutation_family(module: &mut Module, kind: MutationKind, selector: u8) {
    // Keep select and memory-shape mutations deferred so failures stay attributable.
    match kind {
        MutationKind::ReferenceOrdering => mutate_reference_ordering(module, selector),
        MutationKind::PhiStructure => {
            let Some(function) = first_defined_function(module) else {
                return;
            };
            mutate_phi_structure(function, selector);
        }
        MutationKind::Dominance => {
            let Some(function) = first_defined_function(module) else {
                return;
            };
            mutate_dominance(function, selector);
        }
        MutationKind::InvalidBranchTarget => mutate_invalid_branch_target(module, selector),
        MutationKind::CallShape => mutate_call_shape(module, selector),
        MutationKind::GepShape => {
            let Some(function) = first_defined_function(module) else {
                return;
            };
            mutate_gep_shape(function, selector);
        }
        MutationKind::AttributeRef => mutate_invalid_call_site_attr_ref(module),
    }
}

fn mutate_reference_ordering(module: &mut Module, selector: u8) {
    match selector % 5 {
        0 => {
            let Some(function) = first_defined_function(module) else {
                return;
            };
            mutate_typed_local_ref_undefined(function);
        }
        1 => {
            let Some(function) = first_defined_function(module) else {
                return;
            };
            mutate_typed_local_ref_use_before_def(function);
        }
        2 => {
            let Some(function) = first_defined_function(module) else {
                return;
            };
            mutate_undefined_local_ref(function);
        }
        3 => {
            let Some(function) = first_defined_function(module) else {
                return;
            };
            mutate_local_ref_use_before_def(function);
        }
        _ => mutate_undefined_callee(module),
    }
}

fn first_defined_function(module: &mut Module) -> Option<&mut Function> {
    if let Some(index) = module
        .functions
        .iter()
        .position(|function| !function.is_declaration)
    {
        module.functions.get_mut(index)
    } else {
        module.functions.first_mut()
    }
}

fn mutate_typed_local_ref_undefined(function: &mut Function) {
    let missing_name = next_available_local_name(function, "__qir_mut_missing");
    let result_name = next_available_local_name(function, "__qir_mut_use");

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::BinOp {
                op: BinOpKind::Add,
                ty: Type::Integer(64),
                lhs: Operand::TypedLocalRef(missing_name, Type::Integer(64)),
                rhs: Operand::IntConst(Type::Integer(64), 1),
                result: result_name,
            },
        );
    }
}

fn mutate_typed_local_ref_use_before_def(function: &mut Function) {
    let late_name = next_available_local_name(function, "__qir_mut_late");
    let result_name = next_available_local_name(function, "__qir_mut_use");

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::BinOp {
                op: BinOpKind::Add,
                ty: Type::Integer(64),
                lhs: Operand::TypedLocalRef(late_name.clone(), Type::Integer(64)),
                rhs: Operand::IntConst(Type::Integer(64), 1),
                result: result_name,
            },
        );
        insert_before_terminator(
            entry_block,
            Instruction::BinOp {
                op: BinOpKind::Add,
                ty: Type::Integer(64),
                lhs: Operand::IntConst(Type::Integer(64), 2),
                rhs: Operand::IntConst(Type::Integer(64), 3),
                result: late_name,
            },
        );
    }
}

fn mutate_undefined_local_ref(function: &mut Function) {
    let missing_name = next_available_local_name(function, "__qir_mut_missing");
    let result_name = next_available_local_name(function, "__qir_mut_use");

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::BinOp {
                op: BinOpKind::Add,
                ty: Type::Integer(64),
                lhs: Operand::LocalRef(missing_name),
                rhs: Operand::IntConst(Type::Integer(64), 1),
                result: result_name,
            },
        );
    }
}

fn mutate_local_ref_use_before_def(function: &mut Function) {
    let late_name = next_available_local_name(function, "__qir_mut_late");
    let result_name = next_available_local_name(function, "__qir_mut_use");

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::BinOp {
                op: BinOpKind::Add,
                ty: Type::Integer(64),
                lhs: Operand::LocalRef(late_name.clone()),
                rhs: Operand::IntConst(Type::Integer(64), 1),
                result: result_name,
            },
        );
        insert_before_terminator(
            entry_block,
            Instruction::BinOp {
                op: BinOpKind::Add,
                ty: Type::Integer(64),
                lhs: Operand::IntConst(Type::Integer(64), 2),
                rhs: Operand::IntConst(Type::Integer(64), 3),
                result: late_name,
            },
        );
    }
}

fn mutate_undefined_callee(module: &mut Module) {
    let missing_callee = next_available_function_name(module, "__qir_mut_missing_callee");
    let Some(function) = first_defined_function(module) else {
        return;
    };

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::Call {
                return_ty: None,
                callee: missing_callee,
                args: Vec::new(),
                result: None,
                attr_refs: Vec::new(),
            },
        );
    }
}

fn mutate_call_shape(module: &mut Module, selector: u8) {
    match selector % 2 {
        0 => mutate_call_arg_operand_type_mismatch(module),
        _ => mutate_call_typed_local_type_masking(module),
    }
}

fn mutate_call_arg_operand_type_mismatch(module: &mut Module) {
    let callee = next_available_function_name(module, "__qir_mut_consume_i64");
    push_declaration(
        module,
        callee.clone(),
        Type::Void,
        vec![Param {
            ty: Type::Integer(64),
            name: Some("value".to_string()),
        }],
    );

    let Some(function) = first_defined_function(module) else {
        return;
    };

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::Call {
                return_ty: None,
                callee,
                args: vec![(Type::Integer(64), Operand::IntConst(Type::Integer(1), 1))],
                result: None,
                attr_refs: Vec::new(),
            },
        );
    }
}

fn mutate_call_typed_local_type_masking(module: &mut Module) {
    let callee = next_available_function_name(module, "__qir_mut_consume_i1");
    push_declaration(
        module,
        callee.clone(),
        Type::Void,
        vec![Param {
            ty: Type::Integer(1),
            name: Some("flag".to_string()),
        }],
    );

    let Some(function) = first_defined_function(module) else {
        return;
    };

    let value_name = next_available_local_name(function, "__qir_mut_call_value");

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::BinOp {
                op: BinOpKind::Add,
                ty: Type::Integer(64),
                lhs: Operand::IntConst(Type::Integer(64), 1),
                rhs: Operand::IntConst(Type::Integer(64), 2),
                result: value_name.clone(),
            },
        );
        insert_before_terminator(
            entry_block,
            Instruction::Call {
                return_ty: None,
                callee,
                args: vec![(
                    Type::Integer(1),
                    Operand::TypedLocalRef(value_name, Type::Integer(1)),
                )],
                result: None,
                attr_refs: Vec::new(),
            },
        );
    }
}

fn mutate_phi_structure(function: &mut Function, selector: u8) {
    match selector % 3 {
        0 => mutate_phi_predecessor_multiplicity_mismatch(function),
        1 => mutate_phi_non_predecessor_incoming(function),
        _ => mutate_phi_duplicate_incoming_diff_values(function),
    }
}

fn mutate_phi_predecessor_multiplicity_mismatch(function: &mut Function) {
    let return_type = function.return_type.clone();
    let left_block_name = next_available_block_name(function, "__qir_mut_phi_left");
    let right_block_name = next_available_block_name(function, "__qir_mut_phi_right");
    let merge_block_name = next_available_block_name(function, "__qir_mut_phi_merge");
    let phi_result = next_available_local_name(function, "__qir_mut_phi");

    if !replace_entry_terminator(
        function,
        Instruction::Br {
            cond_ty: Type::Integer(1),
            cond: Operand::IntConst(Type::Integer(1), 0),
            true_dest: left_block_name.clone(),
            false_dest: right_block_name.clone(),
        },
    ) {
        return;
    }

    function.basic_blocks.push(BasicBlock {
        name: left_block_name.clone(),
        instructions: vec![Instruction::Jump {
            dest: merge_block_name.clone(),
        }],
    });
    function.basic_blocks.push(BasicBlock {
        name: right_block_name,
        instructions: vec![Instruction::Jump {
            dest: merge_block_name.clone(),
        }],
    });
    function.basic_blocks.push(BasicBlock {
        name: merge_block_name,
        instructions: vec![
            Instruction::Phi {
                ty: Type::Integer(64),
                incoming: vec![
                    (
                        Operand::IntConst(Type::Integer(64), 0),
                        left_block_name.clone(),
                    ),
                    (Operand::IntConst(Type::Integer(64), 0), left_block_name),
                ],
                result: phi_result,
            },
            Instruction::Ret(default_return_operand(&return_type)),
        ],
    });
}

fn mutate_phi_non_predecessor_incoming(function: &mut Function) {
    let return_type = function.return_type.clone();
    let merge_block_name = next_available_block_name(function, "__qir_mut_phi_merge");
    let missing_pred_name = next_available_block_name(function, "__qir_mut_missing_pred");
    let phi_result = next_available_local_name(function, "__qir_mut_phi");

    if !replace_entry_terminator(
        function,
        Instruction::Jump {
            dest: merge_block_name.clone(),
        },
    ) {
        return;
    }

    function.basic_blocks.push(BasicBlock {
        name: merge_block_name,
        instructions: vec![
            Instruction::Phi {
                ty: Type::Integer(64),
                incoming: vec![(Operand::IntConst(Type::Integer(64), 0), missing_pred_name)],
                result: phi_result,
            },
            Instruction::Ret(default_return_operand(&return_type)),
        ],
    });
}

fn mutate_phi_duplicate_incoming_diff_values(function: &mut Function) {
    let Some(entry_block_name) = function
        .basic_blocks
        .first()
        .map(|block| block.name.clone())
    else {
        return;
    };

    let return_type = function.return_type.clone();
    let merge_block_name = next_available_block_name(function, "__qir_mut_phi_merge");
    let phi_result = next_available_local_name(function, "__qir_mut_phi");

    if !replace_entry_terminator(
        function,
        Instruction::Br {
            cond_ty: Type::Integer(1),
            cond: Operand::IntConst(Type::Integer(1), 0),
            true_dest: merge_block_name.clone(),
            false_dest: merge_block_name.clone(),
        },
    ) {
        return;
    }

    function.basic_blocks.push(BasicBlock {
        name: merge_block_name,
        instructions: vec![
            Instruction::Phi {
                ty: Type::Integer(64),
                incoming: vec![
                    (
                        Operand::IntConst(Type::Integer(64), 0),
                        entry_block_name.clone(),
                    ),
                    (Operand::IntConst(Type::Integer(64), 1), entry_block_name),
                ],
                result: phi_result,
            },
            Instruction::Ret(default_return_operand(&return_type)),
        ],
    });
}

fn mutate_dominance(function: &mut Function, selector: u8) {
    match selector % 2 {
        0 => mutate_phi_non_dominating_incoming_value(function),
        _ => mutate_cross_block_non_dominating_use(function),
    }
}

fn mutate_phi_non_dominating_incoming_value(function: &mut Function) {
    let return_type = function.return_type.clone();
    let left_block_name = next_available_block_name(function, "__qir_mut_dom_left");
    let right_block_name = next_available_block_name(function, "__qir_mut_dom_right");
    let merge_block_name = next_available_block_name(function, "__qir_mut_dom_merge");
    let value_name = next_available_local_name(function, "__qir_mut_dom_value");
    let phi_result = next_available_local_name(function, "__qir_mut_dom_phi");

    if !replace_entry_terminator(
        function,
        Instruction::Br {
            cond_ty: Type::Integer(1),
            cond: Operand::IntConst(Type::Integer(1), 0),
            true_dest: left_block_name.clone(),
            false_dest: right_block_name.clone(),
        },
    ) {
        return;
    }

    function.basic_blocks.push(BasicBlock {
        name: left_block_name.clone(),
        instructions: vec![
            Instruction::BinOp {
                op: BinOpKind::Add,
                ty: Type::Integer(64),
                lhs: Operand::IntConst(Type::Integer(64), 1),
                rhs: Operand::IntConst(Type::Integer(64), 2),
                result: value_name.clone(),
            },
            Instruction::Jump {
                dest: merge_block_name.clone(),
            },
        ],
    });
    function.basic_blocks.push(BasicBlock {
        name: right_block_name.clone(),
        instructions: vec![Instruction::Jump {
            dest: merge_block_name.clone(),
        }],
    });
    function.basic_blocks.push(BasicBlock {
        name: merge_block_name,
        instructions: vec![
            Instruction::Phi {
                ty: Type::Integer(64),
                incoming: vec![
                    (Operand::LocalRef(value_name), right_block_name),
                    (Operand::IntConst(Type::Integer(64), 0), left_block_name),
                ],
                result: phi_result,
            },
            Instruction::Ret(default_return_operand(&return_type)),
        ],
    });
}

fn mutate_cross_block_non_dominating_use(function: &mut Function) {
    let return_type = function.return_type.clone();
    let then_block_name = next_available_block_name(function, "__qir_mut_dom_then");
    let else_block_name = next_available_block_name(function, "__qir_mut_dom_else");
    let merge_block_name = next_available_block_name(function, "__qir_mut_dom_merge");
    let value_name = next_available_local_name(function, "__qir_mut_dom_value");
    let use_result = next_available_local_name(function, "__qir_mut_dom_use");

    if !replace_entry_terminator(
        function,
        Instruction::Br {
            cond_ty: Type::Integer(1),
            cond: Operand::IntConst(Type::Integer(1), 0),
            true_dest: then_block_name.clone(),
            false_dest: else_block_name.clone(),
        },
    ) {
        return;
    }

    function.basic_blocks.push(BasicBlock {
        name: then_block_name,
        instructions: vec![
            Instruction::BinOp {
                op: BinOpKind::Add,
                ty: Type::Integer(64),
                lhs: Operand::IntConst(Type::Integer(64), 1),
                rhs: Operand::IntConst(Type::Integer(64), 2),
                result: value_name.clone(),
            },
            Instruction::Jump {
                dest: merge_block_name.clone(),
            },
        ],
    });
    function.basic_blocks.push(BasicBlock {
        name: else_block_name,
        instructions: vec![Instruction::Jump {
            dest: merge_block_name.clone(),
        }],
    });
    function.basic_blocks.push(BasicBlock {
        name: merge_block_name,
        instructions: vec![
            Instruction::BinOp {
                op: BinOpKind::Add,
                ty: Type::Integer(64),
                lhs: Operand::LocalRef(value_name),
                rhs: Operand::IntConst(Type::Integer(64), 0),
                result: use_result,
            },
            Instruction::Ret(default_return_operand(&return_type)),
        ],
    });
}

fn mutate_invalid_branch_target(module: &mut Module, selector: u8) {
    let Some(function) = first_defined_function(module) else {
        return;
    };

    match selector % 4 {
        0 => mutate_invalid_conditional_branch_target(function),
        1 => mutate_invalid_jump_target(function),
        2 => mutate_invalid_switch_case_target(function),
        _ => mutate_invalid_switch_default_target(function),
    }
}

fn mutate_invalid_conditional_branch_target(function: &mut Function) {
    let Some(valid_target) = function
        .basic_blocks
        .first()
        .map(|block| block.name.clone())
    else {
        return;
    };
    let missing_block = next_available_block_name(function, "__qir_mut_missing_block");

    if let Some(entry_block) = function.basic_blocks.first_mut()
        && let Some(terminator) = entry_block.instructions.last_mut()
    {
        *terminator = Instruction::Br {
            cond_ty: Type::Integer(1),
            cond: Operand::IntConst(Type::Integer(1), 0),
            true_dest: missing_block,
            false_dest: valid_target,
        };
    }
}

fn mutate_invalid_jump_target(function: &mut Function) {
    let missing_block = next_available_block_name(function, "__qir_mut_missing_block");

    if let Some(entry_block) = function.basic_blocks.first_mut()
        && let Some(terminator) = entry_block.instructions.last_mut()
    {
        *terminator = Instruction::Jump {
            dest: missing_block,
        };
    }
}

fn mutate_invalid_switch_case_target(function: &mut Function) {
    let Some(valid_target) = function
        .basic_blocks
        .first()
        .map(|block| block.name.clone())
    else {
        return;
    };
    let missing_block = next_available_block_name(function, "__qir_mut_missing_block");

    if let Some(entry_block) = function.basic_blocks.first_mut()
        && let Some(terminator) = entry_block.instructions.last_mut()
    {
        *terminator = Instruction::Switch {
            ty: Type::Integer(64),
            value: Operand::IntConst(Type::Integer(64), 0),
            default_dest: valid_target,
            cases: vec![(1, missing_block)],
        };
    }
}

fn mutate_invalid_switch_default_target(function: &mut Function) {
    let Some(valid_target) = function
        .basic_blocks
        .first()
        .map(|block| block.name.clone())
    else {
        return;
    };
    let missing_block = next_available_block_name(function, "__qir_mut_missing_block");

    if let Some(entry_block) = function.basic_blocks.first_mut()
        && let Some(terminator) = entry_block.instructions.last_mut()
    {
        *terminator = Instruction::Switch {
            ty: Type::Integer(64),
            value: Operand::IntConst(Type::Integer(64), 0),
            default_dest: missing_block,
            cases: vec![(1, valid_target)],
        };
    }
}

fn mutate_gep_shape(function: &mut Function, selector: u8) {
    match selector % 3 {
        0 => mutate_gep_no_indices(function),
        1 => mutate_gep_non_integer_index(function),
        _ => mutate_gep_non_pointer(function),
    }
}

fn mutate_gep_no_indices(function: &mut Function) {
    let result_name = next_available_local_name(function, "__qir_mut_gep");

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::GetElementPtr {
                inbounds: true,
                pointee_ty: Type::Integer(8),
                ptr_ty: Type::Ptr,
                ptr: Operand::NullPtr,
                indices: Vec::new(),
                result: result_name,
            },
        );
    }
}

fn mutate_gep_non_integer_index(function: &mut Function) {
    let result_name = next_available_local_name(function, "__qir_mut_gep");

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::GetElementPtr {
                inbounds: true,
                pointee_ty: Type::Integer(8),
                ptr_ty: Type::Ptr,
                ptr: Operand::NullPtr,
                indices: vec![Operand::float_const(Type::Double, 0.0)],
                result: result_name,
            },
        );
    }
}

fn mutate_gep_non_pointer(function: &mut Function) {
    let result_name = next_available_local_name(function, "__qir_mut_gep");

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::GetElementPtr {
                inbounds: true,
                pointee_ty: Type::Integer(8),
                ptr_ty: Type::Integer(64),
                ptr: Operand::IntConst(Type::Integer(64), 0),
                indices: vec![Operand::IntConst(Type::Integer(32), 0)],
                result: result_name,
            },
        );
    }
}

fn mutate_invalid_call_site_attr_ref(module: &mut Module) {
    let callee = next_available_function_name(module, "__qir_mut_attr_callee");
    let missing_attr_ref = next_missing_attribute_group_id(module);
    push_declaration(module, callee.clone(), Type::Void, Vec::new());

    let Some(function) = first_defined_function(module) else {
        return;
    };

    if let Some(entry_block) = function.basic_blocks.first_mut() {
        insert_before_terminator(
            entry_block,
            Instruction::Call {
                return_ty: None,
                callee,
                args: Vec::new(),
                result: None,
                attr_refs: vec![missing_attr_ref],
            },
        );
    }
}

fn push_declaration(module: &mut Module, name: String, return_type: Type, params: Vec<Param>) {
    module.functions.push(Function {
        name,
        return_type,
        params,
        is_declaration: true,
        attribute_group_refs: Vec::new(),
        basic_blocks: Vec::new(),
    });
}

fn next_missing_attribute_group_id(module: &Module) -> u32 {
    let mut candidate = 0;

    while module
        .attribute_groups
        .iter()
        .any(|group| group.id == candidate)
    {
        candidate += 1;
    }

    candidate
}

fn default_return_operand(return_type: &Type) -> Option<Operand> {
    match return_type {
        Type::Void | Type::Array(_, _) | Type::Function(_, _) | Type::Named(_) | Type::Label => {
            None
        }
        Type::Integer(width) => Some(Operand::IntConst(Type::Integer(*width), 0)),
        Type::Double => Some(Operand::float_const(Type::Double, 0.0)),
        Type::Ptr | Type::NamedPtr(_) | Type::TypedPtr(_) => Some(Operand::NullPtr),
        Type::Half => Some(Operand::float_const(Type::Half, 0.0)),
        Type::Float => Some(Operand::float_const(Type::Float, 0.0)),
    }
}

fn replace_entry_terminator(function: &mut Function, terminator: Instruction) -> bool {
    if let Some(entry_block) = function.basic_blocks.first_mut()
        && let Some(current_terminator) = entry_block.instructions.last_mut()
    {
        *current_terminator = terminator;
        true
    } else {
        false
    }
}

fn insert_before_terminator(block: &mut BasicBlock, instruction: Instruction) {
    let insert_index = block.instructions.len().saturating_sub(1);
    block.instructions.insert(insert_index, instruction);
}

fn next_available_block_name(function: &Function, prefix: &str) -> String {
    next_available_name(prefix, |candidate| {
        function
            .basic_blocks
            .iter()
            .any(|block| block.name == candidate)
    })
}

fn next_available_function_name(module: &Module, prefix: &str) -> String {
    next_available_name(prefix, |candidate| {
        module
            .functions
            .iter()
            .any(|function| function.name == candidate)
    })
}

fn next_available_local_name(function: &Function, prefix: &str) -> String {
    next_available_name(prefix, |candidate| {
        function
            .params
            .iter()
            .any(|param| param.name.as_deref() == Some(candidate))
            || function
                .basic_blocks
                .iter()
                .flat_map(|block| block.instructions.iter())
                .filter_map(instruction_result_name)
                .any(|name| name == candidate)
    })
}

fn next_available_name(prefix: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(prefix) {
        return prefix.to_string();
    }

    for index in 0.. {
        let candidate = format!("{prefix}_{index}");
        if !exists(&candidate) {
            return candidate;
        }
    }

    unreachable!("unbounded suffix search should always find a unique name")
}

fn instruction_result_name(instruction: &Instruction) -> Option<&str> {
    match instruction {
        Instruction::BinOp { result, .. }
        | Instruction::ICmp { result, .. }
        | Instruction::FCmp { result, .. }
        | Instruction::Cast { result, .. }
        | Instruction::Call {
            result: Some(result),
            ..
        }
        | Instruction::Phi { result, .. }
        | Instruction::Alloca { result, .. }
        | Instruction::Load { result, .. }
        | Instruction::Select { result, .. }
        | Instruction::GetElementPtr { result, .. } => Some(result.as_str()),
        _ => None,
    }
}
