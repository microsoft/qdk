// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::ToQir;
use expect_test::expect;
use qsc_rir::builder;
use qsc_rir::rir;

#[test]
fn single_qubit_gate_decl_works() {
    let decl = builder::x_decl();
    expect!["declare void @__quantum__qis__x__body(%Qubit*)"]
        .assert_eq(&decl.to_qir(&rir::Program::default()));
}

#[test]
fn two_qubit_gate_decl_works() {
    let decl = builder::cx_decl();
    expect!["declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)"]
        .assert_eq(&decl.to_qir(&rir::Program::default()));
}

#[test]
fn single_qubit_rotation_decl_works() {
    let decl = builder::rx_decl();
    expect!["declare void @__quantum__qis__rx__body(double, %Qubit*)"]
        .assert_eq(&decl.to_qir(&rir::Program::default()));
}

#[test]
fn measurement_decl_works() {
    let decl = builder::m_decl();
    expect!["declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1"]
        .assert_eq(&decl.to_qir(&rir::Program::default()));
}

#[test]
fn read_result_decl_works() {
    let decl = builder::read_result_decl();
    expect!["declare i1 @__quantum__qis__read_result__body(%Result*)"]
        .assert_eq(&decl.to_qir(&rir::Program::default()));
}

#[test]
fn result_record_decl_works() {
    let decl = builder::result_record_decl();
    expect!["declare void @__quantum__rt__result_record_output(%Result*, i8*)"]
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
    );
    expect!["  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))"]
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
    );
    expect!["  call void @__quantum__qis__rx__body(double 3.141592653589793, %Qubit* inttoptr (i64 0 to %Qubit*))"]
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
    );
    expect![
        "  call void @__quantum__qis__rx__body(double 3.0, %Qubit* inttoptr (i64 0 to %Qubit*))"
    ]
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
                ty: rir::Ty::Double,
            }),
            rir::Operand::Literal(rir::Literal::Qubit(0)),
        ],
        None,
    );
    expect![
        "  call void @__quantum__qis__rx__body(double %var_0, %Qubit* inttoptr (i64 0 to %Qubit*))"
    ]
    .assert_eq(&call.to_qir(&program));
}

#[test]
fn bell_program() {
    let program = builder::bell_program();
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @empty_tag = internal constant [1 x i8] c"\00"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          ret i64 0
        }

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
    "#]].assert_eq(&program.to_qir(&program));
}

#[test]
fn teleport_program() {
    let program = builder::teleport_program();
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @empty_tag = internal constant [1 x i8] c"\00"
        @0 = internal constant [4 x i8] c"0_r\00"

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__z__body(%Qubit*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare i1 @__quantum__qis__read_result__body(%Result*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          %var_0 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
          br i1 %var_0, label %block_1, label %block_2
        block_1:
          call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          br label %block_2
        block_2:
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          %var_1 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
          br i1 %var_1, label %block_3, label %block_4
        block_3:
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          br label %block_4
        block_4:
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
    "#]].assert_eq(&program.to_qir(&program));
}
