// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::v2::ToQir;
use expect_test::expect;
use qsc_rir::rir;

#[test]
fn load_integer_from_pointer() {
    let inst = rir::Instruction::Advanced(rir::AdvancedInstr::Load(
        rir::Variable::new_ptr(rir::VariableId(1)),
        rir::Variable::new_integer(rir::VariableId(0)),
    ));
    expect!["  %var_0 = load i64, ptr %var_1"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn load_bool_from_pointer() {
    let inst = rir::Instruction::Advanced(rir::AdvancedInstr::Load(
        rir::Variable::new_ptr(rir::VariableId(1)),
        rir::Variable::new_boolean(rir::VariableId(0)),
    ));
    expect!["  %var_0 = load i1, ptr %var_1"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn load_double_from_pointer() {
    let inst = rir::Instruction::Advanced(rir::AdvancedInstr::Load(
        rir::Variable::new_ptr(rir::VariableId(1)),
        rir::Variable::new_double(rir::VariableId(0)),
    ));
    expect!["  %var_0 = load double, ptr %var_1"].assert_eq(&inst.to_qir(&rir::Program::default()));
}

#[test]
fn load_pointer_from_pointer() {
    let inst = rir::Instruction::Advanced(rir::AdvancedInstr::Load(
        rir::Variable::new_ptr(rir::VariableId(1)),
        rir::Variable::new_ptr(rir::VariableId(0)),
    ));
    expect!["  %var_0 = load ptr, ptr %var_1"].assert_eq(&inst.to_qir(&rir::Program::default()));
}
