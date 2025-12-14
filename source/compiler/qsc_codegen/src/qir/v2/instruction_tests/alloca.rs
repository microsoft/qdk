// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::v2::ToQir;
use expect_test::expect;
use qsc_rir::rir;

#[test]
fn alloca_integer_without_size() {
    let inst = rir::Instruction::Advanced(rir::AdvancedInstr::Alloca(rir::Variable::new_integer(
        rir::VariableId(0),
    )));
    expect!["  %var_0 = alloca i64"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn alloca_bool_without_size() {
    let inst = rir::Instruction::Advanced(rir::AdvancedInstr::Alloca(rir::Variable::new_boolean(
        rir::VariableId(0),
    )));
    expect!["  %var_0 = alloca i1"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn alloca_double_without_size() {
    let inst = rir::Instruction::Advanced(rir::AdvancedInstr::Alloca(rir::Variable::new_double(
        rir::VariableId(0),
    )));
    expect!["  %var_0 = alloca double"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn alloca_pointer_without_size() {
    let inst = rir::Instruction::Advanced(rir::AdvancedInstr::Alloca(rir::Variable::new_ptr(
        rir::VariableId(0),
    )));
    expect!["  %var_0 = alloca ptr"].assert_eq(&inst.to_qir(&rir::Program::default()));
}
