// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::v2::ToQir;
use expect_test::expect;
use qsc_rir::rir;

#[test]
fn copy_existing_array_into_new_array() {
    let inst = rir::Instruction::CopyArray(
        rir::Variable::new_array(rir::VariableId(0), 2, rir::Prim::Integer),
        rir::Variable::new_array(rir::VariableId(1), 2, rir::Prim::Integer),
    );
    let qir = &inst.to_qir(&rir::Program::default());
    expect!["  store [2 x i64] %var_0, ptr %var_1"].assert_eq(qir);
}
