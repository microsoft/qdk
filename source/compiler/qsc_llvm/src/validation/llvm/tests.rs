// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::model::Type;
use crate::model::{
    Attribute, AttributeGroup, BasicBlock, Function, IntPredicate, MetadataNode, Module,
    NamedMetadata, Param, StructType,
};

// -----------------------------------------------------------------------
// Baseline helpers
// -----------------------------------------------------------------------

fn valid_module() -> Module {
    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".to_string(),
            return_type: Type::Integer(64),
            params: vec![Param {
                ty: Type::Integer(64),
                name: Some("x".to_string()),
            }],
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![BasicBlock {
                name: "entry".to_string(),
                instructions: vec![
                    Instruction::BinOp {
                        op: BinOpKind::Add,
                        ty: Type::Integer(64),
                        lhs: Operand::LocalRef("x".to_string()),
                        rhs: Operand::IntConst(Type::Integer(64), 1),
                        result: "sum".to_string(),
                    },
                    Instruction::Ret(Some(Operand::LocalRef("sum".to_string()))),
                ],
            }],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    }
}

fn valid_switch_module() -> Module {
    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".to_string(),
            return_type: Type::Void,
            params: vec![Param {
                ty: Type::Integer(64),
                name: Some("x".to_string()),
            }],
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![
                BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Switch {
                        ty: Type::Integer(64),
                        value: Operand::LocalRef("x".to_string()),
                        default_dest: "default".to_string(),
                        cases: vec![(0, "zero".to_string())],
                    }],
                },
                BasicBlock {
                    name: "zero".to_string(),
                    instructions: vec![Instruction::Ret(None)],
                },
                BasicBlock {
                    name: "default".to_string(),
                    instructions: vec![Instruction::Ret(None)],
                },
            ],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    }
}

fn valid_alloca_module() -> Module {
    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".to_string(),
            return_type: Type::Void,
            params: Vec::new(),
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![BasicBlock {
                name: "entry".to_string(),
                instructions: vec![
                    Instruction::Alloca {
                        ty: Type::Integer(64),
                        result: "slot".to_string(),
                    },
                    Instruction::Ret(None),
                ],
            }],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    }
}

fn two_block_module_with_phi(phi_instr: Instruction) -> Module {
    Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".to_string(),
            return_type: Type::Void,
            params: Vec::new(),
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![
                BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Jump {
                        dest: "merge".to_string(),
                    }],
                },
                BasicBlock {
                    name: "merge".to_string(),
                    instructions: vec![phi_instr, Instruction::Ret(None)],
                },
            ],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    }
}

fn has_error<F: Fn(&LlvmIrError) -> bool>(errors: &[LlvmIrError], pred: F) -> bool {
    errors.iter().any(pred)
}

fn declaration(name: &str, return_type: Type, params: Vec<Param>) -> Function {
    Function {
        name: name.to_string(),
        return_type,
        params,
        is_declaration: true,
        attribute_group_refs: Vec::new(),
        basic_blocks: Vec::new(),
    }
}

fn typed_ptr(inner: Type) -> Type {
    Type::TypedPtr(Box::new(inner))
}

// -----------------------------------------------------------------------
// Step 5.6: Positive baseline test
// -----------------------------------------------------------------------

#[test]
fn valid_module_passes() {
    let m = valid_module();
    let errors = validate_ir(&m);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
}

// -----------------------------------------------------------------------
// Step 5.2: Structure and terminator tests
// -----------------------------------------------------------------------

#[test]
fn missing_basic_blocks() {
    let mut m = valid_module();
    m.functions[0].is_declaration = false;
    m.functions[0].basic_blocks = vec![];
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::MissingBasicBlocks { .. }
    )));
}

#[test]
fn declaration_has_blocks() {
    let mut m = valid_module();
    m.functions[0].is_declaration = true;
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::DeclarationHasBlocks { .. }
    )));
}

#[test]
fn empty_basic_block() {
    let mut m = valid_module();
    m.functions[0].basic_blocks.push(BasicBlock {
        name: "empty_bb".to_string(),
        instructions: vec![],
    });
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::EmptyBasicBlock { .. }
    )));
}

#[test]
fn missing_terminator() {
    let mut m = valid_module();
    // Replace the Ret with a non-terminator
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.pop(); // remove Ret
    bb.instructions.push(Instruction::BinOp {
        op: BinOpKind::Add,
        ty: Type::Integer(64),
        lhs: Operand::IntConst(Type::Integer(64), 0),
        rhs: Operand::IntConst(Type::Integer(64), 1),
        result: "no_term".to_string(),
    });
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::MissingTerminator { .. }
    )));
}

#[test]
fn mid_block_terminator() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    // Insert Unreachable before the final Ret
    bb.instructions
        .insert(bb.instructions.len() - 1, Instruction::Unreachable);
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::MidBlockTerminator { .. }
    )));
}

// -----------------------------------------------------------------------
// Step 5.3: SSA and reference validation tests
// -----------------------------------------------------------------------

#[test]
fn duplicate_definition() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    // Insert a second instruction with the same result name "sum"
    bb.instructions.insert(
        0,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::LocalRef("x".to_string()),
            rhs: Operand::IntConst(Type::Integer(64), 2),
            result: "sum".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::DuplicateDefinition { .. }
    )));
}

#[test]
fn undefined_local_ref() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::LocalRef("nonexistent".to_string()),
            rhs: Operand::IntConst(Type::Integer(64), 1),
            result: "undef_use".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::UndefinedLocalRef { .. }
    )));
}

#[test]
fn typed_local_ref_undefined_local() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::TypedLocalRef("typed_missing".to_string(), Type::Integer(64)),
            rhs: Operand::IntConst(Type::Integer(64), 1),
            result: "typed_undef_use".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::UndefinedLocalRef { name, .. } if name == "typed_missing"
    )));
}

#[test]
fn typed_local_ref_use_before_definition() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::TypedLocalRef("late".to_string(), Type::Integer(64)),
            rhs: Operand::IntConst(Type::Integer(64), 1),
            result: "early_use".to_string(),
        },
    );
    bb.instructions.insert(
        1,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::IntConst(Type::Integer(64), 2),
            rhs: Operand::IntConst(Type::Integer(64), 3),
            result: "late".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::UndefinedLocalRef { name, .. } if name == "late"
    )));
}

#[test]
fn local_ref_use_before_definition() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::LocalRef("late".to_string()),
            rhs: Operand::IntConst(Type::Integer(64), 1),
            result: "early_use".to_string(),
        },
    );
    bb.instructions.insert(
        1,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::IntConst(Type::Integer(64), 2),
            rhs: Operand::IntConst(Type::Integer(64), 3),
            result: "late".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::UndefinedLocalRef { name, .. } if name == "late"
    )));
}

#[test]
fn alloca_result_used_before_definition() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::Load {
            ty: Type::Integer(64),
            ptr_ty: Type::Ptr,
            ptr: Operand::LocalRef("stack_slot".to_string()),
            result: "loaded".to_string(),
        },
    );
    bb.instructions.insert(
        1,
        Instruction::Alloca {
            ty: Type::Integer(64),
            result: "stack_slot".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::UndefinedLocalRef { name, .. } if name == "stack_slot"
    )));
}

#[test]
fn undefined_global_ref() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::Load {
            ty: Type::Integer(64),
            ptr_ty: Type::Ptr,
            ptr: Operand::GlobalRef("missing_global".to_string()),
            result: "loaded".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::UndefinedGlobalRef { name, .. } if name == "missing_global"
    )));
}

#[test]
fn invalid_branch_target() {
    let mut m = valid_module();
    m.functions[0].basic_blocks[0].instructions = vec![Instruction::Br {
        cond_ty: Type::Integer(1),
        cond: Operand::IntConst(Type::Integer(1), 0),
        true_dest: "no_such_block".to_string(),
        false_dest: "entry".to_string(),
    }];
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::InvalidBranchTarget { .. }
    )));
}

#[test]
fn invalid_jump_target() {
    let mut m = valid_module();
    m.functions[0].basic_blocks[0].instructions = vec![Instruction::Jump {
        dest: "no_such_block".to_string(),
    }];
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::InvalidBranchTarget { target, .. } if target == "no_such_block"
    )));
}

#[test]
fn invalid_switch_target() {
    let mut m = valid_module();
    m.functions[0].basic_blocks[0].instructions = vec![Instruction::Switch {
        ty: Type::Integer(64),
        value: Operand::IntConst(Type::Integer(64), 0),
        default_dest: "entry".to_string(),
        cases: vec![(1, "no_such_block".to_string())],
    }];
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::InvalidBranchTarget { target, .. } if target == "no_such_block"
    )));
}

#[test]
fn invalid_switch_default_target() {
    let mut m = valid_switch_module();
    m.functions[0].basic_blocks[0].instructions[0] = Instruction::Switch {
        ty: Type::Integer(64),
        value: Operand::LocalRef("x".to_string()),
        default_dest: "no_such_block".to_string(),
        cases: vec![(0, "zero".to_string())],
    };
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::InvalidBranchTarget { target, .. } if target == "no_such_block"
    )));
}

#[test]
fn valid_switch_passes() {
    let m = valid_switch_module();
    let errors = validate_ir(&m);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
}

#[test]
fn switch_type_not_integer() {
    let mut m = valid_switch_module();
    m.functions[0].basic_blocks[0].instructions[0] = Instruction::Switch {
        ty: Type::Double,
        value: Operand::float_const(Type::Double, 0.0),
        default_dest: "default".to_string(),
        cases: vec![(0, "zero".to_string())],
    };

    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::SwitchTypeNotInteger { found_type, .. } if found_type == "double"
    )));
}

#[test]
fn switch_selector_type_mismatch() {
    let mut m = valid_switch_module();
    m.functions[0].basic_blocks[0].instructions[0] = Instruction::Switch {
        ty: Type::Integer(32),
        value: Operand::LocalRef("x".to_string()),
        default_dest: "default".to_string(),
        cases: vec![(0, "zero".to_string())],
    };

    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::TypeMismatch {
            instruction,
            expected,
            found,
            ..
        } if instruction == "Switch" && expected == "i32" && found == "i64"
    )));
}

#[test]
fn switch_duplicate_case_value() {
    let mut m = valid_switch_module();
    m.functions[0].basic_blocks[0].instructions[0] = Instruction::Switch {
        ty: Type::Integer(64),
        value: Operand::LocalRef("x".to_string()),
        default_dest: "default".to_string(),
        cases: vec![(0, "zero".to_string()), (0, "default".to_string())],
    };

    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::SwitchDuplicateCaseValue { case_value: 0, .. }
    )));
}

#[test]
fn valid_alloca_passes() {
    let m = valid_alloca_module();
    let errors = validate_ir(&m);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
}

#[test]
fn alloca_void_type() {
    let mut m = valid_alloca_module();
    m.functions[0].basic_blocks[0].instructions[0] = Instruction::Alloca {
        ty: Type::Void,
        result: "void_slot".to_string(),
    };

    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::AllocaUnsizedType { result, .. } if result == "void_slot"
    )));
}

#[test]
fn alloca_function_type() {
    let mut m = valid_alloca_module();
    m.functions[0].basic_blocks[0].instructions[0] = Instruction::Alloca {
        ty: Type::Function(Box::new(Type::Void), vec![Type::Integer(64)]),
        result: "fn_slot".to_string(),
    };

    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::AllocaUnsizedType { result, .. } if result == "fn_slot"
    )));
}

#[test]
fn alloca_named_opaque_struct_type() {
    let mut m = valid_alloca_module();
    m.struct_types.push(StructType {
        name: "Opaque".to_string(),
        is_opaque: true,
    });
    m.functions[0].basic_blocks[0].instructions[0] = Instruction::Alloca {
        ty: Type::Named("Opaque".to_string()),
        result: "opaque_slot".to_string(),
    };

    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::AllocaUnsizedType { result, .. } if result == "opaque_slot"
    )));
}

#[test]
fn undefined_callee() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::Call {
            return_ty: Some(Type::Void),
            callee: "nonexistent_fn".to_string(),
            args: vec![],
            result: None,
            attr_refs: vec![],
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::UndefinedCallee { .. }
    )));
}

#[test]
fn bitcode_roundtrip_call_preserves_callee_name_for_validator() {
    use crate::{parse_bitcode, write_bitcode};

    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "callee".to_string(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Ret(None)],
                }],
            },
            Function {
                name: "caller".to_string(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![
                        Instruction::Call {
                            return_ty: None,
                            callee: "callee".to_string(),
                            args: vec![],
                            result: None,
                            attr_refs: vec![],
                        },
                        Instruction::Ret(None),
                    ],
                }],
            },
        ],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let orig_errors = validate_ir(&m);
    assert!(orig_errors.is_empty(), "original: {orig_errors:?}");

    let bc = write_bitcode(&m);
    let parsed = parse_bitcode(&bc).expect("parse failed");

    let caller = parsed
        .functions
        .iter()
        .find(|function| function.name == "caller")
        .expect("missing caller function");
    assert!(matches!(
        &caller.basic_blocks[0].instructions[0],
        Instruction::Call { callee, .. } if callee == "callee"
    ));

    let rt_errors = validate_ir(&parsed);
    assert!(rt_errors.is_empty(), "round-tripped: {rt_errors:?}");
}

#[test]
fn bitcode_roundtrip_call_preserves_attr_refs_for_validator() {
    use crate::{parse_bitcode, write_bitcode};

    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "callee".to_string(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "caller".to_string(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![
                        Instruction::Call {
                            return_ty: None,
                            callee: "callee".to_string(),
                            args: Vec::new(),
                            result: None,
                            attr_refs: vec![0, 1],
                        },
                        Instruction::Ret(None),
                    ],
                }],
            },
        ],
        attribute_groups: vec![
            AttributeGroup {
                id: 0,
                attributes: vec![Attribute::StringAttr("alwaysinline".to_string())],
            },
            AttributeGroup {
                id: 1,
                attributes: vec![Attribute::StringAttr("noreturn".to_string())],
            },
        ],
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let orig_errors = validate_ir(&m);
    assert!(orig_errors.is_empty(), "original: {orig_errors:?}");

    let bc = write_bitcode(&m);
    let parsed = parse_bitcode(&bc).expect("parse failed");

    let caller = parsed
        .functions
        .iter()
        .find(|function| function.name == "caller")
        .expect("missing caller function");
    assert!(matches!(
        &caller.basic_blocks[0].instructions[0],
        Instruction::Call { attr_refs, .. } if attr_refs == &vec![0, 1]
    ));

    let rt_errors = validate_ir(&parsed);
    assert!(rt_errors.is_empty(), "round-tripped: {rt_errors:?}");
}

#[test]
fn bitcode_roundtrip_global_ref_preserves_name_for_validator() {
    use crate::model::{Constant, GlobalVariable, Linkage};
    use crate::{parse_bitcode, write_bitcode};

    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: vec![GlobalVariable {
            name: "message".to_string(),
            ty: Type::Array(5, Box::new(Type::Integer(8))),
            linkage: Linkage::Internal,
            is_constant: true,
            initializer: Some(Constant::CString("hello".to_string())),
        }],
        functions: vec![
            Function {
                name: "use_ptr".to_string(),
                return_type: Type::Void,
                params: vec![Param {
                    ty: Type::Ptr,
                    name: None,
                }],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "caller".to_string(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![
                        Instruction::Call {
                            return_ty: None,
                            callee: "use_ptr".to_string(),
                            args: vec![(Type::Ptr, Operand::GlobalRef("message".to_string()))],
                            result: None,
                            attr_refs: vec![],
                        },
                        Instruction::Ret(None),
                    ],
                }],
            },
        ],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let orig_errors = validate_ir(&m);
    assert!(orig_errors.is_empty(), "original: {orig_errors:?}");

    let bc = write_bitcode(&m);
    let parsed = parse_bitcode(&bc).expect("parse failed");

    let caller = parsed
        .functions
        .iter()
        .find(|function| function.name == "caller")
        .expect("missing caller function");
    assert!(matches!(
        &caller.basic_blocks[0].instructions[0],
        Instruction::Call { args, .. }
            if matches!(args.first(), Some((Type::Ptr, Operand::GlobalRef(name))) if name == "message")
    ));

    let rt_errors = validate_ir(&parsed);
    assert!(rt_errors.is_empty(), "round-tripped: {rt_errors:?}");
}

#[test]
fn bitcode_roundtrip_preserves_local_names_for_validator() {
    use crate::{parse_bitcode, write_bitcode};

    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "chooser".to_string(),
            return_type: Type::Integer(64),
            params: vec![
                Param {
                    ty: Type::Integer(64),
                    name: Some("a".to_string()),
                },
                Param {
                    ty: Type::Integer(64),
                    name: Some("b".to_string()),
                },
            ],
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![
                BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![
                        Instruction::ICmp {
                            pred: IntPredicate::Slt,
                            ty: Type::Integer(64),
                            lhs: Operand::LocalRef("a".to_string()),
                            rhs: Operand::LocalRef("b".to_string()),
                            result: "cond".to_string(),
                        },
                        Instruction::Br {
                            cond_ty: Type::Integer(1),
                            cond: Operand::LocalRef("cond".to_string()),
                            true_dest: "then".to_string(),
                            false_dest: "else".to_string(),
                        },
                    ],
                },
                BasicBlock {
                    name: "then".to_string(),
                    instructions: vec![
                        Instruction::BinOp {
                            op: BinOpKind::Add,
                            ty: Type::Integer(64),
                            lhs: Operand::LocalRef("a".to_string()),
                            rhs: Operand::IntConst(Type::Integer(64), 1),
                            result: "then_value".to_string(),
                        },
                        Instruction::Jump {
                            dest: "merge".to_string(),
                        },
                    ],
                },
                BasicBlock {
                    name: "else".to_string(),
                    instructions: vec![
                        Instruction::BinOp {
                            op: BinOpKind::Add,
                            ty: Type::Integer(64),
                            lhs: Operand::LocalRef("b".to_string()),
                            rhs: Operand::IntConst(Type::Integer(64), 2),
                            result: "else_value".to_string(),
                        },
                        Instruction::Jump {
                            dest: "merge".to_string(),
                        },
                    ],
                },
                BasicBlock {
                    name: "merge".to_string(),
                    instructions: vec![
                        Instruction::Phi {
                            ty: Type::Integer(64),
                            incoming: vec![
                                (
                                    Operand::LocalRef("then_value".to_string()),
                                    "then".to_string(),
                                ),
                                (
                                    Operand::LocalRef("else_value".to_string()),
                                    "else".to_string(),
                                ),
                            ],
                            result: "result".to_string(),
                        },
                        Instruction::Ret(Some(Operand::LocalRef("result".to_string()))),
                    ],
                },
            ],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let orig_errors = validate_ir(&m);
    assert!(orig_errors.is_empty(), "original: {orig_errors:?}");

    let bc = write_bitcode(&m);
    let parsed = parse_bitcode(&bc).expect("parse failed");
    let chooser = parsed
        .functions
        .iter()
        .find(|function| function.name == "chooser")
        .expect("missing chooser function");

    assert_eq!(chooser.params[0].name.as_deref(), Some("a"));
    assert_eq!(chooser.params[1].name.as_deref(), Some("b"));
    assert_eq!(
        chooser
            .basic_blocks
            .iter()
            .map(|bb| bb.name.as_str())
            .collect::<Vec<_>>(),
        vec!["entry", "then", "else", "merge"]
    );
    assert!(matches!(
        &chooser.basic_blocks[0].instructions[0],
        Instruction::ICmp { lhs, rhs, result, .. }
            if result == "cond"
                && matches!(lhs, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "a")
                && matches!(rhs, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "b")
    ));
    assert!(matches!(
        &chooser.basic_blocks[0].instructions[1],
        Instruction::Br {
            cond,
            true_dest,
            false_dest,
            ..
        } if true_dest == "then"
            && false_dest == "else"
            && matches!(cond, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "cond")
    ));
    assert!(matches!(
        &chooser.basic_blocks[1].instructions[0],
        Instruction::BinOp { lhs, result, .. }
            if result == "then_value"
                && matches!(lhs, Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) if name == "a")
    ));
    assert!(matches!(
        &chooser.basic_blocks[1].instructions[1],
        Instruction::Jump { dest } if dest == "merge"
    ));
    assert!(matches!(
        &chooser.basic_blocks[3].instructions[0],
        Instruction::Phi {
            incoming,
            result,
            ..
        } if result == "result"
            && incoming.len() == 2
            && matches!(&incoming[0], (Operand::LocalRef(name) | Operand::TypedLocalRef(name, _), from) if name == "then_value" && from == "then")
            && matches!(&incoming[1], (Operand::LocalRef(name) | Operand::TypedLocalRef(name, _), from) if name == "else_value" && from == "else")
    ));
    assert!(matches!(
        &chooser.basic_blocks[3].instructions[1],
        Instruction::Ret(Some(Operand::LocalRef(name) | Operand::TypedLocalRef(name, _))) if name == "result"
    ));

    let rt_errors = validate_ir(&parsed);
    assert!(rt_errors.is_empty(), "round-tripped: {rt_errors:?}");
}

#[test]
#[allow(clippy::too_many_lines)]
fn bitcode_roundtrip_preserves_float_local_ref_types_for_validator() {
    use crate::{parse_bitcode, write_bitcode};

    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "use_double".to_string(),
                return_type: Type::Void,
                params: vec![Param {
                    ty: Type::Double,
                    name: None,
                }],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "test".to_string(),
                return_type: Type::Void,
                params: vec![
                    Param {
                        ty: Type::Double,
                        name: Some("a".to_string()),
                    },
                    Param {
                        ty: Type::Double,
                        name: Some("b".to_string()),
                    },
                ],
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![
                        Instruction::BinOp {
                            op: BinOpKind::Fadd,
                            ty: Type::Double,
                            lhs: Operand::TypedLocalRef("a".to_string(), Type::Double),
                            rhs: Operand::TypedLocalRef("b".to_string(), Type::Double),
                            result: "r".to_string(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "use_double".to_string(),
                            args: vec![(
                                Type::Double,
                                Operand::TypedLocalRef("r".to_string(), Type::Double),
                            )],
                            result: None,
                            attr_refs: vec![],
                        },
                        Instruction::Ret(None),
                    ],
                }],
            },
        ],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let orig_errors = validate_ir(&m);
    assert!(orig_errors.is_empty(), "original: {orig_errors:?}");

    let bc = write_bitcode(&m);
    let parsed = parse_bitcode(&bc).expect("parse failed");

    let test_fn = parsed
        .functions
        .iter()
        .find(|function| function.name == "test")
        .expect("missing test function");
    assert_eq!(test_fn.params[0].name.as_deref(), Some("a"));
    assert_eq!(test_fn.params[1].name.as_deref(), Some("b"));
    assert!(matches!(
        &test_fn.basic_blocks[0].instructions[0],
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty,
            lhs,
            rhs,
            result,
        } if ty == &Type::Double
            && result == "r"
            && matches!(lhs, Operand::TypedLocalRef(name, local_ty) if name == "a" && local_ty == &Type::Double)
            && matches!(rhs, Operand::TypedLocalRef(name, local_ty) if name == "b" && local_ty == &Type::Double)
    ));
    assert!(matches!(
        &test_fn.basic_blocks[0].instructions[1],
        Instruction::Call {
            return_ty: None,
            args,
            result: None,
            ..
        } if matches!(
            args.as_slice(),
            [(Type::Double, Operand::TypedLocalRef(name, ty))]
                if name == "r" && ty == &Type::Double
        )
    ));

    let rt_errors = validate_ir(&parsed);
    assert!(rt_errors.is_empty(), "round-tripped: {rt_errors:?}");
}

#[test]
fn bitcode_roundtrip_preserves_load_local_ref_types_for_validator() {
    use crate::{parse_bitcode, write_bitcode};

    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test".to_string(),
            return_type: Type::Integer(64),
            params: Vec::new(),
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![BasicBlock {
                name: "entry".to_string(),
                instructions: vec![
                    Instruction::Alloca {
                        ty: Type::Integer(64),
                        result: "ptr".to_string(),
                    },
                    Instruction::Load {
                        ty: Type::Integer(64),
                        ptr_ty: Type::Ptr,
                        ptr: Operand::TypedLocalRef("ptr".to_string(), Type::Ptr),
                        result: "val".to_string(),
                    },
                    Instruction::Ret(Some(Operand::TypedLocalRef(
                        "val".to_string(),
                        Type::Integer(64),
                    ))),
                ],
            }],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let orig_errors = validate_ir(&m);
    assert!(orig_errors.is_empty(), "original: {orig_errors:?}");

    let bc = write_bitcode(&m);
    let parsed = parse_bitcode(&bc).expect("parse failed");

    let test_fn = parsed
        .functions
        .iter()
        .find(|function| function.name == "test")
        .expect("missing test function");
    assert!(matches!(
        &test_fn.basic_blocks[0].instructions[0],
        Instruction::Alloca { ty, result } if ty == &Type::Integer(64) && result == "ptr"
    ));
    assert!(matches!(
        &test_fn.basic_blocks[0].instructions[1],
        Instruction::Load {
            ty,
            ptr_ty,
            ptr,
            result,
        } if ty == &Type::Integer(64)
            && ptr_ty == &Type::Ptr
            && result == "val"
            && matches!(ptr, Operand::TypedLocalRef(name, local_ty) if name == "ptr" && local_ty == &Type::Ptr)
    ));
    assert!(matches!(
        &test_fn.basic_blocks[0].instructions[2],
        Instruction::Ret(Some(Operand::TypedLocalRef(name, ty)))
            if name == "val" && ty == &Type::Integer(64)
    ));

    let rt_errors = validate_ir(&parsed);
    assert!(rt_errors.is_empty(), "round-tripped: {rt_errors:?}");
}

// -----------------------------------------------------------------------
// Step 5.4: Type consistency and cast validation tests
// -----------------------------------------------------------------------

#[test]
fn binop_type_mismatch() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::IntConst(Type::Integer(32), 1),
            rhs: Operand::IntConst(Type::Integer(64), 2),
            result: "mismatch".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::TypeMismatch { .. }
    )));
}

#[test]
fn int_op_on_float() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Double,
            lhs: Operand::float_const(Type::Double, 1.0),
            rhs: Operand::float_const(Type::Double, 2.0),
            result: "bad_int_op".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::IntOpOnNonInt { .. }
    )));
}

#[test]
fn float_op_on_int() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty: Type::Integer(64),
            lhs: Operand::IntConst(Type::Integer(64), 1),
            rhs: Operand::IntConst(Type::Integer(64), 2),
            result: "bad_float_op".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::FloatOpOnNonFloat { .. }
    )));
}

#[test]
fn br_cond_not_i1() {
    let mut m = valid_module();
    m.functions[0].basic_blocks[0].instructions = vec![Instruction::Br {
        cond_ty: Type::Integer(32),
        cond: Operand::IntConst(Type::Integer(32), 0),
        true_dest: "entry".to_string(),
        false_dest: "entry".to_string(),
    }];
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::BrCondNotI1 { .. }
    )));
}

#[test]
fn ret_type_mismatch() {
    let mut m = valid_module();
    // Function returns i64 but we return a double
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.pop(); // remove Ret
    bb.instructions
        .push(Instruction::Ret(Some(Operand::float_const(
            Type::Double,
            1.0,
        ))));
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::RetTypeMismatch { .. }
    )));
}

#[test]
#[allow(clippy::too_many_lines)]
fn widened_float_surface_accepts_half_float_and_double() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![
            declaration(
                "consume_half",
                Type::Void,
                vec![Param {
                    ty: Type::Half,
                    name: Some("value".to_string()),
                }],
            ),
            declaration(
                "consume_float",
                Type::Void,
                vec![Param {
                    ty: Type::Float,
                    name: Some("value".to_string()),
                }],
            ),
            declaration(
                "consume_double",
                Type::Void,
                vec![Param {
                    ty: Type::Double,
                    name: Some("value".to_string()),
                }],
            ),
            Function {
                name: "test_fn".to_string(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![
                        Instruction::BinOp {
                            op: BinOpKind::Fadd,
                            ty: Type::Half,
                            lhs: Operand::float_const(Type::Half, 1.5),
                            rhs: Operand::float_const(Type::Half, 2.25),
                            result: "half_sum".to_string(),
                        },
                        Instruction::Cast {
                            op: CastKind::FpExt,
                            from_ty: Type::Half,
                            to_ty: Type::Float,
                            value: Operand::LocalRef("half_sum".to_string()),
                            result: "as_float".to_string(),
                        },
                        Instruction::BinOp {
                            op: BinOpKind::Fadd,
                            ty: Type::Float,
                            lhs: Operand::LocalRef("as_float".to_string()),
                            rhs: Operand::float_const(Type::Float, 0.5),
                            result: "float_sum".to_string(),
                        },
                        Instruction::FCmp {
                            pred: crate::model::FloatPredicate::Oeq,
                            ty: Type::Float,
                            lhs: Operand::LocalRef("float_sum".to_string()),
                            rhs: Operand::float_const(Type::Float, 4.25),
                            result: "float_eq".to_string(),
                        },
                        Instruction::Cast {
                            op: CastKind::FpExt,
                            from_ty: Type::Float,
                            to_ty: Type::Double,
                            value: Operand::LocalRef("float_sum".to_string()),
                            result: "as_double".to_string(),
                        },
                        Instruction::Cast {
                            op: CastKind::FpTrunc,
                            from_ty: Type::Double,
                            to_ty: Type::Half,
                            value: Operand::LocalRef("as_double".to_string()),
                            result: "back_to_half".to_string(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "consume_half".to_string(),
                            args: vec![(Type::Half, Operand::LocalRef("back_to_half".to_string()))],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "consume_float".to_string(),
                            args: vec![(Type::Float, Operand::LocalRef("float_sum".to_string()))],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Call {
                            return_ty: None,
                            callee: "consume_double".to_string(),
                            args: vec![(Type::Double, Operand::LocalRef("as_double".to_string()))],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Ret(None),
                    ],
                }],
            },
        ],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let errors = validate_ir(&m);
    assert!(
        errors.is_empty(),
        "valid half/float/double flow failed: {errors:?}"
    );
}

#[test]
fn typed_local_ref_use_site_type_masking() {
    let mut m = valid_module();
    m.functions.push(declaration(
        "consume_i1",
        Type::Void,
        vec![Param {
            ty: Type::Integer(1),
            name: Some("flag".to_string()),
        }],
    ));

    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::IntConst(Type::Integer(64), 1),
            rhs: Operand::IntConst(Type::Integer(64), 2),
            result: "wide".to_string(),
        },
    );
    bb.instructions.insert(
        1,
        Instruction::Call {
            return_ty: None,
            callee: "consume_i1".to_string(),
            args: vec![(
                Type::Integer(1),
                Operand::TypedLocalRef("wide".to_string(), Type::Integer(1)),
            )],
            result: None,
            attr_refs: Vec::new(),
        },
    );

    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::TypeMismatch {
            instruction,
            expected,
            found,
            ..
        } if instruction == "Call @consume_i1" && expected == "i1" && found == "i64"
    )));
}

#[test]
fn load_pointer_operand_mismatch() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::Load {
            ty: Type::Integer(64),
            ptr_ty: typed_ptr(Type::Integer(64)),
            ptr: Operand::IntToPtr(0, typed_ptr(Type::Integer(8))),
            result: "loaded".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::TypeMismatch { instruction, .. } if instruction == "Load"
    )));
}

#[test]
fn store_pointer_operand_mismatch() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::Store {
            ty: Type::Integer(64),
            value: Operand::IntConst(Type::Integer(64), 0),
            ptr_ty: typed_ptr(Type::Integer(64)),
            ptr: Operand::IntToPtr(0, typed_ptr(Type::Integer(8))),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::TypeMismatch { instruction, .. } if instruction == "Store"
    )));
}

#[test]
fn call_arg_operand_type_mismatch() {
    let mut m = valid_module();
    m.functions.push(declaration(
        "consume_i64",
        Type::Void,
        vec![Param {
            ty: Type::Integer(64),
            name: Some("value".to_string()),
        }],
    ));

    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::Call {
            return_ty: None,
            callee: "consume_i64".to_string(),
            args: vec![(Type::Integer(64), Operand::IntConst(Type::Integer(1), 1))],
            result: None,
            attr_refs: Vec::new(),
        },
    );

    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::TypeMismatch {
            instruction,
            expected,
            found,
            ..
        } if instruction == "Call @consume_i64" && expected == "i64" && found == "i1"
    )));
}

#[test]
fn invalid_cast_trunc_wider() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::Cast {
            op: CastKind::Trunc,
            from_ty: Type::Integer(32),
            to_ty: Type::Integer(64),
            value: Operand::IntConst(Type::Integer(32), 0),
            result: "bad_trunc".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::InvalidCast { .. }
    )));
}

#[test]
fn valid_cast_zext() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::Cast {
            op: CastKind::Zext,
            from_ty: Type::Integer(32),
            to_ty: Type::Integer(64),
            value: Operand::IntConst(Type::Integer(32), 0),
            result: "good_zext".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(
        !has_error(&errors, |e| matches!(e, LlvmIrError::InvalidCast { .. })),
        "zext i32→i64 should be valid, but got: {errors:?}"
    );
}

#[test]
fn invalid_cast_int_to_ptr_non_int() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::Cast {
            op: CastKind::IntToPtr,
            from_ty: Type::Double,
            to_ty: Type::Ptr,
            value: Operand::float_const(Type::Double, 0.0),
            result: "bad_inttoptr".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::InvalidCast { .. }
    )));
}

// -----------------------------------------------------------------------
// Step 5.5: PHI validation tests
// -----------------------------------------------------------------------

#[test]
fn phi_not_at_block_start() {
    let mut m = two_block_module_with_phi(Instruction::Phi {
        ty: Type::Integer(64),
        incoming: vec![(Operand::IntConst(Type::Integer(64), 0), "entry".to_string())],
        result: "p".to_string(),
    });
    // Insert a non-PHI instruction before the PHI in block "merge"
    let merge = &mut m.functions[0].basic_blocks[1];
    merge.instructions.insert(
        0,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::IntConst(Type::Integer(64), 0),
            rhs: Operand::IntConst(Type::Integer(64), 1),
            result: "dummy".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::PhiNotAtBlockStart { .. }
    )));
}

#[test]
fn phi_void_type() {
    let m = two_block_module_with_phi(Instruction::Phi {
        ty: Type::Void,
        incoming: vec![(Operand::IntConst(Type::Integer(64), 0), "entry".to_string())],
        result: "p".to_string(),
    });
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::PhiVoidType { .. }
    )));
}

#[test]
fn phi_in_entry_block() {
    let mut m = valid_module();
    // Put a PHI node in the entry block
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::Phi {
            ty: Type::Integer(64),
            incoming: vec![],
            result: "phi_entry".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::PhiInEntryBlock { .. }
    )));
}

#[test]
fn phi_backedge_value_defined_later_in_same_block_is_valid() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".to_string(),
            return_type: Type::Integer(64),
            params: Vec::new(),
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![
                BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Jump {
                        dest: "loop".to_string(),
                    }],
                },
                BasicBlock {
                    name: "loop".to_string(),
                    instructions: vec![
                        Instruction::Phi {
                            ty: Type::Integer(64),
                            incoming: vec![
                                (Operand::IntConst(Type::Integer(64), 0), "entry".to_string()),
                                (Operand::LocalRef("next".to_string()), "loop".to_string()),
                            ],
                            result: "acc".to_string(),
                        },
                        Instruction::BinOp {
                            op: BinOpKind::Add,
                            ty: Type::Integer(64),
                            lhs: Operand::LocalRef("acc".to_string()),
                            rhs: Operand::IntConst(Type::Integer(64), 1),
                            result: "next".to_string(),
                        },
                        Instruction::ICmp {
                            pred: IntPredicate::Slt,
                            ty: Type::Integer(64),
                            lhs: Operand::LocalRef("next".to_string()),
                            rhs: Operand::IntConst(Type::Integer(64), 10),
                            result: "cond".to_string(),
                        },
                        Instruction::Br {
                            cond_ty: Type::Integer(1),
                            cond: Operand::LocalRef("cond".to_string()),
                            true_dest: "loop".to_string(),
                            false_dest: "exit".to_string(),
                        },
                    ],
                },
                BasicBlock {
                    name: "exit".to_string(),
                    instructions: vec![Instruction::Ret(Some(Operand::LocalRef(
                        "next".to_string(),
                    )))],
                },
            ],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let errors = validate_ir(&m);
    assert!(
        errors.is_empty(),
        "expected no validation errors, got: {errors:?}"
    );
}

#[test]
fn phi_pred_count_mismatch() {
    // "merge" has 1 predecessor ("entry"), but PHI has 2 incoming entries
    let m = two_block_module_with_phi(Instruction::Phi {
        ty: Type::Integer(64),
        incoming: vec![
            (Operand::IntConst(Type::Integer(64), 0), "entry".to_string()),
            (Operand::IntConst(Type::Integer(64), 1), "other".to_string()),
        ],
        result: "p".to_string(),
    });
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::PhiPredCountMismatch { .. }
    )));
}

#[test]
fn phi_incoming_not_predecessor() {
    // "merge" has 1 predecessor ("entry"), list only a non-predecessor label
    let m = two_block_module_with_phi(Instruction::Phi {
        ty: Type::Integer(64),
        incoming: vec![(
            Operand::IntConst(Type::Integer(64), 0),
            "no_such_pred".to_string(),
        )],
        result: "p".to_string(),
    });
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::PhiIncomingNotPredecessor { .. }
    )));
}

#[test]
fn phi_predecessor_multiset_mismatch() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".to_string(),
            return_type: Type::Void,
            params: Vec::new(),
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![
                BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Br {
                        cond_ty: Type::Integer(1),
                        cond: Operand::IntConst(Type::Integer(1), 0),
                        true_dest: "left".to_string(),
                        false_dest: "right".to_string(),
                    }],
                },
                BasicBlock {
                    name: "left".to_string(),
                    instructions: vec![Instruction::Jump {
                        dest: "merge".to_string(),
                    }],
                },
                BasicBlock {
                    name: "right".to_string(),
                    instructions: vec![Instruction::Jump {
                        dest: "merge".to_string(),
                    }],
                },
                BasicBlock {
                    name: "merge".to_string(),
                    instructions: vec![
                        Instruction::Phi {
                            ty: Type::Integer(64),
                            incoming: vec![
                                (Operand::IntConst(Type::Integer(64), 0), "left".to_string()),
                                (Operand::IntConst(Type::Integer(64), 0), "left".to_string()),
                            ],
                            result: "p".to_string(),
                        },
                        Instruction::Ret(None),
                    ],
                },
            ],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::PhiIncomingNotPredecessor {
            incoming_block,
            ..
        } if incoming_block == "left"
    )));
}

#[test]
fn phi_duplicate_block_diff_value() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".to_string(),
            return_type: Type::Void,
            params: Vec::new(),
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![
                BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Br {
                        cond_ty: Type::Integer(1),
                        cond: Operand::IntConst(Type::Integer(1), 0),
                        true_dest: "merge".to_string(),
                        false_dest: "merge".to_string(),
                    }],
                },
                BasicBlock {
                    name: "merge".to_string(),
                    instructions: vec![
                        Instruction::Phi {
                            ty: Type::Integer(64),
                            incoming: vec![
                                (Operand::IntConst(Type::Integer(64), 0), "entry".to_string()),
                                (Operand::IntConst(Type::Integer(64), 1), "entry".to_string()),
                            ],
                            result: "p".to_string(),
                        },
                        Instruction::Ret(None),
                    ],
                },
            ],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::PhiDuplicateBlockDiffValue { dup_block, .. } if dup_block == "entry"
    )));
}

// -----------------------------------------------------------------------
// Step 5.5: GEP validation tests
// -----------------------------------------------------------------------

#[test]
fn gep_no_indices() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::GetElementPtr {
            inbounds: true,
            pointee_ty: Type::Integer(8),
            ptr_ty: Type::Ptr,
            ptr: Operand::NullPtr,
            indices: vec![],
            result: "gep_empty".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::GepNoIndices { .. }
    )));
}

#[test]
fn gep_void_pointee() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::GetElementPtr {
            inbounds: true,
            pointee_ty: Type::Void,
            ptr_ty: Type::Ptr,
            ptr: Operand::NullPtr,
            indices: vec![Operand::IntConst(Type::Integer(32), 0)],
            result: "gep_void".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::UnsizedPointeeType { .. }
    )));
}

#[test]
fn gep_non_ptr() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::GetElementPtr {
            inbounds: true,
            pointee_ty: Type::Integer(8),
            ptr_ty: Type::Integer(64),
            ptr: Operand::IntConst(Type::Integer(64), 0),
            indices: vec![Operand::IntConst(Type::Integer(32), 0)],
            result: "gep_notptr".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::PtrExpected { .. }
    )));
}

#[test]
fn gep_non_integer_index() {
    let mut m = valid_module();
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::GetElementPtr {
            inbounds: true,
            pointee_ty: Type::Integer(8),
            ptr_ty: Type::Ptr,
            ptr: Operand::NullPtr,
            indices: vec![Operand::float_const(Type::Double, 0.0)],
            result: "gep_bad_index".to_string(),
        },
    );
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::TypeMismatch {
            instruction,
            expected,
            ..
        } if instruction == "GetElementPtr" && expected == "integer type"
    )));
}

// -----------------------------------------------------------------------
// Step 8.6: Dominance validation tests
// -----------------------------------------------------------------------

#[test]
fn cross_block_non_dominating_use() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".to_string(),
            return_type: Type::Void,
            params: vec![],
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![
                BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Br {
                        cond_ty: Type::Integer(1),
                        cond: Operand::IntConst(Type::Integer(1), 0),
                        true_dest: "then_bb".to_string(),
                        false_dest: "else_bb".to_string(),
                    }],
                },
                BasicBlock {
                    name: "then_bb".to_string(),
                    instructions: vec![
                        Instruction::BinOp {
                            op: BinOpKind::Add,
                            ty: Type::Integer(64),
                            lhs: Operand::IntConst(Type::Integer(64), 1),
                            rhs: Operand::IntConst(Type::Integer(64), 2),
                            result: "x".to_string(),
                        },
                        Instruction::Jump {
                            dest: "merge".to_string(),
                        },
                    ],
                },
                BasicBlock {
                    name: "else_bb".to_string(),
                    instructions: vec![Instruction::Jump {
                        dest: "merge".to_string(),
                    }],
                },
                BasicBlock {
                    name: "merge".to_string(),
                    instructions: vec![
                        Instruction::BinOp {
                            op: BinOpKind::Add,
                            ty: Type::Integer(64),
                            lhs: Operand::LocalRef("x".to_string()),
                            rhs: Operand::IntConst(Type::Integer(64), 0),
                            result: "y".to_string(),
                        },
                        Instruction::Ret(None),
                    ],
                },
            ],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::UseNotDominatedByDef { .. }
    )));
}

#[test]
fn phi_from_non_dominating_block() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".to_string(),
            return_type: Type::Void,
            params: vec![],
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![
                BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Br {
                        cond_ty: Type::Integer(1),
                        cond: Operand::IntConst(Type::Integer(1), 0),
                        true_dest: "left".to_string(),
                        false_dest: "right".to_string(),
                    }],
                },
                BasicBlock {
                    name: "left".to_string(),
                    instructions: vec![
                        Instruction::BinOp {
                            op: BinOpKind::Add,
                            ty: Type::Integer(64),
                            lhs: Operand::IntConst(Type::Integer(64), 1),
                            rhs: Operand::IntConst(Type::Integer(64), 2),
                            result: "x".to_string(),
                        },
                        Instruction::Jump {
                            dest: "merge".to_string(),
                        },
                    ],
                },
                BasicBlock {
                    name: "right".to_string(),
                    instructions: vec![Instruction::Jump {
                        dest: "merge".to_string(),
                    }],
                },
                BasicBlock {
                    name: "merge".to_string(),
                    instructions: vec![
                        Instruction::Phi {
                            ty: Type::Integer(64),
                            incoming: vec![
                                (Operand::LocalRef("x".to_string()), "right".to_string()),
                                (Operand::IntConst(Type::Integer(64), 0), "left".to_string()),
                            ],
                            result: "p".to_string(),
                        },
                        Instruction::Ret(None),
                    ],
                },
            ],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::UseNotDominatedByDef { .. }
    )));
}

#[test]
fn valid_diamond_with_phi() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".to_string(),
            return_type: Type::Void,
            params: vec![],
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![
                BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Br {
                        cond_ty: Type::Integer(1),
                        cond: Operand::IntConst(Type::Integer(1), 0),
                        true_dest: "then_bb".to_string(),
                        false_dest: "else_bb".to_string(),
                    }],
                },
                BasicBlock {
                    name: "then_bb".to_string(),
                    instructions: vec![
                        Instruction::BinOp {
                            op: BinOpKind::Add,
                            ty: Type::Integer(64),
                            lhs: Operand::IntConst(Type::Integer(64), 1),
                            rhs: Operand::IntConst(Type::Integer(64), 2),
                            result: "x".to_string(),
                        },
                        Instruction::Jump {
                            dest: "merge".to_string(),
                        },
                    ],
                },
                BasicBlock {
                    name: "else_bb".to_string(),
                    instructions: vec![
                        Instruction::BinOp {
                            op: BinOpKind::Add,
                            ty: Type::Integer(64),
                            lhs: Operand::IntConst(Type::Integer(64), 3),
                            rhs: Operand::IntConst(Type::Integer(64), 4),
                            result: "y_val".to_string(),
                        },
                        Instruction::Jump {
                            dest: "merge".to_string(),
                        },
                    ],
                },
                BasicBlock {
                    name: "merge".to_string(),
                    instructions: vec![
                        Instruction::Phi {
                            ty: Type::Integer(64),
                            incoming: vec![
                                (Operand::LocalRef("x".to_string()), "then_bb".to_string()),
                                (
                                    Operand::LocalRef("y_val".to_string()),
                                    "else_bb".to_string(),
                                ),
                            ],
                            result: "p".to_string(),
                        },
                        Instruction::Ret(None),
                    ],
                },
            ],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };
    let errors = validate_ir(&m);
    assert!(
        !has_error(&errors, |e| matches!(
            e,
            LlvmIrError::UseNotDominatedByDef { .. }
        )),
        "valid diamond with PHI should not report dominance errors: {errors:?}"
    );
}

#[test]
fn unreachable_block_def() {
    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".to_string(),
            return_type: Type::Void,
            params: vec![],
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![
                BasicBlock {
                    name: "entry".to_string(),
                    instructions: vec![Instruction::Jump {
                        dest: "reachable".to_string(),
                    }],
                },
                BasicBlock {
                    name: "unreachable_bb".to_string(),
                    instructions: vec![
                        Instruction::BinOp {
                            op: BinOpKind::Add,
                            ty: Type::Integer(64),
                            lhs: Operand::IntConst(Type::Integer(64), 1),
                            rhs: Operand::IntConst(Type::Integer(64), 2),
                            result: "x".to_string(),
                        },
                        Instruction::Jump {
                            dest: "reachable".to_string(),
                        },
                    ],
                },
                BasicBlock {
                    name: "reachable".to_string(),
                    instructions: vec![
                        Instruction::BinOp {
                            op: BinOpKind::Add,
                            ty: Type::Integer(64),
                            lhs: Operand::LocalRef("x".to_string()),
                            rhs: Operand::IntConst(Type::Integer(64), 0),
                            result: "z".to_string(),
                        },
                        Instruction::Ret(None),
                    ],
                },
            ],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::UseNotDominatedByDef { .. }
    )));
}

#[test]
fn dominance_helpers_ignore_unreachable_blocks() {
    let func = Function {
        name: "test_fn".to_string(),
        return_type: Type::Void,
        params: Vec::new(),
        is_declaration: false,
        attribute_group_refs: Vec::new(),
        basic_blocks: vec![
            BasicBlock {
                name: "entry".to_string(),
                instructions: vec![Instruction::Jump {
                    dest: "reachable".to_string(),
                }],
            },
            BasicBlock {
                name: "unreachable_bb".to_string(),
                instructions: vec![Instruction::Jump {
                    dest: "reachable".to_string(),
                }],
            },
            BasicBlock {
                name: "reachable".to_string(),
                instructions: vec![Instruction::Ret(None)],
            },
        ],
    };

    let (successors, predecessors) = build_cfg(&func);
    let entry = func.basic_blocks[0].name.as_str();
    let rpo = reverse_postorder(entry, &successors);
    let idom = compute_dominators(entry, &rpo, &predecessors);

    assert!(!rpo.contains(&"unreachable_bb"));
    assert_eq!(idom.get("reachable").copied(), Some("entry"));
    assert!(dominates("entry", "reachable", &idom, entry));
    assert!(!dominates("entry", "unreachable_bb", &idom, entry));
}

// -----------------------------------------------------------------------
// Step 9.5: Attribute group and metadata validation tests
// -----------------------------------------------------------------------

#[test]
fn duplicate_attribute_group_id() {
    let mut m = valid_module();
    m.attribute_groups = vec![
        AttributeGroup {
            id: 0,
            attributes: Vec::new(),
        },
        AttributeGroup {
            id: 0,
            attributes: Vec::new(),
        },
    ];
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::DuplicateAttributeGroupId { id: 0 }
    )));
}

#[test]
fn invalid_attribute_group_ref() {
    let mut m = valid_module();
    m.attribute_groups = vec![AttributeGroup {
        id: 0,
        attributes: Vec::new(),
    }];
    m.functions[0].attribute_group_refs = vec![99];
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::InvalidAttributeGroupRef { ref_id: 99, .. }
    )));
}

#[test]
fn invalid_call_site_attribute_group_ref() {
    let mut m = valid_module();
    m.functions
        .push(declaration("callee", Type::Void, Vec::new()));
    let bb = &mut m.functions[0].basic_blocks[0];
    bb.instructions.insert(
        0,
        Instruction::Call {
            return_ty: None,
            callee: "callee".to_string(),
            args: Vec::new(),
            result: None,
            attr_refs: vec![99],
        },
    );

    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::InvalidAttributeGroupRef { ref_id: 99, .. }
    )));
}

#[test]
fn duplicate_metadata_node_id() {
    let mut m = valid_module();
    m.metadata_nodes = vec![
        MetadataNode {
            id: 0,
            values: Vec::new(),
        },
        MetadataNode {
            id: 0,
            values: Vec::new(),
        },
    ];
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::DuplicateMetadataNodeId { id: 0 }
    )));
}

#[test]
fn invalid_metadata_node_ref() {
    let mut m = valid_module();
    m.named_metadata = vec![NamedMetadata {
        name: "llvm.module.flags".to_string(),
        node_refs: vec![42],
    }];
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::InvalidMetadataNodeRef { ref_id: 42, .. }
    )));
}

#[test]
fn metadata_cycle() {
    let mut m = valid_module();
    m.metadata_nodes = vec![
        MetadataNode {
            id: 0,
            values: vec![MetadataValue::NodeRef(1)],
        },
        MetadataNode {
            id: 1,
            values: vec![MetadataValue::NodeRef(0)],
        },
    ];
    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::MetadataRefCycle { .. }
    )));
}

#[test]
fn valid_metadata_passes() {
    let mut m = valid_module();
    m.attribute_groups = vec![
        AttributeGroup {
            id: 0,
            attributes: Vec::new(),
        },
        AttributeGroup {
            id: 1,
            attributes: Vec::new(),
        },
    ];
    m.functions[0].attribute_group_refs = vec![0];
    m.metadata_nodes = vec![
        MetadataNode {
            id: 0,
            values: vec![MetadataValue::Int(Type::Integer(32), 1)],
        },
        MetadataNode {
            id: 1,
            values: vec![MetadataValue::NodeRef(0)],
        },
    ];
    m.named_metadata = vec![NamedMetadata {
        name: "llvm.module.flags".to_string(),
        node_refs: vec![0, 1],
    }];
    let errors = validate_ir(&m);
    assert!(
        !has_error(&errors, |e| matches!(
            e,
            LlvmIrError::DuplicateAttributeGroupId { .. }
                | LlvmIrError::InvalidAttributeGroupRef { .. }
                | LlvmIrError::DuplicateMetadataNodeId { .. }
                | LlvmIrError::InvalidMetadataNodeRef { .. }
                | LlvmIrError::MetadataRefCycle { .. }
        )),
        "valid metadata and attributes should not trigger errors: {errors:?}"
    );
}

// -----------------------------------------------------------------------
// Step 11.1: Bitcode round-trip NamedPtr regression test
// -----------------------------------------------------------------------

#[test]
fn bitcode_roundtrip_preserves_named_ptr_types() {
    use crate::{parse_bitcode, write_bitcode};

    let m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: vec![StructType {
            name: "Qubit".into(),
            is_opaque: true,
        }],
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_func".into(),
            return_type: Type::Void,
            params: vec![Param {
                ty: Type::NamedPtr("Qubit".into()),
                name: Some("q".into()),
            }],
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![BasicBlock {
                name: "entry".into(),
                instructions: vec![Instruction::Ret(None)],
            }],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let orig_errors = validate_ir(&m);
    assert!(orig_errors.is_empty(), "original: {orig_errors:?}");

    let bc = write_bitcode(&m);
    let parsed = parse_bitcode(&bc).expect("parse failed");

    assert_eq!(
        m.functions[0].params[0].ty,
        parsed.functions[0].params[0].ty
    );

    let rt_errors = validate_ir(&parsed);
    assert!(rt_errors.is_empty(), "round-tripped: {rt_errors:?}");
}

#[test]
fn named_ptr_inttoptr_cast_rejects_named_target_and_accepts_named_ptr_target() {
    let mut m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: vec![StructType {
            name: "Qubit".into(),
            is_opaque: true,
        }],
        globals: Vec::new(),
        functions: vec![Function {
            name: "test_fn".into(),
            return_type: Type::Void,
            params: Vec::new(),
            is_declaration: false,
            attribute_group_refs: Vec::new(),
            basic_blocks: vec![BasicBlock {
                name: "entry".into(),
                instructions: vec![
                    Instruction::Cast {
                        op: CastKind::IntToPtr,
                        from_ty: Type::Integer(64),
                        to_ty: Type::Named("Qubit".into()),
                        value: Operand::IntConst(Type::Integer(64), 0),
                        result: "bad_qubit".into(),
                    },
                    Instruction::Ret(None),
                ],
            }],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::InvalidCast {
            cast_kind,
            from_ty,
            to_ty,
            ..
        } if cast_kind == "IntToPtr" && from_ty == "i64" && to_ty == "%Qubit"
    )));

    m.functions[0].basic_blocks[0].instructions[0] = Instruction::Cast {
        op: CastKind::IntToPtr,
        from_ty: Type::Integer(64),
        to_ty: Type::NamedPtr("Qubit".into()),
        value: Operand::IntConst(Type::Integer(64), 0),
        result: "good_qubit".into(),
    };

    let errors = validate_ir(&m);
    assert!(
        errors.is_empty(),
        "valid named-pointer inttoptr cast failed: {errors:?}"
    );
}

#[test]
fn named_ptr_call_rejects_named_inttoptr_operand() {
    let qubit_ty = Type::NamedPtr("Qubit".into());
    let mut m = Module {
        source_filename: None,
        target_datalayout: None,
        target_triple: None,
        struct_types: vec![StructType {
            name: "Qubit".into(),
            is_opaque: true,
        }],
        globals: Vec::new(),
        functions: vec![
            Function {
                name: "callee".into(),
                return_type: Type::Void,
                params: vec![Param {
                    ty: qubit_ty.clone(),
                    name: None,
                }],
                is_declaration: true,
                attribute_group_refs: Vec::new(),
                basic_blocks: Vec::new(),
            },
            Function {
                name: "caller".into(),
                return_type: Type::Void,
                params: Vec::new(),
                is_declaration: false,
                attribute_group_refs: Vec::new(),
                basic_blocks: vec![BasicBlock {
                    name: "entry".into(),
                    instructions: vec![
                        Instruction::Call {
                            return_ty: None,
                            callee: "callee".into(),
                            args: vec![(
                                qubit_ty,
                                Operand::IntToPtr(0, Type::Named("Qubit".into())),
                            )],
                            result: None,
                            attr_refs: Vec::new(),
                        },
                        Instruction::Ret(None),
                    ],
                }],
            },
        ],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };

    let errors = validate_ir(&m);
    assert!(has_error(&errors, |e| matches!(
        e,
        LlvmIrError::TypeMismatch {
            instruction,
            expected,
            found,
            ..
        } if instruction == "Call @callee" && expected == "%Qubit*" && found == "%Qubit"
    )));

    m.functions[1].basic_blocks[0].instructions[0] = Instruction::Call {
        return_ty: None,
        callee: "callee".into(),
        args: vec![(
            Type::NamedPtr("Qubit".into()),
            Operand::int_to_named_ptr(0, "Qubit"),
        )],
        result: None,
        attr_refs: Vec::new(),
    };

    let errors = validate_ir(&m);
    assert!(
        errors.is_empty(),
        "valid named-pointer call failed: {errors:?}"
    );
}
