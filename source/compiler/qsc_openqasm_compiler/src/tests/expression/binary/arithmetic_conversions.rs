// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use miette::Report;

use crate::tests::compile_qasm_to_qsharp;

#[test]
fn int_idents_without_width_can_be_multiplied() -> miette::Result<(), Vec<Report>> {
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
fn int_idents_with_same_width_can_be_multiplied() -> miette::Result<(), Vec<Report>> {
    let source = "
        int[32] x = 5;
        int[32] y = 3;
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
fn int_idents_with_different_width_can_be_multiplied() -> miette::Result<(), Vec<Report>> {
    let source = "
        int[32] x = 5;
        int[64] y = 3;
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
fn multiplying_int_idents_with_different_width_result_in_higher_width_result()
-> miette::Result<(), Vec<Report>> {
    let source = "
        int[32] x = 5;
        int[64] y = 3;
        int[64] z = x * y;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 5;
        mutable y = 3;
        mutable z = x * y;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn multiplying_int_idents_with_different_width_result_in_no_width_result()
-> miette::Result<(), Vec<Report>> {
    let source = "
        int[32] x = 5;
        int[64] y = 3;
        int z = x * y;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 5;
        mutable y = 3;
        mutable z = x * y;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn multiplying_int_idents_with_width_greater_than_64_result_in_bigint_result()
-> miette::Result<(), Vec<Report>> {
    let source = "
        int[32] x = 5;
        int[64] y = 3;
        int[67] z = x * y;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 5;
        mutable y = 3;
        mutable z = Std.Convert.IntAsBigInt(x * y);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
