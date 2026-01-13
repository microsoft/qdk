// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
#[ignore = "pow builtin collides with the pow gate modifier"]
fn pow_int() {
    let source = r"
        pow(2, 3);
    ";

    check_stmt_kinds(source, &expect![[r#""#]]);
}

#[test]
#[ignore = "pow builtin collides with pow gate modifier"]
fn pow_float() {
    let source = r"
        pow(2., 3.);
    ";

    check_stmt_kinds(source, &expect![[r#""#]]);
}

#[test]
#[ignore = "pow builtin collides with pow gate modifier"]
fn pow_complex() {
    let source = r"
        pow(2 im, 3);
    ";

    check_stmt_kinds(source, &expect![[r#""#]]);
}
