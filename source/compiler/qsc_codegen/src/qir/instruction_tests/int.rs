// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::ToQir;
use expect_test::expect;
use qsc_rir::rir;

#[test]
fn add_integer_literals() {
    let inst = rir::Instruction::Add(
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = add i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn add_integer_variables() {
    let inst = rir::Instruction::Add(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = add i64 %var_1, %var_2"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn ashr_integer_literals() {
    let inst = rir::Instruction::Ashr(
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = ashr i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn ashr_integer_variables() {
    let inst = rir::Instruction::Ashr(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = ashr i64 %var_1, %var_2"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn bitwise_and_integer_literals() {
    let inst = rir::Instruction::BitwiseAnd(
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = and i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn bitwise_add_integer_variables() {
    let inst = rir::Instruction::BitwiseAnd(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = and i64 %var_1, %var_2"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn bitwise_not_integer_literals() {
    let inst = rir::Instruction::BitwiseNot(
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = xor i64 2, -1"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn bitwise_not_integer_variables() {
    let inst = rir::Instruction::BitwiseNot(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = xor i64 %var_1, -1"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn bitwise_or_integer_literals() {
    let inst = rir::Instruction::BitwiseOr(
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = or i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn bitwise_or_integer_variables() {
    let inst = rir::Instruction::BitwiseOr(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = or i64 %var_1, %var_2"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn bitwise_xor_integer_literals() {
    let inst = rir::Instruction::BitwiseXor(
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = xor i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn bitwise_xor_integer_variables() {
    let inst = rir::Instruction::BitwiseXor(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = xor i64 %var_1, %var_2"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn icmp_eq_integer_literals() {
    let inst = rir::Instruction::Icmp(
        rir::ConditionCode::Eq,
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = icmp eq i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn icmp_eq_integer_variables() {
    let inst = rir::Instruction::Icmp(
        rir::ConditionCode::Eq,
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = icmp eq i64 %var_1, %var_2"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn icmp_ne_integer_literals() {
    let inst = rir::Instruction::Icmp(
        rir::ConditionCode::Ne,
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = icmp ne i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn icmp_ne_integer_variables() {
    let inst = rir::Instruction::Icmp(
        rir::ConditionCode::Ne,
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = icmp ne i64 %var_1, %var_2"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}
#[test]
fn icmp_slt_integer_literals() {
    let inst = rir::Instruction::Icmp(
        rir::ConditionCode::Slt,
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = icmp slt i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn icmp_slt_integer_variables() {
    let inst = rir::Instruction::Icmp(
        rir::ConditionCode::Slt,
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = icmp slt i64 %var_1, %var_2"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}
#[test]
fn icmp_sle_integer_literals() {
    let inst = rir::Instruction::Icmp(
        rir::ConditionCode::Sle,
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = icmp sle i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn icmp_sle_integer_variables() {
    let inst = rir::Instruction::Icmp(
        rir::ConditionCode::Sle,
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = icmp sle i64 %var_1, %var_2"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}
#[test]
fn icmp_sgt_integer_literals() {
    let inst = rir::Instruction::Icmp(
        rir::ConditionCode::Sgt,
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = icmp sgt i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn icmp_sgt_integer_variables() {
    let inst = rir::Instruction::Icmp(
        rir::ConditionCode::Sgt,
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = icmp sgt i64 %var_1, %var_2"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}
#[test]
fn icmp_sge_integer_literals() {
    let inst = rir::Instruction::Icmp(
        rir::ConditionCode::Sge,
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = icmp sge i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn icmp_sge_integer_variables() {
    let inst = rir::Instruction::Icmp(
        rir::ConditionCode::Sge,
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Boolean,
        },
    );
    expect!["  %var_0 = icmp sge i64 %var_1, %var_2"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn mul_integer_literals() {
    let inst = rir::Instruction::Mul(
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = mul i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn mul_integer_variables() {
    let inst = rir::Instruction::Mul(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = mul i64 %var_1, %var_2"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn sdiv_integer_literals() {
    let inst = rir::Instruction::Sdiv(
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = sdiv i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn sdiv_integer_variables() {
    let inst = rir::Instruction::Sdiv(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = sdiv i64 %var_1, %var_2"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn shl_integer_literals() {
    let inst = rir::Instruction::Shl(
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = shl i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn shl_integer_variables() {
    let inst = rir::Instruction::Shl(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = shl i64 %var_1, %var_2"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn srem_integer_literals() {
    let inst = rir::Instruction::Srem(
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = srem i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn srem_integer_variables() {
    let inst = rir::Instruction::Srem(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = srem i64 %var_1, %var_2"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn sub_integer_literals() {
    let inst = rir::Instruction::Sub(
        rir::Operand::Literal(rir::Literal::Integer(2)),
        rir::Operand::Literal(rir::Literal::Integer(5)),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = sub i64 2, 5"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn sub_integer_variables() {
    let inst = rir::Instruction::Sub(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(1),
            ty: rir::Ty::Integer,
        }),
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(2),
            ty: rir::Ty::Integer,
        }),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = sub i64 %var_1, %var_2"].assert_eq(&inst.to_qir(&rir::Program::default()));
}
