// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use miette::Report;

use crate::tests::compile_qasm_stmt_to_qsharp;

#[test]
fn int_float_lhs_promoted_to_float() -> miette::Result<(), Vec<Report>> {
    let source = "
        5 * 0.3;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        5. * 0.3;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn float_int_rhs_promoted_to_float() -> miette::Result<(), Vec<Report>> {
    let source = "
        0.3 * 5;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        0.3 * 5.;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn int_int_stays_int() -> miette::Result<(), Vec<Report>> {
    let source = "
        3 * 5;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        3 * 5;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn float_float_stays_float() -> miette::Result<(), Vec<Report>> {
    let source = "
        3. * 5.;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        3. * 5.;
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}
