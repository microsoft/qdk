// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_stmt_to_qsharp;
use expect_test::expect;
use miette::Report;

#[test]
fn bit_with_no_initializer() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit b;
    "#;

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
            mutable b = Zero;
        "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn bit_with_initializer_lit_one() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit b = 1;
    "#;

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
            mutable b = One;
        "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn bit_with_initializer_lit_zero() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit b = 0;
    "#;

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
            mutable b = Zero;
        "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_bit_with_initializer_lit_one() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit b = 1;
    "#;

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
            let b = One;
        "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_bit_with_initializer_lit_zero() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit b = 0;
    "#;

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
            let b = Zero;
        "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}
