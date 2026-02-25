// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::ToQir;
use expect_test::expect;
use qsc_rir::rir;

#[test]
#[should_panic(expected = "phi instruction should have at least one argument")]
fn phi_with_empty_args() {
    let args = [];
    let inst = rir::Instruction::Phi(
        args.to_vec(),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    let _ = &inst.to_qir(&rir::Program::default());
}

#[test]
fn phi_with_single_arg() {
    let args = [(
        rir::Operand::Variable(rir::Variable {
            variable_id: rir::VariableId(13),
            ty: rir::Ty::Integer,
        }),
        rir::BlockId(3),
    )];
    let inst = rir::Instruction::Phi(
        args.to_vec(),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = phi i64 [%var_13, %block_3]"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn phi_with_multiple_args() {
    let args = [
        (
            rir::Operand::Variable(rir::Variable {
                variable_id: rir::VariableId(13),
                ty: rir::Ty::Integer,
            }),
            rir::BlockId(3),
        ),
        (
            rir::Operand::Variable(rir::Variable {
                variable_id: rir::VariableId(2),
                ty: rir::Ty::Integer,
            }),
            rir::BlockId(7),
        ),
    ];
    let inst = rir::Instruction::Phi(
        args.to_vec(),
        rir::Variable {
            variable_id: rir::VariableId(0),
            ty: rir::Ty::Integer,
        },
    );
    expect!["  %var_0 = phi i64 [%var_13, %block_3], [%var_2, %block_7]"]
        .assert_eq(&inst.to_qir(&rir::Program::default()));
}
