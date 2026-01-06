// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use miette::Report;

use crate::tests::compile_qasm_to_qsharp;

#[test]
fn mutable_int_idents_without_width_can_be_multiplied() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 5;
        int y = 3;
        x * y;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 5;
        mutable y = 3;
        x * y;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_int_idents_without_width_can_be_multiplied() -> miette::Result<(), Vec<Report>> {
    let source = "
        const int x = 5;
        const int y = 3;
        x * y;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let x = 5;
        let y = 3;
        x * y;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_int_idents_widthless_lhs_can_be_multiplied_by_explicit_width_int()
-> miette::Result<(), Vec<Report>> {
    let source = "
        const int[32] x = 5;
        const int y = 3;
        x * y;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let x = 5;
        let y = 3;
        x * y;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
