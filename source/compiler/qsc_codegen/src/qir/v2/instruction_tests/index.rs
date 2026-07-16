// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::v2::ToQir;
use expect_test::expect;
use qsc_rir::rir;

#[test]
fn integer_from_variable_index() {
    let inst = rir::Instruction::Index(
        rir::Operand::Variable(rir::Variable::new_ptr(rir::VariableId(1))),
        rir::Operand::Variable(rir::Variable::new_integer(rir::VariableId(0))),
        rir::Variable::new_integer(rir::VariableId(0)),
    );
    expect!["  %var_0 = getelementptr i64, ptr %var_1, i64 %var_0"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn integer_from_literal_index() {
    let inst = rir::Instruction::Index(
        rir::Operand::Variable(rir::Variable::new_ptr(rir::VariableId(1))),
        rir::Operand::Literal(rir::Literal::Integer(0)),
        rir::Variable::new_integer(rir::VariableId(0)),
    );
    expect!["  %var_0 = getelementptr i64, ptr %var_1, i64 0"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn integer_from_literal_index_literal_array() {
    let inst = rir::Instruction::Index(
        rir::Operand::Literal(rir::Literal::Array(0)),
        rir::Operand::Literal(rir::Literal::Integer(0)),
        rir::Variable::new_integer(rir::VariableId(0)),
    );
    expect!["  %var_0 = getelementptr i64, ptr @array0, i64 0"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn double_from_variable_index() {
    let inst = rir::Instruction::Index(
        rir::Operand::Variable(rir::Variable::new_ptr(rir::VariableId(1))),
        rir::Operand::Variable(rir::Variable::new_integer(rir::VariableId(0))),
        rir::Variable::new_double(rir::VariableId(0)),
    );
    expect!["  %var_0 = getelementptr double, ptr %var_1, i64 %var_0"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn double_from_literal_index() {
    let inst = rir::Instruction::Index(
        rir::Operand::Variable(rir::Variable::new_ptr(rir::VariableId(1))),
        rir::Operand::Literal(rir::Literal::Integer(0)),
        rir::Variable::new_double(rir::VariableId(0)),
    );
    expect!["  %var_0 = getelementptr double, ptr %var_1, i64 0"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}
