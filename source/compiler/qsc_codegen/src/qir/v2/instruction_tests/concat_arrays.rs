// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::v2::ToQir;
use expect_test::expect;
use qsc_rir::rir;

#[test]
fn concat_two_arrays() {
    let inst = rir::Instruction::ConcatArrays(
        rir::Variable::new_array(rir::VariableId(0), 2, rir::Prim::Integer),
        rir::Variable::new_array(rir::VariableId(1), 1, rir::Prim::Integer),
        rir::Variable::new_array(rir::VariableId(2), 3, rir::Prim::Integer),
    );
    let qir = &inst.to_qir(&rir::Program::default());
    let mut lines = qir.lines();
    expect!["  %var_2_0_src = getelementptr [2 x i64], ptr %var_0, i64 0, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_2_0 = load i64, ptr %var_2_0_src"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_2_0_dst = getelementptr [3 x i64], ptr %var_2, i64 0, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 %var_2_0, ptr %var_2_0_dst"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_2_1_src = getelementptr [2 x i64], ptr %var_0, i64 0, i64 1"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_2_1 = load i64, ptr %var_2_1_src"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_2_1_dst = getelementptr [3 x i64], ptr %var_2, i64 0, i64 1"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 %var_2_1, ptr %var_2_1_dst"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_2_2_src = getelementptr [1 x i64], ptr %var_1, i64 0, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_2_2 = load i64, ptr %var_2_2_src"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_2_2_dst = getelementptr [3 x i64], ptr %var_2, i64 0, i64 2"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 %var_2_2, ptr %var_2_2_dst"]
        .assert_eq(lines.next().expect("line should exist"));
    assert_eq!(lines.next(), None);
}
