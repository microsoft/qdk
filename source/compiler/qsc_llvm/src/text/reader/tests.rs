// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::model::test_helpers::*;
use crate::text::writer::write_module_to_string;
use crate::{ReadDiagnosticKind, ReadPolicy};

// Helper: round-trip an instruction through write → parse → write
fn round_trip_instruction(instr: Instruction) {
    let m = single_instruction_module(instr);
    let text = write_module_to_string(&m);
    let parsed = parse_module(&text).expect("winnow parse failed");
    let text2 = write_module_to_string(&parsed);
    assert_eq!(text, text2, "text round-trip mismatch");
    assert_eq!(m, parsed, "model equality mismatch");
}

// --- Source filename ---

#[test]
fn parse_source_filename_test() {
    let input = "source_filename = \"qir\"\n";
    let m = parse_module(input).expect("parse failed");
    assert_eq!(m.source_filename.as_deref(), Some("qir"));
}

// --- Target directives ---

#[test]
fn parse_target_datalayout_test() {
    let input = "target datalayout = \"e-m:e-i64:64-f80:128\"\n";
    let m = parse_module(input).expect("parse failed");
    assert_eq!(m.target_datalayout.as_deref(), Some("e-m:e-i64:64-f80:128"));
}

#[test]
fn parse_target_triple_test() {
    let input = "target triple = \"x86_64-unknown-linux-gnu\"\n";
    let m = parse_module(input).expect("parse failed");
    assert_eq!(m.target_triple.as_deref(), Some("x86_64-unknown-linux-gnu"));
}

// --- Struct types ---

#[test]
fn parse_opaque_struct_test() {
    let input = "%Qubit = type opaque\n";
    let m = parse_module(input).expect("parse failed");
    assert_eq!(m.struct_types.len(), 1);
    assert_eq!(m.struct_types[0].name, "Qubit");
    assert!(m.struct_types[0].is_opaque);
}

// --- Global variables ---

#[test]
fn parse_global_string_constant_test() {
    let input = r#"@0 = internal constant [4 x i8] c"0_r\00""#;
    let m = parse_module(input).expect("parse failed");
    assert_eq!(m.globals.len(), 1);
    assert_eq!(m.globals[0].name, "0");
    assert!(m.globals[0].is_constant);
    assert!(matches!(m.globals[0].linkage, Linkage::Internal));
    assert_eq!(
        m.globals[0].initializer,
        Some(Constant::CString("0_r".to_string()))
    );
}

// --- Function declarations ---

#[test]
fn parse_void_single_ptr_declaration_test() {
    let input = "declare void @__quantum__rt__initialize(ptr)\n";
    let m = parse_module(input).expect("parse failed");
    assert_eq!(m.functions.len(), 1);
    let f = &m.functions[0];
    assert!(f.is_declaration);
    assert_eq!(f.name, "__quantum__rt__initialize");
    assert_eq!(f.return_type, Type::Void);
    assert_eq!(f.params.len(), 1);
    assert_eq!(f.params[0].ty, Type::Ptr);
}

#[test]
fn parse_declaration_with_attr_ref_test() {
    let input = "declare void @__quantum__qis__m__body(ptr, ptr) #1\n";
    let m = parse_module(input).expect("parse failed");
    let f = &m.functions[0];
    assert_eq!(f.attribute_group_refs, vec![1]);
}

// --- Function definitions ---

#[test]
fn parse_simple_definition_test() {
    let input = r#"define i64 @ENTRYPOINT__main() #0 {
block_0:
  ret i64 0
}
"#;
    let m = parse_module(input).expect("parse failed");
    let f = &m.functions[0];
    assert!(!f.is_declaration);
    assert_eq!(f.name, "ENTRYPOINT__main");
    assert_eq!(f.return_type, Type::Integer(64));
    assert_eq!(f.attribute_group_refs, vec![0]);
    assert_eq!(f.basic_blocks.len(), 1);
    assert_eq!(f.basic_blocks[0].name, "block_0");
}

// --- Instructions ---

#[test]
fn parse_ret_void_test() {
    let input = "define void @f() {\nentry:\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert!(matches!(instr, Instruction::Ret(None)));
}

#[test]
fn parse_ret_i64_test() {
    let input = "define i64 @f() {\nentry:\n  ret i64 0\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Ret(Some(Operand::IntConst(Type::Integer(64), 0)))
    );
}

#[test]
fn parse_br_conditional_test() {
    let input = "define void @f() {\nentry:\n  br i1 %var_0, label %block_1, label %block_2\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Br {
            cond_ty: Type::Integer(1),
            cond: Operand::LocalRef("var_0".into()),
            true_dest: "block_1".into(),
            false_dest: "block_2".into(),
        }
    );
}

#[test]
fn parse_br_bool_const_test() {
    let input = "define void @f() {\nentry:\n  br i1 true, label %block_1, label %block_2\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Br {
            cond_ty: Type::Integer(1),
            cond: Operand::IntConst(Type::Integer(1), 1),
            true_dest: "block_1".into(),
            false_dest: "block_2".into(),
        }
    );
}

#[test]
fn parse_jump_test() {
    let input = "define void @f() {\nentry:\n  br label %block_1\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Jump {
            dest: "block_1".into()
        }
    );
}

#[test]
fn parse_binop_add_test() {
    let input = "define void @f() {\nentry:\n  %var_2 = add i64 %var_0, %var_1\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::BinOp {
            op: BinOpKind::Add,
            ty: Type::Integer(64),
            lhs: Operand::LocalRef("var_0".into()),
            rhs: Operand::LocalRef("var_1".into()),
            result: "var_2".into(),
        }
    );
}

#[test]
fn parse_binop_sub_const_test() {
    let input = "define void @f() {\nentry:\n  %var_1 = sub i64 %var_0, 1\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::BinOp {
            op: BinOpKind::Sub,
            ty: Type::Integer(64),
            lhs: Operand::LocalRef("var_0".into()),
            rhs: Operand::IntConst(Type::Integer(64), 1),
            result: "var_1".into(),
        }
    );
}

#[test]
fn parse_binop_xor_bool_test() {
    let input = "define void @f() {\nentry:\n  %var_1 = xor i1 %var_0, true\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::BinOp {
            op: BinOpKind::Xor,
            ty: Type::Integer(1),
            lhs: Operand::LocalRef("var_0".into()),
            rhs: Operand::IntConst(Type::Integer(1), 1),
            result: "var_1".into(),
        }
    );
}

#[test]
fn parse_binop_fadd_test() {
    let input =
        "define void @f() {\nentry:\n  %var_2 = fadd double %var_0, %var_1\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::BinOp {
            op: BinOpKind::Fadd,
            ty: Type::Double,
            lhs: Operand::LocalRef("var_0".into()),
            rhs: Operand::LocalRef("var_1".into()),
            result: "var_2".into(),
        }
    );
}

#[test]
fn parse_icmp_eq_test() {
    let input = "define void @f() {\nentry:\n  %var_1 = icmp eq i64 %var_0, 42\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::ICmp {
            pred: IntPredicate::Eq,
            ty: Type::Integer(64),
            lhs: Operand::LocalRef("var_0".into()),
            rhs: Operand::IntConst(Type::Integer(64), 42),
            result: "var_1".into(),
        }
    );
}

#[test]
fn parse_fcmp_oeq_test() {
    let input =
        "define void @f() {\nentry:\n  %var_2 = fcmp oeq double %var_0, %var_1\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::FCmp {
            pred: FloatPredicate::Oeq,
            ty: Type::Double,
            lhs: Operand::LocalRef("var_0".into()),
            rhs: Operand::LocalRef("var_1".into()),
            result: "var_2".into(),
        }
    );
}

#[test]
fn parse_cast_sitofp_test() {
    let input =
        "define void @f() {\nentry:\n  %var_1 = sitofp i64 %var_0 to double\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Cast {
            op: CastKind::Sitofp,
            from_ty: Type::Integer(64),
            to_ty: Type::Double,
            value: Operand::LocalRef("var_0".into()),
            result: "var_1".into(),
        }
    );
}

#[test]
fn parse_call_void_test() {
    let input = "define void @f() {\nentry:\n  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Call {
            return_ty: None,
            callee: "__quantum__qis__h__body".into(),
            args: vec![(Type::Ptr, Operand::IntToPtr(0, Type::Ptr))],
            result: None,
            attr_refs: vec![],
        }
    );
}

#[test]
fn parse_call_with_return_test() {
    let input = "define void @f() {\nentry:\n  %var_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Call {
            return_ty: Some(Type::Integer(1)),
            callee: "__quantum__rt__read_result".into(),
            args: vec![(Type::Ptr, Operand::IntToPtr(0, Type::Ptr))],
            result: Some("var_0".into()),
            attr_refs: vec![],
        }
    );
}

#[test]
fn parse_call_with_attr_ref_test() {
    let input = "define void @f() {\nentry:\n  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr)) #1\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Call {
            return_ty: None,
            callee: "__quantum__qis__m__body".into(),
            args: vec![
                (Type::Ptr, Operand::IntToPtr(0, Type::Ptr)),
                (Type::Ptr, Operand::IntToPtr(0, Type::Ptr)),
            ],
            result: None,
            attr_refs: vec![1],
        }
    );
}

#[test]
fn parse_call_named_ptr_inttoptr_test() {
    let input = "%Qubit = type opaque\n\ndeclare void @takes_qubit(%Qubit*)\ndefine void @f() {\nentry:\n  call void @takes_qubit(%Qubit* inttoptr (i64 0 to %Qubit*))\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    assert_eq!(m.functions[0].params[0].ty, Type::NamedPtr("Qubit".into()));
    let instr = &m.functions[1].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Call {
            return_ty: None,
            callee: "takes_qubit".into(),
            args: vec![(
                Type::NamedPtr("Qubit".into()),
                Operand::int_to_named_ptr(0, "Qubit"),
            )],
            result: None,
            attr_refs: vec![],
        }
    );
}

#[test]
fn parse_call_named_ptr_inttoptr_mismatched_target_rejected_test() {
    let input = "%Qubit = type opaque\n%Result = type opaque\n\ndeclare void @takes_qubit(%Qubit*)\ndefine void @f() {\nentry:\n  call void @takes_qubit(%Qubit* inttoptr (i64 0 to %Result*))\n  ret void\n}\n";
    let result = parse_module(input);
    assert!(
        result.is_err(),
        "mismatched named-pointer inttoptr should fail to parse"
    );
}

#[test]
fn parse_call_with_global_ref_test() {
    let input = "define void @f() {\nentry:\n  call void @__quantum__rt__array_record_output(i64 2, ptr @0)\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Call {
            return_ty: None,
            callee: "__quantum__rt__array_record_output".into(),
            args: vec![
                (Type::Integer(64), Operand::IntConst(Type::Integer(64), 2)),
                (Type::Ptr, Operand::GlobalRef("0".into())),
            ],
            result: None,
            attr_refs: vec![],
        }
    );
}

#[test]
fn parse_phi_i1_test() {
    let input = "define void @f() {\nentry:\n  %var_3 = phi i1 [true, %block_0], [%var_2, %block_1]\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Phi {
            ty: Type::Integer(1),
            incoming: vec![
                (Operand::IntConst(Type::Integer(1), 1), "block_0".into()),
                (Operand::LocalRef("var_2".into()), "block_1".into()),
            ],
            result: "var_3".into(),
        }
    );
}

#[test]
fn parse_alloca_test() {
    let input = "define void @f() {\nentry:\n  %var_0 = alloca i1\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Alloca {
            ty: Type::Integer(1),
            result: "var_0".into(),
        }
    );
}

#[test]
fn parse_load_test() {
    let input = "define void @f() {\nentry:\n  %var_1 = load i1, ptr %var_0\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Load {
            ty: Type::Integer(1),
            ptr_ty: Type::Ptr,
            ptr: Operand::LocalRef("var_0".into()),
            result: "var_1".into(),
        }
    );
}

#[test]
fn parse_store_test() {
    let input = "define void @f() {\nentry:\n  store i1 true, ptr %var_0\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Store {
            ty: Type::Integer(1),
            value: Operand::IntConst(Type::Integer(1), 1),
            ptr_ty: Type::Ptr,
            ptr: Operand::LocalRef("var_0".into()),
        }
    );
}

#[test]
fn parse_store_half_const_test() {
    let input = "define void @f() {\nentry:\n  store half 1.5, ptr %var_0\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Store {
            ty: Type::Half,
            value: Operand::float_const(Type::Half, 1.5),
            ptr_ty: Type::Ptr,
            ptr: Operand::LocalRef("var_0".into()),
        }
    );
}

#[test]
fn parse_store_float_const_test() {
    let input = "define void @f() {\nentry:\n  store float 2.5, ptr %var_0\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Store {
            ty: Type::Float,
            value: Operand::float_const(Type::Float, 2.5),
            ptr_ty: Type::Ptr,
            ptr: Operand::LocalRef("var_0".into()),
        }
    );
}

#[test]
fn parse_store_double_const_test() {
    let input = "define void @f() {\nentry:\n  store double 3.5, ptr %var_0\n  ret void\n}\n";
    let m = parse_module(input).expect("parse failed");
    let instr = &m.functions[0].basic_blocks[0].instructions[0];
    assert_eq!(
        *instr,
        Instruction::Store {
            ty: Type::Double,
            value: Operand::float_const(Type::Double, 3.5),
            ptr_ty: Type::Ptr,
            ptr: Operand::LocalRef("var_0".into()),
        }
    );
}

// --- Attribute groups ---

#[test]
fn parse_attribute_group_test() {
    let input = r#"attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
"#;
    let m = parse_module(input).expect("parse failed");
    assert_eq!(m.attribute_groups.len(), 1);
    let ag = &m.attribute_groups[0];
    assert_eq!(ag.id, 0);
    assert_eq!(ag.attributes.len(), 5);
    assert_eq!(
        ag.attributes[0],
        Attribute::StringAttr("entry_point".into())
    );
    assert_eq!(
        ag.attributes[2],
        Attribute::KeyValue("qir_profiles".into(), "adaptive_profile".into())
    );
}

// --- Metadata ---

#[test]
fn parse_named_metadata_test() {
    let input = "!llvm.module.flags = !{!0, !1, !2}\n";
    let m = parse_module(input).expect("parse failed");
    assert_eq!(m.named_metadata.len(), 1);
    let nm = &m.named_metadata[0];
    assert_eq!(nm.name, "llvm.module.flags");
    assert_eq!(nm.node_refs, vec![0, 1, 2]);
}

#[test]
fn parse_metadata_node_int_and_string_test() {
    let input = "!0 = !{i32 1, !\"qir_major_version\", i32 2}\n";
    let m = parse_module(input).expect("parse failed");
    let node = &m.metadata_nodes[0];
    assert_eq!(node.id, 0);
    assert_eq!(node.values.len(), 3);
    assert_eq!(node.values[0], MetadataValue::Int(Type::Integer(32), 1));
    assert_eq!(
        node.values[1],
        MetadataValue::String("qir_major_version".into())
    );
    assert_eq!(node.values[2], MetadataValue::Int(Type::Integer(32), 2));
}

#[test]
fn parse_metadata_node_with_bool_test() {
    let input = "!2 = !{i32 1, !\"dynamic_qubit_management\", i1 false}\n";
    let m = parse_module(input).expect("parse failed");
    let node = &m.metadata_nodes[0];
    assert_eq!(node.values[2], MetadataValue::Int(Type::Integer(1), 0));
}

#[test]
fn parse_metadata_node_with_sublist_test() {
    let input = "!4 = !{i32 5, !\"int_computations\", !{!\"i64\"}}\n";
    let m = parse_module(input).expect("parse failed");
    let node = &m.metadata_nodes[0];
    assert_eq!(
        node.values[2],
        MetadataValue::SubList(vec![MetadataValue::String("i64".into())])
    );
}

// --- Comments ---

#[test]
fn parse_skips_comments_test() {
    let input = "; this is a comment\nsource_filename = \"qir\"\n; another comment\n";
    let m = parse_module(input).expect("parse failed");
    assert_eq!(m.source_filename.as_deref(), Some("qir"));
}

// --- Error tests ---

#[test]
fn parse_error_on_invalid_input_test() {
    let result = parse_module("invalid_keyword at module level");
    assert!(result.is_err());
}

#[test]
fn parse_module_detailed_reports_structured_error_on_invalid_input_test() {
    let diagnostics = parse_module_detailed(
        "invalid_keyword at module level",
        ReadPolicy::QirSubsetStrict,
    )
    .expect_err("invalid text IR should surface a structured diagnostic");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].kind, ReadDiagnosticKind::MalformedInput);
    assert_eq!(diagnostics[0].context, "text IR");
}

#[test]
fn strict_text_import_rejects_non_opaque_struct_body_fixture() {
    let diagnostics =
        parse_module_detailed("%Pair = type { i64, i64 }\n", ReadPolicy::QirSubsetStrict)
            .expect_err("non-opaque struct bodies should remain unsupported");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].kind, ReadDiagnosticKind::MalformedInput);
    assert_eq!(diagnostics[0].context, "text IR");
}

// --- Round-trip tests ---

#[test]
fn round_trip_empty_module_test() {
    let m = empty_module();
    let text = write_module_to_string(&m);
    let parsed = parse_module(&text).expect("parse failed");
    let text2 = write_module_to_string(&parsed);
    assert_eq!(text, text2);
}

#[test]
fn round_trip_source_filename_test() {
    let input = "source_filename = \"qir\"\n";
    let m = parse_module(input).expect("parse failed");
    let text = write_module_to_string(&m);
    let m2 = parse_module(&text).expect("parse failed");
    assert_eq!(m, m2);
}

#[test]
fn round_trip_global_variables_test() {
    let input = r#"@0 = internal constant [4 x i8] c"0_r\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
"#;
    let m = parse_module(input).expect("parse failed");
    let text = write_module_to_string(&m);
    let m2 = parse_module(&text).expect("parse failed");
    assert_eq!(m, m2);
}

#[test]
fn round_trip_ret_void_test() {
    round_trip_instruction(Instruction::Ret(None));
}

#[test]
fn round_trip_ret_value_test() {
    round_trip_instruction(Instruction::Ret(Some(Operand::IntConst(
        Type::Integer(64),
        0,
    ))));
}

#[test]
fn round_trip_binop_add_test() {
    round_trip_instruction(Instruction::BinOp {
        op: BinOpKind::Add,
        ty: Type::Integer(64),
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::LocalRef("var_1".into()),
        result: "var_2".into(),
    });
}

#[test]
fn round_trip_icmp_test() {
    round_trip_instruction(Instruction::ICmp {
        pred: IntPredicate::Eq,
        ty: Type::Integer(64),
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::IntConst(Type::Integer(64), 42),
        result: "var_1".into(),
    });
}

#[test]
fn round_trip_fcmp_test() {
    round_trip_instruction(Instruction::FCmp {
        pred: FloatPredicate::Oeq,
        ty: Type::Double,
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::LocalRef("var_1".into()),
        result: "var_2".into(),
    });
}

#[test]
fn round_trip_cast_test() {
    round_trip_instruction(Instruction::Cast {
        op: CastKind::Sitofp,
        from_ty: Type::Integer(64),
        to_ty: Type::Double,
        value: Operand::LocalRef("var_0".into()),
        result: "var_1".into(),
    });
}

#[test]
fn round_trip_call_void_test() {
    round_trip_instruction(Instruction::Call {
        return_ty: None,
        callee: "__quantum__qis__h__body".into(),
        args: vec![(Type::Ptr, Operand::IntToPtr(0, Type::Ptr))],
        result: None,
        attr_refs: vec![],
    });
}

#[test]
fn round_trip_call_with_return_test() {
    round_trip_instruction(Instruction::Call {
        return_ty: Some(Type::Integer(1)),
        callee: "__quantum__rt__read_result".into(),
        args: vec![(Type::Ptr, Operand::IntToPtr(0, Type::Ptr))],
        result: Some("var_0".into()),
        attr_refs: vec![],
    });
}

#[test]
fn round_trip_phi_test() {
    round_trip_instruction(Instruction::Phi {
        ty: Type::Integer(1),
        incoming: vec![
            (Operand::IntConst(Type::Integer(1), 1), "block_0".into()),
            (Operand::LocalRef("var_2".into()), "block_1".into()),
        ],
        result: "var_3".into(),
    });
}

#[test]
fn round_trip_alloca_test() {
    round_trip_instruction(Instruction::Alloca {
        ty: Type::Integer(1),
        result: "var_0".into(),
    });
}

#[test]
fn round_trip_load_test() {
    round_trip_instruction(Instruction::Load {
        ty: Type::Integer(1),
        ptr_ty: Type::Ptr,
        ptr: Operand::LocalRef("var_0".into()),
        result: "var_1".into(),
    });
}

#[test]
fn round_trip_store_test() {
    round_trip_instruction(Instruction::Store {
        ty: Type::Integer(1),
        value: Operand::IntConst(Type::Integer(1), 1),
        ptr_ty: Type::Ptr,
        ptr: Operand::LocalRef("var_0".into()),
    });
}

#[test]
fn round_trip_select_test() {
    round_trip_instruction(Instruction::Select {
        cond: Operand::LocalRef("var_0".into()),
        true_val: Operand::IntConst(Type::Integer(64), 1),
        false_val: Operand::IntConst(Type::Integer(64), 2),
        ty: Type::Integer(64),
        result: "var_1".into(),
    });
}

#[test]
fn round_trip_switch_test() {
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
                    instructions: vec![Instruction::Switch {
                        ty: Type::Integer(64),
                        value: Operand::LocalRef("var_0".into()),
                        default_dest: "block_default".into(),
                        cases: vec![(0, "block_0".into()), (1, "block_1".into())],
                    }],
                },
                BasicBlock {
                    name: "block_0".to_string(),
                    instructions: vec![Instruction::Ret(None)],
                },
                BasicBlock {
                    name: "block_1".to_string(),
                    instructions: vec![Instruction::Ret(None)],
                },
                BasicBlock {
                    name: "block_default".to_string(),
                    instructions: vec![Instruction::Ret(None)],
                },
            ],
        }],
        attribute_groups: Vec::new(),
        named_metadata: Vec::new(),
        metadata_nodes: Vec::new(),
    };
    let text = write_module_to_string(&m);
    let parsed = parse_module(&text).expect("winnow parse failed");
    let text2 = write_module_to_string(&parsed);
    assert_eq!(text, text2);
    assert_eq!(m, parsed);
}

#[test]
fn round_trip_unreachable_test() {
    round_trip_instruction(Instruction::Unreachable);
}

#[test]
fn round_trip_bell_module_v2_test() {
    let m = bell_module_v2();
    let text = write_module_to_string(&m);
    let parsed = parse_module(&text).expect("winnow parse failed");
    let text2 = write_module_to_string(&parsed);
    assert_eq!(text, text2);
    assert_eq!(m, parsed);
}
