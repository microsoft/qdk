// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::model::test_helpers::*;
use crate::model::*;
use expect_test::expect;

// --- Struct type emission ---

#[test]
fn struct_type_opaque() {
    let st = StructType {
        name: "Qubit".into(),
        is_opaque: true,
    };
    let mut buf = String::new();
    write_struct_type(&mut buf, &st).expect("failed to write");
    expect!["%Qubit = type opaque"].assert_eq(&buf);
}

// --- Global variable emission ---

#[test]
fn global_string_constant() {
    let g = GlobalVariable {
        name: "0".into(),
        ty: Type::Array(4, Box::new(Type::Integer(8))),
        linkage: Linkage::Internal,
        is_constant: true,
        initializer: Some(Constant::CString("0_r".into())),
    };
    let mut buf = String::new();
    write_global(&mut buf, &g).expect("failed to write");
    expect![r#"@0 = internal constant [4 x i8] c"0_r\00""#].assert_eq(&buf);
}

#[test]
fn global_string_longer() {
    let g = GlobalVariable {
        name: "1".into(),
        ty: Type::Array(6, Box::new(Type::Integer(8))),
        linkage: Linkage::Internal,
        is_constant: true,
        initializer: Some(Constant::CString("1_a0r".into())),
    };
    let mut buf = String::new();
    write_global(&mut buf, &g).expect("failed to write");
    expect![r#"@1 = internal constant [6 x i8] c"1_a0r\00""#].assert_eq(&buf);
}

// --- Function declaration emission ---

#[test]
fn void_single_ptr_declaration() {
    let f = Function {
        name: "__quantum__rt__initialize".into(),
        return_type: Type::Void,
        params: vec![Param {
            ty: Type::Ptr,
            name: None,
        }],
        is_declaration: true,
        attribute_group_refs: vec![],
        basic_blocks: vec![],
    };
    let mut buf = String::new();
    write_function(&mut buf, &f).expect("failed to write");
    expect!["declare void @__quantum__rt__initialize(ptr)"].assert_eq(&buf);
}

#[test]
fn void_two_ptr_declaration() {
    let f = Function {
        name: "__quantum__qis__cx__body".into(),
        return_type: Type::Void,
        params: vec![
            Param {
                ty: Type::Ptr,
                name: None,
            },
            Param {
                ty: Type::Ptr,
                name: None,
            },
        ],
        is_declaration: true,
        attribute_group_refs: vec![],
        basic_blocks: vec![],
    };
    let mut buf = String::new();
    write_function(&mut buf, &f).expect("failed to write");
    expect!["declare void @__quantum__qis__cx__body(ptr, ptr)"].assert_eq(&buf);
}

#[test]
fn declaration_with_return_type() {
    let f = Function {
        name: "__quantum__rt__read_result".into(),
        return_type: Type::Integer(1),
        params: vec![Param {
            ty: Type::Ptr,
            name: None,
        }],
        is_declaration: true,
        attribute_group_refs: vec![],
        basic_blocks: vec![],
    };
    let mut buf = String::new();
    write_function(&mut buf, &f).expect("failed to write");
    expect!["declare i1 @__quantum__rt__read_result(ptr)"].assert_eq(&buf);
}

#[test]
fn declaration_with_attr_ref() {
    let f = Function {
        name: "__quantum__qis__m__body".into(),
        return_type: Type::Void,
        params: vec![
            Param {
                ty: Type::Ptr,
                name: None,
            },
            Param {
                ty: Type::Ptr,
                name: None,
            },
        ],
        is_declaration: true,
        attribute_group_refs: vec![1],
        basic_blocks: vec![],
    };
    let mut buf = String::new();
    write_function(&mut buf, &f).expect("failed to write");
    expect!["declare void @__quantum__qis__m__body(ptr, ptr) #1"].assert_eq(&buf);
}

// --- Function definition emission ---

#[test]
fn simple_definition() {
    let f = Function {
        name: "ENTRYPOINT__main".into(),
        return_type: Type::Integer(64),
        params: vec![],
        is_declaration: false,
        attribute_group_refs: vec![0],
        basic_blocks: vec![BasicBlock {
            name: "block_0".into(),
            instructions: vec![Instruction::Ret(Some(Operand::IntConst(
                Type::Integer(64),
                0,
            )))],
        }],
    };
    let mut buf = String::new();
    write_function(&mut buf, &f).expect("failed to write");
    expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              ret i64 0
            }"#]]
    .assert_eq(&buf);
}

// --- Instruction emission ---

#[test]
fn ret_void() {
    let mut buf = String::new();
    write_instruction(&mut buf, &Instruction::Ret(None)).expect("failed to write");
    expect!["  ret void"].assert_eq(&buf);
}

#[test]
fn ret_i64() {
    let mut buf = String::new();
    write_instruction(
        &mut buf,
        &Instruction::Ret(Some(Operand::IntConst(Type::Integer(64), 0))),
    )
    .expect("failed to write");
    expect!["  ret i64 0"].assert_eq(&buf);
}

#[test]
fn br_conditional() {
    let instr = Instruction::Br {
        cond_ty: Type::Integer(1),
        cond: Operand::LocalRef("var_0".into()),
        true_dest: "block_1".into(),
        false_dest: "block_2".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  br i1 %var_0, label %block_1, label %block_2"].assert_eq(&buf);
}

#[test]
fn br_conditional_with_bool_const() {
    let instr = Instruction::Br {
        cond_ty: Type::Integer(1),
        cond: Operand::IntConst(Type::Integer(1), 1),
        true_dest: "block_1".into(),
        false_dest: "block_2".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  br i1 true, label %block_1, label %block_2"].assert_eq(&buf);
}

#[test]
fn jump_unconditional() {
    let instr = Instruction::Jump {
        dest: "block_1".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  br label %block_1"].assert_eq(&buf);
}

#[test]
fn binop_add_i64() {
    let instr = Instruction::BinOp {
        op: BinOpKind::Add,
        ty: Type::Integer(64),
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::LocalRef("var_1".into()),
        result: "var_2".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_2 = add i64 %var_0, %var_1"].assert_eq(&buf);
}

#[test]
fn binop_sub_i64() {
    let instr = Instruction::BinOp {
        op: BinOpKind::Sub,
        ty: Type::Integer(64),
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::IntConst(Type::Integer(64), 1),
        result: "var_1".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_1 = sub i64 %var_0, 1"].assert_eq(&buf);
}

#[test]
fn binop_mul_i64() {
    let instr = Instruction::BinOp {
        op: BinOpKind::Mul,
        ty: Type::Integer(64),
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::LocalRef("var_1".into()),
        result: "var_2".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_2 = mul i64 %var_0, %var_1"].assert_eq(&buf);
}

#[test]
fn binop_and_i1() {
    let instr = Instruction::BinOp {
        op: BinOpKind::And,
        ty: Type::Integer(1),
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::LocalRef("var_1".into()),
        result: "var_2".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_2 = and i1 %var_0, %var_1"].assert_eq(&buf);
}

#[test]
fn binop_xor_not_i1() {
    let instr = Instruction::BinOp {
        op: BinOpKind::Xor,
        ty: Type::Integer(1),
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::IntConst(Type::Integer(1), 1),
        result: "var_1".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_1 = xor i1 %var_0, true"].assert_eq(&buf);
}

#[test]
fn binop_xor_not_i64() {
    let instr = Instruction::BinOp {
        op: BinOpKind::Xor,
        ty: Type::Integer(64),
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::IntConst(Type::Integer(64), -1),
        result: "var_1".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_1 = xor i64 %var_0, -1"].assert_eq(&buf);
}

#[test]
fn binop_fadd_double() {
    let instr = Instruction::BinOp {
        op: BinOpKind::Fadd,
        ty: Type::Double,
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::LocalRef("var_1".into()),
        result: "var_2".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_2 = fadd double %var_0, %var_1"].assert_eq(&buf);
}

#[test]
fn icmp_eq_i64() {
    let instr = Instruction::ICmp {
        pred: IntPredicate::Eq,
        ty: Type::Integer(64),
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::LocalRef("var_1".into()),
        result: "var_2".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_2 = icmp eq i64 %var_0, %var_1"].assert_eq(&buf);
}

#[test]
fn icmp_slt_i64() {
    let instr = Instruction::ICmp {
        pred: IntPredicate::Slt,
        ty: Type::Integer(64),
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::IntConst(Type::Integer(64), 10),
        result: "var_1".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_1 = icmp slt i64 %var_0, 10"].assert_eq(&buf);
}

#[test]
fn fcmp_oeq_double() {
    let instr = Instruction::FCmp {
        pred: FloatPredicate::Oeq,
        ty: Type::Double,
        lhs: Operand::LocalRef("var_0".into()),
        rhs: Operand::LocalRef("var_1".into()),
        result: "var_2".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_2 = fcmp oeq double %var_0, %var_1"].assert_eq(&buf);
}

#[test]
fn cast_sitofp() {
    let instr = Instruction::Cast {
        op: CastKind::Sitofp,
        from_ty: Type::Integer(64),
        to_ty: Type::Double,
        value: Operand::LocalRef("var_0".into()),
        result: "var_1".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_1 = sitofp i64 %var_0 to double"].assert_eq(&buf);
}

#[test]
fn cast_fptosi() {
    let instr = Instruction::Cast {
        op: CastKind::Fptosi,
        from_ty: Type::Double,
        to_ty: Type::Integer(64),
        value: Operand::LocalRef("var_0".into()),
        result: "var_1".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_1 = fptosi double %var_0 to i64"].assert_eq(&buf);
}

#[test]
fn call_void_no_return() {
    let instr = Instruction::Call {
        return_ty: None,
        callee: "__quantum__qis__h__body".into(),
        args: vec![(Type::Ptr, Operand::IntToPtr(0, Type::Ptr))],
        result: None,
        attr_refs: vec![],
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))"].assert_eq(&buf);
}

#[test]
fn call_void_two_args() {
    let instr = Instruction::Call {
        return_ty: None,
        callee: "__quantum__qis__cx__body".into(),
        args: vec![
            (Type::Ptr, Operand::IntToPtr(0, Type::Ptr)),
            (Type::Ptr, Operand::IntToPtr(1, Type::Ptr)),
        ],
        result: None,
        attr_refs: vec![],
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))"].assert_eq(&buf);
}

#[test]
fn call_with_return() {
    let instr = Instruction::Call {
        return_ty: Some(Type::Integer(1)),
        callee: "__quantum__rt__read_result".into(),
        args: vec![(Type::Ptr, Operand::IntToPtr(0, Type::Ptr))],
        result: Some("var_0".into()),
        attr_refs: vec![],
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))"]
        .assert_eq(&buf);
}

#[test]
fn call_with_attr_ref() {
    let instr = Instruction::Call {
        return_ty: None,
        callee: "__quantum__qis__m__body".into(),
        args: vec![
            (Type::Ptr, Operand::IntToPtr(0, Type::Ptr)),
            (Type::Ptr, Operand::IntToPtr(0, Type::Ptr)),
        ],
        result: None,
        attr_refs: vec![1],
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr)) #1"].assert_eq(&buf);
}

#[test]
fn call_with_i64_arg() {
    let instr = Instruction::Call {
        return_ty: None,
        callee: "__quantum__rt__array_record_output".into(),
        args: vec![
            (Type::Integer(64), Operand::IntConst(Type::Integer(64), 2)),
            (Type::Ptr, Operand::GlobalRef("0".into())),
        ],
        result: None,
        attr_refs: vec![],
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  call void @__quantum__rt__array_record_output(i64 2, ptr @0)"].assert_eq(&buf);
}

#[test]
fn call_result_record_output() {
    let instr = Instruction::Call {
        return_ty: None,
        callee: "__quantum__rt__result_record_output".into(),
        args: vec![
            (Type::Ptr, Operand::IntToPtr(0, Type::Ptr)),
            (Type::Ptr, Operand::GlobalRef("1".into())),
        ],
        result: None,
        attr_refs: vec![],
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect![
        "  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)"
    ]
    .assert_eq(&buf);
}

#[test]
fn phi_i1() {
    let instr = Instruction::Phi {
        ty: Type::Integer(1),
        incoming: vec![
            (Operand::IntConst(Type::Integer(1), 1), "block_0".into()),
            (Operand::LocalRef("var_2".into()), "block_1".into()),
        ],
        result: "var_3".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_3 = phi i1 [true, %block_0], [%var_2, %block_1]"].assert_eq(&buf);
}

#[test]
fn alloca_i1() {
    let instr = Instruction::Alloca {
        ty: Type::Integer(1),
        result: "var_0".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_0 = alloca i1"].assert_eq(&buf);
}

#[test]
fn load_i1() {
    let instr = Instruction::Load {
        ty: Type::Integer(1),
        ptr_ty: Type::Ptr,
        ptr: Operand::LocalRef("var_0".into()),
        result: "var_1".into(),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  %var_1 = load i1, ptr %var_0"].assert_eq(&buf);
}

#[test]
fn store_i1_true() {
    let instr = Instruction::Store {
        ty: Type::Integer(1),
        value: Operand::IntConst(Type::Integer(1), 1),
        ptr_ty: Type::Ptr,
        ptr: Operand::LocalRef("var_0".into()),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  store i1 true, ptr %var_0"].assert_eq(&buf);
}

#[test]
fn store_half_constant() {
    let instr = Instruction::Store {
        ty: Type::Half,
        value: Operand::float_const(Type::Half, 1.5),
        ptr_ty: Type::Ptr,
        ptr: Operand::LocalRef("var_0".into()),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  store half 1.5, ptr %var_0"].assert_eq(&buf);
}

#[test]
fn store_float_constant() {
    let instr = Instruction::Store {
        ty: Type::Float,
        value: Operand::float_const(Type::Float, 2.5),
        ptr_ty: Type::Ptr,
        ptr: Operand::LocalRef("var_0".into()),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  store float 2.5, ptr %var_0"].assert_eq(&buf);
}

#[test]
fn store_double_constant() {
    let instr = Instruction::Store {
        ty: Type::Double,
        value: Operand::float_const(Type::Double, 3.5),
        ptr_ty: Type::Ptr,
        ptr: Operand::LocalRef("var_0".into()),
    };
    let mut buf = String::new();
    write_instruction(&mut buf, &instr).expect("failed to write");
    expect!["  store double 3.5, ptr %var_0"].assert_eq(&buf);
}

// --- Operand formatting ---

#[test]
fn operand_inttoptr_named() {
    let op = Operand::IntToPtr(0, Type::NamedPtr("Qubit".into()));
    let mut buf = String::new();
    write_typed_operand(&mut buf, &op).expect("failed to write");
    expect!["%Qubit* inttoptr (i64 0 to %Qubit*)"].assert_eq(&buf);
}

#[test]
fn operand_float_whole() {
    let mut buf = String::new();
    write_float(&mut buf, 3.0).expect("failed to write");
    expect!["3.0"].assert_eq(&buf);
}

#[test]
fn operand_float_fractional() {
    let mut buf = String::new();
    write_float(&mut buf, std::f64::consts::PI).expect("failed to write");
    expect!["3.141592653589793"].assert_eq(&buf);
}

// --- Attribute group emission ---

#[test]
fn attribute_group_entry_point() {
    let ag = AttributeGroup {
        id: 0,
        attributes: vec![
            Attribute::StringAttr("entry_point".into()),
            Attribute::StringAttr("output_labeling_schema".into()),
            Attribute::KeyValue("qir_profiles".into(), "adaptive_profile".into()),
            Attribute::KeyValue("required_num_qubits".into(), "2".into()),
            Attribute::KeyValue("required_num_results".into(), "1".into()),
        ],
    };
    let mut buf = String::new();
    write_attribute_group(&mut buf, &ag).expect("failed to write");
    expect![r#"attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }"#].assert_eq(&buf);
}

#[test]
fn attribute_group_irreversible() {
    let ag = AttributeGroup {
        id: 1,
        attributes: vec![Attribute::StringAttr("irreversible".into())],
    };
    let mut buf = String::new();
    write_attribute_group(&mut buf, &ag).expect("failed to write");
    expect![r#"attributes #1 = { "irreversible" }"#].assert_eq(&buf);
}

// --- Metadata emission ---

#[test]
fn named_metadata_output() {
    let nm = NamedMetadata {
        name: "llvm.module.flags".into(),
        node_refs: vec![0, 1, 2, 3, 4, 5, 6, 7],
    };
    let mut buf = String::new();
    write_named_metadata(&mut buf, &nm).expect("failed to write");
    expect!["!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}"].assert_eq(&buf);
}

#[test]
fn metadata_node_with_int_and_string() {
    let node = MetadataNode {
        id: 0,
        values: vec![
            MetadataValue::Int(Type::Integer(32), 1),
            MetadataValue::String("qir_major_version".into()),
            MetadataValue::Int(Type::Integer(32), 2),
        ],
    };
    let mut buf = String::new();
    write_metadata_node(&mut buf, &node).expect("failed to write");
    expect![r#"!0 = !{i32 1, !"qir_major_version", i32 2}"#].assert_eq(&buf);
}

#[test]
fn metadata_node_with_bool_false() {
    let node = MetadataNode {
        id: 2,
        values: vec![
            MetadataValue::Int(Type::Integer(32), 1),
            MetadataValue::String("dynamic_qubit_management".into()),
            MetadataValue::Int(Type::Integer(1), 0),
        ],
    };
    let mut buf = String::new();
    write_metadata_node(&mut buf, &node).expect("failed to write");
    expect![r#"!2 = !{i32 1, !"dynamic_qubit_management", i1 false}"#].assert_eq(&buf);
}

#[test]
fn metadata_node_with_bool_true() {
    let node = MetadataNode {
        id: 7,
        values: vec![
            MetadataValue::Int(Type::Integer(32), 1),
            MetadataValue::String("arrays".into()),
            MetadataValue::Int(Type::Integer(1), 1),
        ],
    };
    let mut buf = String::new();
    write_metadata_node(&mut buf, &node).expect("failed to write");
    expect![r#"!7 = !{i32 1, !"arrays", i1 true}"#].assert_eq(&buf);
}

#[test]
fn metadata_node_with_i2() {
    let node = MetadataNode {
        id: 6,
        values: vec![
            MetadataValue::Int(Type::Integer(32), 7),
            MetadataValue::String("backwards_branching".into()),
            MetadataValue::Int(Type::Integer(2), 3),
        ],
    };
    let mut buf = String::new();
    write_metadata_node(&mut buf, &node).expect("failed to write");
    expect![r#"!6 = !{i32 7, !"backwards_branching", i2 3}"#].assert_eq(&buf);
}

#[test]
fn metadata_node_with_sublist() {
    let node = MetadataNode {
        id: 4,
        values: vec![
            MetadataValue::Int(Type::Integer(32), 5),
            MetadataValue::String("int_computations".into()),
            MetadataValue::SubList(vec![MetadataValue::String("i64".into())]),
        ],
    };
    let mut buf = String::new();
    write_metadata_node(&mut buf, &node).expect("failed to write");
    expect![r#"!4 = !{i32 5, !"int_computations", !{!"i64"}}"#].assert_eq(&buf);
}

// --- Full module emission ---

#[test]
fn empty_module_output() {
    let m = empty_module();
    let output = write_module_to_string(&m);
    expect![""].assert_eq(&output);
}

#[test]
fn bell_module_v2_output() {
    let m = bell_module_v2();
    let output = write_module_to_string(&m);
    expect![[r#"
            @0 = internal constant [4 x i8] c"0_a\00"
            @1 = internal constant [6 x i8] c"1_a0r\00"
            @2 = internal constant [6 x i8] c"2_a1r\00"

            declare void @__quantum__qis__h__body(ptr)

            declare void @__quantum__qis__cx__body(ptr, ptr)

            declare void @__quantum__qis__m__body(ptr, ptr) #1

            declare void @__quantum__rt__array_record_output(i64, ptr)

            declare void @__quantum__rt__result_record_output(ptr, ptr)

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__rt__array_record_output(i64 2, ptr @0)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
              ret i64 0
            }

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 7, !"backwards_branching", i2 3}
            !6 = !{i32 1, !"arrays", i1 true}
        "#]]
        .assert_eq(&output);
}
