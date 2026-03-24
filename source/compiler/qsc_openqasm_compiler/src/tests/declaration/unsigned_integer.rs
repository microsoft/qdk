// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_stmt_to_qsharp;

use expect_test::expect;
use miette::Report;

#[test]
fn implicit_bitness_int_default_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        uint x;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        mutable x = 0;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_lit_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const uint x = 42;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        let x = 42;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_hex_cap_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        uint x = 0XFa_1F;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        mutable x = 64031;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_hex_low_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const uint x = 0xFa_1F;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        let x = 64031;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_hex_cap_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const uint x = 0XFa_1F;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        let x = 64031;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_octal_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        uint x = 0o42;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        mutable x = 34;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_octal_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const uint x = 0o42;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        let x = 34;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_binary_low_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        uint x = 0b1001_1001;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        mutable x = 153;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_binary_cap_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        uint x = 0B1010;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        mutable x = 10;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_binary_low_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const uint x = 0b1001_1001;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        let x = 153;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_binary_cap_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const uint x = 0B1010;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        let x = 10;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_formatted_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        uint x = 2_0_00;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        mutable x = 2000;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_formatted_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const uint x = 2_0_00;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        let x = 2000;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn explicit_bitness_int_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        uint[10] x;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        mutable x = 0;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
#[ignore = "not implemented"]
fn assigning_uint_to_negative_lit_results_in_semantic_error() {
    let source = "
        const uint[10] x = -42;
    ";

    let Err(errors) = compile_qasm_stmt_to_qsharp(source) else {
        panic!("Expected error");
    };
    expect![[
        r#"Cannot assign a value of Negative Int type to a classical variable of UInt(Some(10), True) type."#
    ]]
    .assert_eq(&errors[0].to_string());
}
