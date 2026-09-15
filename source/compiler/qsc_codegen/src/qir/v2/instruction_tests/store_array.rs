// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::v2::ToQir;
use expect_test::expect;
use qsc_rir::rir;

#[test]
fn store_integer_literals_to_array_variable() {
    let inst = rir::Instruction::StoreArray(
        vec![
            rir::Operand::Literal(rir::Literal::Integer(5)),
            rir::Operand::Literal(rir::Literal::Integer(6)),
        ],
        rir::Variable::new_array(rir::VariableId(0), 2, rir::Prim::Integer),
    );
    // Each store array produces 2N lines, where N is the size of the array.
    let qir = inst.to_qir(&rir::Program::default());
    let mut lines = qir.lines();
    expect!["  %var_0_0 = getelementptr [2 x i64], ptr %var_0, i64 0, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 5, ptr %var_0_0"].assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_0_1 = getelementptr [2 x i64], ptr %var_0, i64 0, i64 1"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 6, ptr %var_0_1"].assert_eq(lines.next().expect("line should exist"));
    assert_eq!(lines.next(), None);
}

#[test]
fn store_mix_of_integer_literal_and_variable_to_array_variable() {
    let inst = rir::Instruction::StoreArray(
        vec![
            rir::Operand::Literal(rir::Literal::Integer(5)),
            rir::Operand::Variable(rir::Variable::new_integer(rir::VariableId(1))),
        ],
        rir::Variable::new_array(rir::VariableId(0), 2, rir::Prim::Integer),
    );
    // Each store array produces 2N lines, where N is the size of the array.
    let qir = inst.to_qir(&rir::Program::default());
    let mut lines = qir.lines();
    expect!["  %var_0_0 = getelementptr [2 x i64], ptr %var_0, i64 0, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 5, ptr %var_0_0"].assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_0_1 = getelementptr [2 x i64], ptr %var_0, i64 0, i64 1"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 %var_1, ptr %var_0_1"].assert_eq(lines.next().expect("line should exist"));
    assert_eq!(lines.next(), None);
}
