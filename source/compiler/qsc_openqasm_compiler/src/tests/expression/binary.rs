// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::tests::compile_qasm_to_qsharp;

mod angle;
mod arithmetic_conversions;
mod comparison;
mod complex;
mod ident;
mod literal;

#[test]
fn binary_expr_fail_parse_missing_op() {
    let source = r#"
        input int a;
        input int b;
        a b;
    "#;

    assert!(compile_qasm_to_qsharp(source).is_err());
}

#[test]
fn binary_expr_fail_parse_missing_lhs() {
    let source = r#"
        input int b;
        < b;
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![r#"expected EOF, found `<`"#].assert_eq(&errors[0].to_string());
}

#[test]
fn binary_expr_fail_parse_missing_rhs() {
    let source = r#"
        input int a;
        a <;
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![r#"expected expression, found `;`"#].assert_eq(&errors[0].to_string());
}
