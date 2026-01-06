// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_stmt_to_qsharp;

use expect_test::expect;
use miette::Report;

#[test]
fn implicit_bitness_int_negative_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = -42;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = -42;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_const_negative_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const int x = -42;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = -42;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_default_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = 0;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_lit_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const int x = 42;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = 42;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_hex_cap_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 0XFa_1F;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = 64031;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_hex_low_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const int x = 0xFa_1F;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = 64031;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_hex_cap_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const int x = 0XFa_1F;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = 64031;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_octal_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 0o42;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = 34;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_octal_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const int x = 0o42;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = 34;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_binary_low_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 0b1001_1001;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = 153;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_binary_cap_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 0B1010;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = 10;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_binary_low_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const int x = 0b1001_1001;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = 153;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_binary_cap_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const int x = 0B1010;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = 10;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_formatted_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 2_0_00;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = 2000;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_formatted_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const int x = 2_0_00;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = 2000;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn explicit_bitness_int_default_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        int[10] x;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = 0;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn explicit_bitness_int_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        int[10] x = 42;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = 42;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_explicit_bitness_int_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const int[10] x = 42;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = 42;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_negative_float_decl_creates_truncation_call()
-> miette::Result<(), Vec<Report>> {
    let source = "
        int x = -42.;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.Truncate(-42.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
