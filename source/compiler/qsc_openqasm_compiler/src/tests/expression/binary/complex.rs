// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_stmt_to_qsharp;
use expect_test::expect;
use miette::Report;

#[test]
fn addition() -> miette::Result<(), Vec<Report>> {
    let source = "
        input complex[float] a;
        input complex[float] b;
        complex x = a + b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.PlusC(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn addition_assign_op() -> miette::Result<(), Vec<Report>> {
    let source = "
        input complex[float] a;
        complex x = 0.0;
        x += a;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        set x = Std.Math.PlusC(x, a);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn subtraction() -> miette::Result<(), Vec<Report>> {
    let source = "
        input complex[float] a;
        input complex[float] b;
        complex x = a - b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.MinusC(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn subtraction_assign_op() -> miette::Result<(), Vec<Report>> {
    let source = "
        input complex[float] a;
        complex x = 0.0;
        x -= a;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        set x = Std.Math.MinusC(x, a);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn multiplication() -> miette::Result<(), Vec<Report>> {
    let source = "
        input complex[float] a;
        input complex[float] b;
        complex x = a * b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.TimesC(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn multiplication_assign_op() -> miette::Result<(), Vec<Report>> {
    let source = "
        input complex[float] a;
        complex x = 0.0;
        x *= a;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        set x = Std.Math.TimesC(x, a);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn division() -> miette::Result<(), Vec<Report>> {
    let source = "
        input complex[float] a;
        input complex[float] b;
        complex x = a / b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.DividedByC(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn division_assign_op() -> miette::Result<(), Vec<Report>> {
    let source = "
        input complex[float] a;
        complex x = 0.0;
        x /= a;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        set x = Std.Math.DividedByC(x, a);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn power() -> miette::Result<(), Vec<Report>> {
    let source = "
        input complex[float] a;
        input complex[float] b;
        complex x = a ** b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.PowC(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn power_assign_op() -> miette::Result<(), Vec<Report>> {
    let source = "
        input complex[float] a;
        complex x = 0.0;
        x **= a;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        set x = Std.Math.PowC(x, a);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
