// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use miette::Report;

use crate::tests::compile_qasm_to_qsharp;

#[test]
fn to_int_decl_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit[5] reg;
        int b = reg;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable reg = [Zero, Zero, Zero, Zero, Zero];
        mutable b = Std.OpenQASM.Convert.ResultArrayAsIntBE(reg);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_int_assignment_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit[5] reg;
        int a;
        a = reg;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable reg = [Zero, Zero, Zero, Zero, Zero];
        mutable a = 0;
        set a = Std.OpenQASM.Convert.ResultArrayAsIntBE(reg);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_int_with_equal_width_in_assignment_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit[5] reg;
        int[5] a;
        a = reg;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable reg = [Zero, Zero, Zero, Zero, Zero];
        mutable a = 0;
        set a = Std.OpenQASM.Convert.ResultArrayAsIntBE(reg);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_int_with_equal_width_in_decl_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit[5] reg;
        int[5] a = reg;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable reg = [Zero, Zero, Zero, Zero, Zero];
        mutable a = Std.OpenQASM.Convert.ResultArrayAsIntBE(reg);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_int_with_higher_width_implicitly_fails() {
    let source = "
        int[6] a;
        bit[5] reg;
        a = reg;
    ";

    let Err(error) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error")
    };

    expect!["cannot cast expression of type bit[5] to type int[6]"]
        .assert_eq(&error[0].to_string());
}

#[test]
fn to_int_with_higher_width_decl_implicitly_fails() {
    let source = "
        bit[5] reg;
        int[6] a = reg;
    ";

    let Err(error) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error")
    };

    expect!["cannot cast expression of type bit[5] to type int[6]"]
        .assert_eq(&error[0].to_string());
}

#[test]
fn to_int_with_lower_width_implicitly_fails() {
    let source = "
        input int[4] a;
        bit[5] reg;
        a = reg;
    ";

    let Err(error) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error")
    };

    expect!["cannot cast expression of type bit[5] to type int[4]"]
        .assert_eq(&error[0].to_string());
}

#[test]
fn to_int_with_lower_width_decl_implicitly_fails() {
    let source = "
        bit[5] reg;
        int[4] a = reg;
    ";

    let Err(error) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error")
    };

    expect!["cannot cast expression of type bit[5] to type int[4]"]
        .assert_eq(&error[0].to_string());
}
