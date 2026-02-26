// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use core::f64::consts::{E, PI};

use crate::qir::ToQir;
use expect_test::expect;
use qsc_rir::rir::{
    FcmpConditionCode, Instruction, Literal, Operand, Program, Ty, Variable, VariableId,
};

#[test]
#[should_panic(expected = "unsupported type double for add")]
fn add_double_literals() {
    let inst = Instruction::Add(
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    let _ = &inst.to_qir(&Program::default());
}

#[test]
#[should_panic(expected = "unsupported type double for sub")]
fn sub_double_literals() {
    let inst = Instruction::Sub(
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    let _ = &inst.to_qir(&Program::default());
}

#[test]
#[should_panic(expected = "unsupported type double for mul")]
fn mul_double_literals() {
    let inst = Instruction::Mul(
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    let _ = &inst.to_qir(&Program::default());
}

#[test]
#[should_panic(expected = "unsupported type double for sdiv")]
fn sdiv_double_literals() {
    let inst = Instruction::Sdiv(
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    let _ = &inst.to_qir(&Program::default());
}

#[test]
fn fadd_double_literals() {
    let inst = Instruction::Fadd(
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    expect!["  %var_0 = fadd double 3.141592653589793, 2.718281828459045"]
        .assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
#[should_panic(expected = "unsupported type double for ashr")]
fn ashr_double_literals() {
    let inst = Instruction::Ashr(
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    let _ = &inst.to_qir(&Program::default());
}

#[test]
#[should_panic(expected = "unsupported type double for and")]
fn bitwise_and_double_literals() {
    let inst = Instruction::BitwiseAnd(
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    let _ = &inst.to_qir(&Program::default());
}

#[test]
#[should_panic(expected = "unsupported type double for not")]
fn bitwise_not_double_literals() {
    let inst = Instruction::BitwiseNot(
        Operand::Literal(Literal::Double(PI)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    let _ = &inst.to_qir(&Program::default());
}

#[test]
#[should_panic(expected = "unsupported type double for or")]
fn bitwise_or_double_literals() {
    let inst = Instruction::BitwiseOr(
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    let _ = &inst.to_qir(&Program::default());
}

#[test]
#[should_panic(expected = "unsupported type double for xor")]
fn bitwise_xor_double_literals() {
    let inst = Instruction::BitwiseXor(
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    let _ = &inst.to_qir(&Program::default());
}

#[test]
fn fadd_double_variables() {
    let inst = Instruction::Fadd(
        Operand::Variable(Variable {
            variable_id: VariableId(1),
            ty: Ty::Double,
        }),
        Operand::Variable(Variable {
            variable_id: VariableId(2),
            ty: Ty::Double,
        }),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    expect!["  %var_0 = fadd double %var_1, %var_2"].assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fcmp_oeq_double_literals() {
    let inst = Instruction::Fcmp(
        FcmpConditionCode::OrderedAndEqual,
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Boolean,
        },
    );
    expect!["  %var_0 = fcmp oeq double 3.141592653589793, 2.718281828459045"]
        .assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fcmp_oeq_double_variables() {
    let inst = Instruction::Fcmp(
        FcmpConditionCode::OrderedAndEqual,
        Operand::Variable(Variable {
            variable_id: VariableId(1),
            ty: Ty::Double,
        }),
        Operand::Variable(Variable {
            variable_id: VariableId(2),
            ty: Ty::Double,
        }),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Boolean,
        },
    );
    expect!["  %var_0 = fcmp oeq double %var_1, %var_2"]
        .assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fcmp_one_double_literals() {
    let inst = Instruction::Fcmp(
        FcmpConditionCode::OrderedAndNotEqual,
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Boolean,
        },
    );
    expect!["  %var_0 = fcmp one double 3.141592653589793, 2.718281828459045"]
        .assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fcmp_one_double_variables() {
    let inst = Instruction::Fcmp(
        FcmpConditionCode::OrderedAndNotEqual,
        Operand::Variable(Variable {
            variable_id: VariableId(1),
            ty: Ty::Double,
        }),
        Operand::Variable(Variable {
            variable_id: VariableId(2),
            ty: Ty::Double,
        }),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Boolean,
        },
    );
    expect!["  %var_0 = fcmp one double %var_1, %var_2"]
        .assert_eq(&inst.to_qir(&Program::default()));
}
#[test]
fn fcmp_olt_double_literals() {
    let inst = Instruction::Fcmp(
        FcmpConditionCode::OrderedAndLessThan,
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Boolean,
        },
    );
    expect!["  %var_0 = fcmp olt double 3.141592653589793, 2.718281828459045"]
        .assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fcmp_olt_double_variables() {
    let inst = Instruction::Fcmp(
        FcmpConditionCode::OrderedAndLessThan,
        Operand::Variable(Variable {
            variable_id: VariableId(1),
            ty: Ty::Double,
        }),
        Operand::Variable(Variable {
            variable_id: VariableId(2),
            ty: Ty::Double,
        }),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Boolean,
        },
    );
    expect!["  %var_0 = fcmp olt double %var_1, %var_2"]
        .assert_eq(&inst.to_qir(&Program::default()));
}
#[test]
fn fcmp_ole_double_literals() {
    let inst = Instruction::Fcmp(
        FcmpConditionCode::OrderedAndLessThanOrEqual,
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Boolean,
        },
    );
    expect!["  %var_0 = fcmp ole double 3.141592653589793, 2.718281828459045"]
        .assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fcmp_ole_double_variables() {
    let inst = Instruction::Fcmp(
        FcmpConditionCode::OrderedAndLessThanOrEqual,
        Operand::Variable(Variable {
            variable_id: VariableId(1),
            ty: Ty::Double,
        }),
        Operand::Variable(Variable {
            variable_id: VariableId(2),
            ty: Ty::Double,
        }),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Boolean,
        },
    );
    expect!["  %var_0 = fcmp ole double %var_1, %var_2"]
        .assert_eq(&inst.to_qir(&Program::default()));
}
#[test]
fn fcmp_ogt_double_literals() {
    let inst = Instruction::Fcmp(
        FcmpConditionCode::OrderedAndGreaterThan,
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Boolean,
        },
    );
    expect!["  %var_0 = fcmp ogt double 3.141592653589793, 2.718281828459045"]
        .assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fcmp_ogt_double_variables() {
    let inst = Instruction::Fcmp(
        FcmpConditionCode::OrderedAndGreaterThan,
        Operand::Variable(Variable {
            variable_id: VariableId(1),
            ty: Ty::Double,
        }),
        Operand::Variable(Variable {
            variable_id: VariableId(2),
            ty: Ty::Double,
        }),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Boolean,
        },
    );
    expect!["  %var_0 = fcmp ogt double %var_1, %var_2"]
        .assert_eq(&inst.to_qir(&Program::default()));
}
#[test]
fn fcmp_oge_double_literals() {
    let inst = Instruction::Fcmp(
        FcmpConditionCode::OrderedAndGreaterThanOrEqual,
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Boolean,
        },
    );
    expect!["  %var_0 = fcmp oge double 3.141592653589793, 2.718281828459045"]
        .assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fcmp_oge_double_variables() {
    let inst = Instruction::Fcmp(
        FcmpConditionCode::OrderedAndGreaterThanOrEqual,
        Operand::Variable(Variable {
            variable_id: VariableId(1),
            ty: Ty::Double,
        }),
        Operand::Variable(Variable {
            variable_id: VariableId(2),
            ty: Ty::Double,
        }),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Boolean,
        },
    );
    expect!["  %var_0 = fcmp oge double %var_1, %var_2"]
        .assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fmul_double_literals() {
    let inst = Instruction::Fmul(
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    expect!["  %var_0 = fmul double 3.141592653589793, 2.718281828459045"]
        .assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fmul_double_variables() {
    let inst = Instruction::Fmul(
        Operand::Variable(Variable {
            variable_id: VariableId(1),
            ty: Ty::Double,
        }),
        Operand::Variable(Variable {
            variable_id: VariableId(2),
            ty: Ty::Double,
        }),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    expect!["  %var_0 = fmul double %var_1, %var_2"].assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fdiv_double_literals() {
    let inst = Instruction::Fdiv(
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    expect!["  %var_0 = fdiv double 3.141592653589793, 2.718281828459045"]
        .assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fdiv_double_variables() {
    let inst = Instruction::Fdiv(
        Operand::Variable(Variable {
            variable_id: VariableId(1),
            ty: Ty::Double,
        }),
        Operand::Variable(Variable {
            variable_id: VariableId(2),
            ty: Ty::Double,
        }),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    expect!["  %var_0 = fdiv double %var_1, %var_2"].assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fsub_double_literals() {
    let inst = Instruction::Fsub(
        Operand::Literal(Literal::Double(PI)),
        Operand::Literal(Literal::Double(E)),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    expect!["  %var_0 = fsub double 3.141592653589793, 2.718281828459045"]
        .assert_eq(&inst.to_qir(&Program::default()));
}

#[test]
fn fsub_double_variables() {
    let inst = Instruction::Fsub(
        Operand::Variable(Variable {
            variable_id: VariableId(1),
            ty: Ty::Double,
        }),
        Operand::Variable(Variable {
            variable_id: VariableId(2),
            ty: Ty::Double,
        }),
        Variable {
            variable_id: VariableId(0),
            ty: Ty::Double,
        },
    );
    expect!["  %var_0 = fsub double %var_1, %var_2"].assert_eq(&inst.to_qir(&Program::default()));
}
