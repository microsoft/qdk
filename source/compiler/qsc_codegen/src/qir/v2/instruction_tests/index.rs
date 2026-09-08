// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::v2::ToQir;
use expect_test::expect;
use qsc_rir::rir;

#[test]
fn integer_from_variable_index() {
    let inst = rir::Instruction::Index(
        rir::Operand::Variable(rir::Variable::new_array(
            rir::VariableId(1),
            1,
            rir::Prim::Integer,
        )),
        rir::Operand::Variable(rir::Variable::new_integer(rir::VariableId(0))),
        rir::Variable::new_integer(rir::VariableId(0)),
    );
    let qir = inst.to_qir(&rir::Program::default());
    let mut lines = qir.lines();
    expect!["  %var_0_offset_chk = icmp slt i64 %var_0, 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_0_offset = select i1 %var_0_offset_chk, i64 1, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_0 = getelementptr [1 x i64], ptr %var_1, i64 %var_0_offset, i64 %var_0"]
        .assert_eq(lines.next().expect("line should exist"));
    assert_eq!(lines.next(), None);
}

#[test]
fn integer_from_literal_index() {
    let inst = rir::Instruction::Index(
        rir::Operand::Variable(rir::Variable::new_array(
            rir::VariableId(1),
            1,
            rir::Prim::Integer,
        )),
        rir::Operand::Literal(rir::Literal::Integer(0)),
        rir::Variable::new_integer(rir::VariableId(0)),
    );
    let qir = inst.to_qir(&rir::Program::default());
    let mut lines = qir.lines();
    expect!["  %var_0_offset_chk = icmp slt i64 0, 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_0_offset = select i1 %var_0_offset_chk, i64 1, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_0 = getelementptr [1 x i64], ptr %var_1, i64 %var_0_offset, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    assert_eq!(lines.next(), None);
}

#[test]
fn integer_from_literal_index_literal_array() {
    let mut program = rir::Program::default();
    program.array_literals.push(rir::ArrayLiteral {
        contents: vec![rir::Literal::Integer(0)],
        ty: rir::Prim::Integer,
    });
    let inst = rir::Instruction::Index(
        rir::Operand::Literal(rir::Literal::Array(0)),
        rir::Operand::Literal(rir::Literal::Integer(0)),
        rir::Variable::new_integer(rir::VariableId(0)),
    );
    let qir = inst.to_qir(&program);
    let mut lines = qir.lines();
    expect!["  %var_0_offset_chk = icmp slt i64 0, 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_0_offset = select i1 %var_0_offset_chk, i64 1, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_0 = getelementptr [1 x i64], ptr @array0, i64 %var_0_offset, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    assert_eq!(lines.next(), None);
}

#[test]
fn double_from_variable_index() {
    let inst = rir::Instruction::Index(
        rir::Operand::Variable(rir::Variable::new_array(
            rir::VariableId(1),
            1,
            rir::Prim::Double,
        )),
        rir::Operand::Variable(rir::Variable::new_integer(rir::VariableId(0))),
        rir::Variable::new_double(rir::VariableId(0)),
    );
    let qir = inst.to_qir(&rir::Program::default());
    let mut lines = qir.lines();
    expect!["  %var_0_offset_chk = icmp slt i64 %var_0, 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_0_offset = select i1 %var_0_offset_chk, i64 1, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_0 = getelementptr [1 x double], ptr %var_1, i64 %var_0_offset, i64 %var_0"]
        .assert_eq(lines.next().expect("line should exist"));
    assert_eq!(lines.next(), None);
}

#[test]
fn double_from_literal_index() {
    let inst = rir::Instruction::Index(
        rir::Operand::Variable(rir::Variable::new_array(
            rir::VariableId(1),
            1,
            rir::Prim::Double,
        )),
        rir::Operand::Literal(rir::Literal::Integer(0)),
        rir::Variable::new_double(rir::VariableId(0)),
    );
    let qir = inst.to_qir(&rir::Program::default());
    let mut lines = qir.lines();
    expect!["  %var_0_offset_chk = icmp slt i64 0, 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_0_offset = select i1 %var_0_offset_chk, i64 1, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_0 = getelementptr [1 x double], ptr %var_1, i64 %var_0_offset, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    assert_eq!(lines.next(), None);
}
