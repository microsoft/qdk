// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::stmt::parse;
use crate::parser::tests::check;
use expect_test::expect;

#[test]
fn simple_alias() {
    check(
        parse,
        "let x = a;",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: AliasDeclStmt [0-10]:
                    ident: Ident [4-5] "x"
                    exprs:
                        Expr [8-9]: Ident [8-9] "a""#]],
    );
}

#[test]
fn concatenation_alias() {
    check(
        parse,
        "let x = a[1:2] ++ b ++ c[1:2:3];",
        &expect![[r#"
            Stmt [0-32]:
                annotations: <empty>
                kind: AliasDeclStmt [0-32]:
                    ident: Ident [4-5] "x"
                    exprs:
                        Expr [8-14]: IndexExpr [8-14]:
                            collection: Expr [8-9]: Ident [8-9] "a"
                            index: IndexList [10-13]:
                                values:
                                    Range [10-13]:
                                        start: Expr [10-11]: Lit: Int(1)
                                        step: <none>
                                        end: Expr [12-13]: Lit: Int(2)
                        Expr [18-19]: Ident [18-19] "b"
                        Expr [23-31]: IndexExpr [23-31]:
                            collection: Expr [23-24]: Ident [23-24] "c"
                            index: IndexList [25-30]:
                                values:
                                    Range [25-30]:
                                        start: Expr [25-26]: Lit: Int(1)
                                        step: Expr [27-28]: Lit: Int(2)
                                        end: Expr [29-30]: Lit: Int(3)"#]],
    );
}
