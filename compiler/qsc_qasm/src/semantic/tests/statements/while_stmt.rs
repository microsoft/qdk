// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn single_stmt_body_creates_its_own_scope() {
    check_stmt_kinds(
        "
    int a = 3;
    while(true) int a = 1;
    ",
        &expect![[r#"
            ClassicalDeclarationStmt [5-15]:
                symbol_id: 8
                ty_span: [5-8]
                init_expr: Expr [13-14]:
                    ty: Int(None, false)
                    kind: Lit: Int(3)
            WhileLoop [20-42]:
                condition: Expr [26-30]:
                    ty: Bool(true)
                    kind: Lit: Bool(true)
                body: Stmt [32-42]:
                    annotations: <empty>
                    kind: ClassicalDeclarationStmt [32-42]:
                        symbol_id: 9
                        ty_span: [32-35]
                        init_expr: Expr [40-41]:
                            ty: Int(None, false)
                            kind: Lit: Int(1)
        "#]],
    );
}

#[test]
fn block_body_creates_its_own_scope() {
    check_stmt_kinds(
        "
    int a = 3;
    while(true) { int a = 1; }
    ",
        &expect![[r#"
            ClassicalDeclarationStmt [5-15]:
                symbol_id: 8
                ty_span: [5-8]
                init_expr: Expr [13-14]:
                    ty: Int(None, false)
                    kind: Lit: Int(3)
            WhileLoop [20-46]:
                condition: Expr [26-30]:
                    ty: Bool(true)
                    kind: Lit: Bool(true)
                body: Stmt [32-46]:
                    annotations: <empty>
                    kind: Block [32-46]:
                        Stmt [34-44]:
                            annotations: <empty>
                            kind: ClassicalDeclarationStmt [34-44]:
                                symbol_id: 9
                                ty_span: [34-37]
                                init_expr: Expr [42-43]:
                                    ty: Int(None, false)
                                    kind: Lit: Int(1)
        "#]],
    );
}

#[test]
fn condition_cast() {
    check_stmt_kinds(
        "while (1) true;",
        &expect![[r#"
            WhileLoop [0-15]:
                condition: Expr [7-8]:
                    ty: Bool(true)
                    kind: Cast [0-0]:
                        ty: Bool(true)
                        expr: Expr [7-8]:
                            ty: Int(None, true)
                            kind: Lit: Int(1)
                body: Stmt [10-15]:
                    annotations: <empty>
                    kind: ExprStmt [10-15]:
                        expr: Expr [10-14]:
                            ty: Bool(true)
                            kind: Lit: Bool(true)
        "#]],
    );
}
