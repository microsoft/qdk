// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::stmt::parse;
use crate::parser::tests::check;
use expect_test::expect;

#[test]
fn alias_decl_stmt() {
    check(parse, "let x = a[1:2] ++ b ++ c[1:2:3]", &expect![[r#"
        Stmt [0-31]
            StmtKind: Alias [0-31]: Ident [4-5] "x"
                Expr [8-14]: IndexExpr [9-14]: Expr [8-9]: Ident [8-9] "a", IndexElement:
                    Range: [10-13]
                        start: Expr [10-11]: Lit: Int(1)
                        <no step>
                        end: Expr [12-13]: Lit: Int(2)
                Expr [18-19]: Ident [18-19] "b"
                Expr [23-31]: IndexExpr [24-31]: Expr [23-24]: Ident [23-24] "c", IndexElement:
                    Range: [25-30]
                        start: Expr [25-26]: Lit: Int(1)
                        step: Expr [27-28]: Lit: Int(2)
                        end: Expr [29-30]: Lit: Int(3)"#]]);
}
