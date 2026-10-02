// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::v2::ToQir;
use expect_test::expect;
use qsc_rir::rir;

#[test]
fn slice_array_variable() {
    let inst = rir::Instruction::SliceArray(
        rir::Variable::new_array(rir::VariableId(0), 4, rir::Prim::Integer),
        0,
        2,
        3,
        rir::Variable::new_array(rir::VariableId(1), 2, rir::Prim::Integer),
    );
    // Each slice array produces 4N lines, where N is the size of the slice (or destination array).
    let qir = inst.to_qir(&rir::Program::default());
    let mut lines = qir.lines();
    expect!["  %var_1_0_src = getelementptr [4 x i64], ptr %var_0, i64 0, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_0 = load i64, ptr %var_1_0_src"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_0_dst = getelementptr [2 x i64], ptr %var_1, i64 0, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 %var_1_0, ptr %var_1_0_dst"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_1_src = getelementptr [4 x i64], ptr %var_0, i64 0, i64 2"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_1 = load i64, ptr %var_1_1_src"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_1_dst = getelementptr [2 x i64], ptr %var_1, i64 0, i64 1"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 %var_1_1, ptr %var_1_1_dst"]
        .assert_eq(lines.next().expect("line should exist"));
    assert_eq!(lines.next(), None);
}

#[test]
fn slice_array_variable_reversed() {
    let inst = rir::Instruction::SliceArray(
        rir::Variable::new_array(rir::VariableId(0), 4, rir::Prim::Integer),
        3,
        -2,
        0,
        rir::Variable::new_array(rir::VariableId(1), 2, rir::Prim::Integer),
    );
    // Each slice array produces 4N lines, where N is the size of the slice (or destination array).
    let qir = inst.to_qir(&rir::Program::default());
    let mut lines = qir.lines();
    expect!["  %var_1_0_src = getelementptr [4 x i64], ptr %var_0, i64 0, i64 3"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_0 = load i64, ptr %var_1_0_src"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_0_dst = getelementptr [2 x i64], ptr %var_1, i64 0, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 %var_1_0, ptr %var_1_0_dst"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_1_src = getelementptr [4 x i64], ptr %var_0, i64 0, i64 1"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_1 = load i64, ptr %var_1_1_src"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_1_dst = getelementptr [2 x i64], ptr %var_1, i64 0, i64 1"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 %var_1_1, ptr %var_1_1_dst"]
        .assert_eq(lines.next().expect("line should exist"));
    assert_eq!(lines.next(), None);
}

#[test]
fn slice_array_variable_negative_index() {
    let inst = rir::Instruction::SliceArray(
        rir::Variable::new_array(rir::VariableId(0), 4, rir::Prim::Integer),
        -4,
        2,
        -1,
        rir::Variable::new_array(rir::VariableId(1), 2, rir::Prim::Integer),
    );
    // Each slice array produces 4N lines, where N is the size of the slice (or destination array).
    let qir = inst.to_qir(&rir::Program::default());
    let mut lines = qir.lines();
    expect!["  %var_1_0_src = getelementptr [4 x i64], ptr %var_0, i64 1, i64 -4"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_0 = load i64, ptr %var_1_0_src"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_0_dst = getelementptr [2 x i64], ptr %var_1, i64 0, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 %var_1_0, ptr %var_1_0_dst"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_1_src = getelementptr [4 x i64], ptr %var_0, i64 1, i64 -2"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_1 = load i64, ptr %var_1_1_src"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_1_dst = getelementptr [2 x i64], ptr %var_1, i64 0, i64 1"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 %var_1_1, ptr %var_1_1_dst"]
        .assert_eq(lines.next().expect("line should exist"));
    assert_eq!(lines.next(), None);
}

#[test]
fn slice_array_variable_negative_index_reversed() {
    let inst = rir::Instruction::SliceArray(
        rir::Variable::new_array(rir::VariableId(0), 4, rir::Prim::Integer),
        -1,
        -2,
        -4,
        rir::Variable::new_array(rir::VariableId(1), 2, rir::Prim::Integer),
    );
    // Each slice array produces 4N lines, where N is the size of the slice (or destination array).
    let qir = inst.to_qir(&rir::Program::default());
    let mut lines = qir.lines();
    expect!["  %var_1_0_src = getelementptr [4 x i64], ptr %var_0, i64 1, i64 -1"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_0 = load i64, ptr %var_1_0_src"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_0_dst = getelementptr [2 x i64], ptr %var_1, i64 0, i64 0"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 %var_1_0, ptr %var_1_0_dst"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_1_src = getelementptr [4 x i64], ptr %var_0, i64 1, i64 -3"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_1 = load i64, ptr %var_1_1_src"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  %var_1_1_dst = getelementptr [2 x i64], ptr %var_1, i64 0, i64 1"]
        .assert_eq(lines.next().expect("line should exist"));
    expect!["  store i64 %var_1_1, ptr %var_1_1_dst"]
        .assert_eq(lines.next().expect("line should exist"));
    assert_eq!(lines.next(), None);
}
