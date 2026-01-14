// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_stmt_to_qsharp;

use expect_test::expect;
use miette::Report;

#[test]
fn bool_default_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        bool x;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        mutable x = false;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn bool_true_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        bool x = true;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        mutable x = true;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_bool_true_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const bool x = true;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        let x = true;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn bool_false_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        bool x = false;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        mutable x = false;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_bool_false_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const bool x = false;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        let x = false;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}
