// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::expect;
use test_helpers::{empty_module, single_instruction_module};

#[test]
fn display_void() {
    expect!["void"].assert_eq(&Type::Void.to_string());
}

#[test]
fn display_integer_1() {
    expect!["i1"].assert_eq(&Type::Integer(1).to_string());
}

#[test]
fn display_integer_64() {
    expect!["i64"].assert_eq(&Type::Integer(64).to_string());
}

#[test]
fn display_double() {
    expect!["double"].assert_eq(&Type::Double.to_string());
}

#[test]
fn display_ptr() {
    expect!["ptr"].assert_eq(&Type::Ptr.to_string());
}

#[test]
fn display_named_ptr() {
    expect!["%Qubit*"].assert_eq(&Type::NamedPtr("Qubit".to_string()).to_string());
}

#[test]
fn display_array() {
    expect!["[4 x i8]"].assert_eq(&Type::Array(4, Box::new(Type::Integer(8))).to_string());
}

#[test]
fn display_function_no_params() {
    expect!["void ()"].assert_eq(&Type::Function(Box::new(Type::Void), vec![]).to_string());
}

#[test]
fn display_function_with_params() {
    expect!["void (ptr, ptr)"]
        .assert_eq(&Type::Function(Box::new(Type::Void), vec![Type::Ptr, Type::Ptr]).to_string());
}

#[test]
fn display_function_with_return() {
    expect!["i1 (ptr)"]
        .assert_eq(&Type::Function(Box::new(Type::Integer(1)), vec![Type::Ptr]).to_string());
}

#[test]
fn display_named() {
    expect!["%Qubit"].assert_eq(&Type::Named("Qubit".to_string()).to_string());
}

#[test]
fn display_named_result() {
    expect!["%Result"].assert_eq(&Type::Named("Result".to_string()).to_string());
}

#[test]
fn equality() {
    assert_eq!(Type::Integer(32), Type::Integer(32));
    assert_ne!(Type::Integer(32), Type::Integer(64));
    assert_ne!(Type::Ptr, Type::Double);
}

#[test]
fn clone_preserves_equality() {
    let ty = Type::Array(8, Box::new(Type::Integer(8)));
    let cloned = ty.clone();
    assert_eq!(ty, cloned);
}

#[test]
fn empty_module_has_no_functions() {
    let m = empty_module();
    assert!(m.functions.is_empty());
    assert!(m.globals.is_empty());
    assert!(m.struct_types.is_empty());
}

#[test]
fn single_instruction_module_has_one_function() {
    let m = single_instruction_module(Instruction::Ret(None));
    assert_eq!(m.functions.len(), 1);
    assert_eq!(m.functions[0].name, "test_fn");
    assert_eq!(m.functions[0].basic_blocks.len(), 1);
    assert_eq!(m.functions[0].basic_blocks[0].instructions.len(), 2);
}

#[test]
fn module_clone_preserves_equality() {
    let m = single_instruction_module(Instruction::Ret(Some(Operand::IntConst(
        Type::Integer(64),
        0,
    ))));
    let cloned = m.clone();
    assert_eq!(m, cloned);
}

#[test]
fn instruction_debug_format() {
    let instr = Instruction::BinOp {
        op: BinOpKind::Add,
        ty: Type::Integer(64),
        lhs: Operand::LocalRef("a".to_string()),
        rhs: Operand::IntConst(Type::Integer(64), 1),
        result: "sum".to_string(),
    };
    let debug_str = format!("{instr:?}");
    assert!(debug_str.contains("BinOp"));
    assert!(debug_str.contains("Add"));
}

#[test]
fn call_instruction_construction() {
    let call = Instruction::Call {
        return_ty: None,
        callee: "__quantum__qis__h__body".to_string(),
        args: vec![(Type::Ptr, Operand::IntToPtr(0, Type::Ptr))],
        result: None,
        attr_refs: Vec::new(),
    };
    assert!(matches!(call, Instruction::Call { .. }));
}

#[test]
fn global_variable_construction() {
    let g = GlobalVariable {
        name: "0".to_string(),
        ty: Type::Array(4, Box::new(Type::Integer(8))),
        linkage: Linkage::Internal,
        is_constant: true,
        initializer: Some(Constant::CString("0_r".to_string())),
    };
    assert!(g.is_constant);
    assert_eq!(g.linkage, Linkage::Internal);
}

#[test]
fn metadata_construction() {
    let named = NamedMetadata {
        name: "llvm.module.flags".to_string(),
        node_refs: vec![0, 1],
    };
    let node = MetadataNode {
        id: 0,
        values: vec![
            MetadataValue::Int(Type::Integer(32), 1),
            MetadataValue::String("qir_major_version".to_string()),
            MetadataValue::Int(Type::Integer(32), 1),
        ],
    };
    assert_eq!(named.node_refs.len(), 2);
    assert_eq!(node.values.len(), 3);
}
