// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::ToQir;
use expect_test::expect;
use qsc_rir::builder;
use qsc_rir::rir;

#[test]
fn single_qubit_gate_decl_works() {
    let decl = builder::x_decl();
    expect!["declare void @__quantum__qis__x__body(ptr)"]
        .assert_eq(&decl.to_qir(&rir::Program::default()));
}

#[test]
fn two_qubit_gate_decl_works() {
    let decl = builder::cx_decl();
    expect!["declare void @__quantum__qis__cx__body(ptr, ptr)"]
        .assert_eq(&decl.to_qir(&rir::Program::default()));
}

#[test]
fn single_qubit_rotation_decl_works() {
    let decl = builder::rx_decl();
    expect!["declare void @__quantum__qis__rx__body(double, ptr)"]
        .assert_eq(&decl.to_qir(&rir::Program::default()));
}

#[test]
fn measurement_decl_works() {
    let decl = builder::m_decl();
    expect!["declare void @__quantum__qis__m__body(ptr, ptr) #1"]
        .assert_eq(&decl.to_qir(&rir::Program::default()));
}

#[test]
fn read_result_decl_works() {
    let decl = builder::read_result_decl();
    expect!["declare i1 @__quantum__rt__read_result(ptr)"]
        .assert_eq(&decl.to_qir(&rir::Program::default()));
}

#[test]
fn result_record_decl_works() {
    let decl = builder::result_record_decl();
    expect!["declare void @__quantum__rt__result_record_output(ptr, ptr)"]
        .assert_eq(&decl.to_qir(&rir::Program::default()));
}

#[test]
fn single_qubit_call() {
    let mut program = rir::Program::default();
    program
        .callables
        .insert(rir::CallableId(0), builder::x_decl());
    let call = rir::Instruction::Call(
        rir::CallableId(0),
        vec![rir::Operand::Literal(rir::Literal::Qubit(0))],
        None,
        None,
    );
    expect!["  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))"]
        .assert_eq(&call.to_qir(&program));
}

#[test]
fn qubit_rotation_call() {
    let mut program = rir::Program::default();
    program
        .callables
        .insert(rir::CallableId(0), builder::rx_decl());
    let call = rir::Instruction::Call(
        rir::CallableId(0),
        vec![
            rir::Operand::Literal(rir::Literal::Double(std::f64::consts::PI)),
            rir::Operand::Literal(rir::Literal::Qubit(0)),
        ],
        None,
        None,
    );
    expect!["  call void @__quantum__qis__rx__body(double 3.141592653589793, ptr inttoptr (i64 0 to ptr))"]
        .assert_eq(&call.to_qir(&program));
}

#[test]
fn qubit_rotation_round_number_call() {
    let mut program = rir::Program::default();
    program
        .callables
        .insert(rir::CallableId(0), builder::rx_decl());
    let call = rir::Instruction::Call(
        rir::CallableId(0),
        vec![
            rir::Operand::Literal(rir::Literal::Double(3.0)),
            rir::Operand::Literal(rir::Literal::Qubit(0)),
        ],
        None,
        None,
    );
    expect!["  call void @__quantum__qis__rx__body(double 3.0, ptr inttoptr (i64 0 to ptr))"]
        .assert_eq(&call.to_qir(&program));
}

#[test]
fn qubit_rotation_variable_angle_call() {
    let mut program = rir::Program::default();
    program
        .callables
        .insert(rir::CallableId(0), builder::rx_decl());
    let call = rir::Instruction::Call(
        rir::CallableId(0),
        vec![
            rir::Operand::Variable(rir::Variable {
                variable_id: rir::VariableId(0),
                ty: rir::Ty::Prim(rir::Prim::Double),
            }),
            rir::Operand::Literal(rir::Literal::Qubit(0)),
        ],
        None,
        None,
    );
    expect!["  call void @__quantum__qis__rx__body(double %var_0, ptr inttoptr (i64 0 to ptr))"]
        .assert_eq(&call.to_qir(&program));
}

#[test]
fn bell_program() {
    let program = builder::bell_program();
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

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]].assert_eq(&program.to_qir(&program));
}

#[test]
fn teleport_program() {
    let program = builder::teleport_program();
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_r\00"

        declare void @__quantum__qis__h__body(ptr)

        declare void @__quantum__qis__z__body(ptr)

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__qis__cx__body(ptr, ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare i1 @__quantum__rt__read_result(ptr)

        declare void @__quantum__rt__result_record_output(ptr, ptr)

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_0, label %block_1, label %block_2
        block_1:
          call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
          br label %block_2
        block_2:
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
          %var_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          br i1 %var_1, label %block_3, label %block_4
        block_3:
          call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
          br label %block_4
        block_4:
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
          ret i64 0
        }

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]].assert_eq(&program.to_qir(&program));
}

#[test]
fn ir_function_program() {
    let mut program = rir::Program::default();
    program
        .callables
        .insert(rir::CallableId(0), builder::x_decl());
    program.callables.insert(
        rir::CallableId(1),
        rir::Callable {
            name: "ApplyX".to_string(),
            input_type: vec![rir::Ty::Prim(rir::Prim::Qubit)],
            input_vars: vec![rir::VariableId(0)],
            output_type: None,
            body: Some(rir::BlockId(0)),
            call_type: rir::CallableType::Regular,
        },
    );
    program.callables.insert(
        rir::CallableId(2),
        rir::Callable {
            name: "main".to_string(),
            input_type: vec![],
            input_vars: vec![],
            output_type: Some(rir::Ty::Prim(rir::Prim::Integer)),
            body: Some(rir::BlockId(1)),
            call_type: rir::CallableType::Regular,
        },
    );
    program.entry = rir::CallableId(2);
    program.blocks.insert(
        rir::BlockId(0),
        rir::Block(vec![
            rir::Instruction::Call(
                rir::CallableId(0),
                vec![rir::Operand::Variable(rir::Variable {
                    variable_id: rir::VariableId(0),
                    ty: rir::Ty::Prim(rir::Prim::Qubit),
                })],
                None,
                None,
            ),
            rir::Instruction::Return(None),
        ]),
    );
    program.blocks.insert(
        rir::BlockId(1),
        rir::Block(vec![
            rir::Instruction::Call(
                rir::CallableId(1),
                vec![rir::Operand::Literal(rir::Literal::Qubit(0))],
                None,
                None,
            ),
            rir::Instruction::Return(Some(rir::Operand::Literal(rir::Literal::Integer(0)))),
        ]),
    );
    program.num_qubits = 1;
    program.num_results = 0;
    expect![[r#"

        declare void @__quantum__qis__x__body(ptr)

        define void @ApplyX(ptr %var_0) {
        block_0:
          call void @__quantum__qis__x__body(ptr %var_0)
          ret void
        }

        define i64 @ENTRYPOINT__main() #0 {
        block_1:
          call void @ApplyX(ptr inttoptr (i64 0 to ptr))
          ret i64 0
        }

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]].assert_eq(&program.to_qir(&program));
}

#[test]
fn ir_function_name_with_special_characters_is_quoted() {
    let mut program = rir::Program::default();
    program
        .callables
        .insert(rir::CallableId(0), builder::x_decl());
    program.callables.insert(
        rir::CallableId(1),
        rir::Callable {
            name: "ApplyGeneric<Qubit, AdjCtl>{X}".to_string(),
            input_type: vec![rir::Ty::Prim(rir::Prim::Qubit)],
            input_vars: vec![rir::VariableId(0)],
            output_type: None,
            body: Some(rir::BlockId(0)),
            call_type: rir::CallableType::Regular,
        },
    );
    program.callables.insert(
        rir::CallableId(2),
        rir::Callable {
            name: "main".to_string(),
            input_type: vec![],
            input_vars: vec![],
            output_type: Some(rir::Ty::Prim(rir::Prim::Integer)),
            body: Some(rir::BlockId(1)),
            call_type: rir::CallableType::Regular,
        },
    );
    program.entry = rir::CallableId(2);
    program.blocks.insert(
        rir::BlockId(0),
        rir::Block(vec![
            rir::Instruction::Call(
                rir::CallableId(0),
                vec![rir::Operand::Variable(rir::Variable {
                    variable_id: rir::VariableId(0),
                    ty: rir::Ty::Prim(rir::Prim::Qubit),
                })],
                None,
                None,
            ),
            rir::Instruction::Return(None),
        ]),
    );
    program.blocks.insert(
        rir::BlockId(1),
        rir::Block(vec![
            rir::Instruction::Call(
                rir::CallableId(1),
                vec![rir::Operand::Literal(rir::Literal::Qubit(0))],
                None,
                None,
            ),
            rir::Instruction::Return(Some(rir::Operand::Literal(rir::Literal::Integer(0)))),
        ]),
    );
    program.num_qubits = 1;
    program.num_results = 0;

    let qir = program.to_qir(&program);
    assert!(
        qir.contains("define void @\"ApplyGeneric<Qubit, AdjCtl>{X}\"(ptr %var_0)"),
        "expected quoted IR-function definition for special-character name; got:\n{qir}"
    );
    assert!(
        qir.contains("call void @\"ApplyGeneric<Qubit, AdjCtl>{X}\"(ptr inttoptr (i64 0 to ptr))"),
        "expected quoted call target for special-character name; got:\n{qir}"
    );
}

#[test]
fn scalar_ir_function_program() {
    // A non-entry, scalar-returning IR function renders as a real `define i64 @<name>(...)` with a
    // typed `ret i64 <op>` terminator, and its call site binds a typed output variable.
    let mut program = rir::Program::default();
    program.callables.insert(
        rir::CallableId(0),
        rir::Callable {
            name: "Increment".to_string(),
            input_type: vec![rir::Ty::Prim(rir::Prim::Integer)],
            input_vars: vec![rir::VariableId(0)],
            output_type: Some(rir::Ty::Prim(rir::Prim::Integer)),
            body: Some(rir::BlockId(0)),
            call_type: rir::CallableType::Regular,
        },
    );
    program.callables.insert(
        rir::CallableId(1),
        rir::Callable {
            name: "main".to_string(),
            input_type: vec![],
            input_vars: vec![],
            output_type: Some(rir::Ty::Prim(rir::Prim::Integer)),
            body: Some(rir::BlockId(1)),
            call_type: rir::CallableType::Regular,
        },
    );
    program.entry = rir::CallableId(1);
    program.blocks.insert(
        rir::BlockId(0),
        rir::Block(vec![
            rir::Instruction::Add(
                rir::Operand::Variable(rir::Variable {
                    variable_id: rir::VariableId(0),
                    ty: rir::Ty::Prim(rir::Prim::Integer),
                }),
                rir::Operand::Literal(rir::Literal::Integer(1)),
                rir::Variable {
                    variable_id: rir::VariableId(1),
                    ty: rir::Ty::Prim(rir::Prim::Integer),
                },
            ),
            rir::Instruction::Return(Some(rir::Operand::Variable(rir::Variable {
                variable_id: rir::VariableId(1),
                ty: rir::Ty::Prim(rir::Prim::Integer),
            }))),
        ]),
    );
    program.blocks.insert(
        rir::BlockId(1),
        rir::Block(vec![
            rir::Instruction::Call(
                rir::CallableId(0),
                vec![rir::Operand::Literal(rir::Literal::Integer(5))],
                Some(rir::Variable {
                    variable_id: rir::VariableId(2),
                    ty: rir::Ty::Prim(rir::Prim::Integer),
                }),
                None,
            ),
            rir::Instruction::Return(Some(rir::Operand::Literal(rir::Literal::Integer(0)))),
        ]),
    );
    program.num_qubits = 0;
    program.num_results = 0;
    expect![[r#"

        define i64 @Increment(i64 %var_0) {
        block_0:
          %var_1 = add i64 %var_0, 1
          ret i64 %var_1
        }

        define i64 @ENTRYPOINT__main() #0 {
        block_1:
          %var_2 = call i64 @Increment(i64 5)
          ret i64 0
        }

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]].assert_eq(&program.to_qir(&program));
}

#[test]
fn program_without_ir_functions_omits_ir_functions_flag() {
    let program = builder::bell_program();
    let qir = program.to_qir(&program);
    assert!(
        !qir.contains("ir_functions"),
        "a program without IR functions should not emit the ir_functions module flag"
    );
}
