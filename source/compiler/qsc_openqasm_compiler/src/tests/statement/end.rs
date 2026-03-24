// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_to_qsharp;
use expect_test::expect;
use miette::Report;

#[test]
fn end_can_be_in_nested_scope() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        int sum = 0;
        for int i in {1, 5, 10} {
            end;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable sum = 0;
        for i : Int in [1, 5, 10] {
            fail "end";
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn end_can_be_in_global_scope() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        end;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        fail "end";
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
