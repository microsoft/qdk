// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::ToQir;
use expect_test::expect;
use qsc_rir::rir;

#[test]
fn logical_and_literals() {
    let inst = rir::Instruction::LogicalAnd(
        rir::Operand::Literal(rir::Literal::Bool(true)),
        rir::Operand::Literal(rir::Literal::Bool(false)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = and i1 true, false"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn logical_and_variables() {
    let inst = rir::Instruction::LogicalAnd(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Boolean,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Boolean,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = and i1 %var_1, %var_2"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn logical_not_true_literal() {
    let inst = rir::Instruction::LogicalNot(
        rir::Operand::Literal(rir::Literal::Bool(true)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = xor i1 true, true"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn logical_not_variables() {
    let inst = rir::Instruction::LogicalNot(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Boolean,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = xor i1 %var_1, true"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn logical_not_false_literal() {
    let inst = rir::Instruction::LogicalNot(
        rir::Operand::Literal(rir::Literal::Bool(false)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = xor i1 false, true"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn logical_or_literals() {
    let inst = rir::Instruction::LogicalOr(
        rir::Operand::Literal(rir::Literal::Bool(true)),
        rir::Operand::Literal(rir::Literal::Bool(false)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = or i1 true, false"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn logical_or_variables() {
    let inst = rir::Instruction::LogicalOr(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Boolean,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Boolean,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = or i1 %var_1, %var_2"].assert_eq(&inst.to_qir(&rir::Program::default()));
}
