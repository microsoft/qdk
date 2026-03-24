// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::parser::tests::check;

use crate::parser::stmt::parse;

#[test]
fn creg_decl() {
    check(
        parse,
        "creg c;",
        &expect![[r#"
            Stmt [0-7]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-7]:
                    type: ScalarType [0-7]: BitType [0-7]:
                        size: <none>
                    ident: Ident [5-6] "c"
                    init_expr: <none>"#]],
    );
}

#[test]
fn creg_array_decl() {
    check(
        parse,
        "creg c[n];",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-10]:
                    type: ScalarType [0-10]: BitType [0-10]:
                        size: Expr [7-8]: Ident [7-8] "n"
                    ident: Ident [5-6] "c"
                    init_expr: <none>"#]],
    );
}

#[test]
fn qreg_decl() {
    check(
        parse,
        "qreg q;",
        &expect![[r#"
            Stmt [0-7]:
                annotations: <empty>
                kind: QubitDeclaration [0-7]:
                    ty: QubitType [0-7]:
                        size: <none>
                    ident: Ident [5-6] "q""#]],
    );
}

#[test]
fn qreg_array_decl() {
    check(
        parse,
        "qreg q[n];",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: QubitDeclaration [0-10]:
                    ty: QubitType [0-10]:
                        size: Expr [7-8]: Ident [7-8] "n"
                    ident: Ident [5-6] "q""#]],
    );
}
