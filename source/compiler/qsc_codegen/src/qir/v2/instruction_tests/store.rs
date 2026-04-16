// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::v2::ToQir;
use expect_test::expect;
use qsc_rir::rir;

#[test]
fn store_integer_literal_to_pointer() {
    let inst = rir::Instruction::Store(
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable::new_ptr(rir::VariableId(0)),
    );
    expect!["  store i64 5, ptr %var_0"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn store_integer_variable_to_pointer() {
    let inst = rir::Instruction::Store(
        rir::Operand::Variable(rir::Variable::new_integer(rir::VariableId(1))),
        rir::Variable::new_ptr(rir::VariableId(0)),
    );
    expect!["  store i64 %var_1, ptr %var_0"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn store_bool_literal_to_pointer() {
    let inst = rir::Instruction::Store(
        rir::Operand::Literal(rir::Literal::Bool(true)),
        rir::Variable::new_ptr(rir::VariableId(0)),
    );
    expect!["  store i1 true, ptr %var_0"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn store_double_literal_to_pointer() {
    let inst = rir::Instruction::Store(
        rir::Operand::Literal(rir::Literal::Double(2.5)),
        rir::Variable::new_ptr(rir::VariableId(0)),
    );
    expect!["  store double 2.5, ptr %var_0"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn store_pointer_literal_to_pointer() {
    let inst = rir::Instruction::Store(
        rir::Operand::Literal(rir::Literal::Pointer),
        rir::Variable::new_ptr(rir::VariableId(0)),
    );
    expect!["  store ptr null, ptr %var_0"].assert_eq(&inst.to_qir(&rir::Program::default()));
}
