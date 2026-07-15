// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn if_branch_creates_its_own_scope() {
    check_stmt_kinds(
        "
    int a = 2;
    if (true) int a = 1;
    ",
        &expect![[r#"
            ClassicalDeclarationStmt [5-15]:
                symbol_id: 8
                ty_span: [5-8]
                ty_exprs: <empty>
                init_expr: Expr [13-14]:
                    ty: int
                    kind: Lit: Int(2)
            IfStmt [20-40]:
                condition: Expr [24-28]:
                    ty: const bool
                    kind: Lit: Bool(true)
                if_body: Stmt [30-40]:
                    annotations: <empty>
                    kind: Block [30-40]:
                        Stmt [30-40]:
                            annotations: <empty>
                            kind: ClassicalDeclarationStmt [30-40]:
                                symbol_id: 9
                                ty_span: [30-33]
                                ty_exprs: <empty>
                                init_expr: Expr [38-39]:
                                    ty: int
                                    kind: Lit: Int(1)
                else_body: <none>
        "#]],
    );
}

#[test]
fn else_branch_creates_its_own_scope() {
    check_stmt_kinds(
        "
    int a = 2;
    if (true) {}
    else int a = 1;
    ",
        &expect![[r#"
            ClassicalDeclarationStmt [5-15]:
                symbol_id: 8
                ty_span: [5-8]
                ty_exprs: <empty>
                init_expr: Expr [13-14]:
                    ty: int
                    kind: Lit: Int(2)
            IfStmt [20-52]:
                condition: Expr [24-28]:
                    ty: const bool
                    kind: Lit: Bool(true)
                if_body: Stmt [30-32]:
                    annotations: <empty>
                    kind: Block [30-32]: <empty>
                else_body: Stmt [42-52]:
                    annotations: <empty>
                    kind: Block [42-52]:
                        Stmt [42-52]:
                            annotations: <empty>
                            kind: ClassicalDeclarationStmt [42-52]:
                                symbol_id: 9
                                ty_span: [42-45]
                                ty_exprs: <empty>
                                init_expr: Expr [50-51]:
                                    ty: int
                                    kind: Lit: Int(1)
        "#]],
    );
}

#[test]
fn branch_block_creates_a_new_scope() {
    check_stmt_kinds(
        "
    int a = 2;
    if (true) { int a = 1; }
    ",
        &expect![[r#"
            ClassicalDeclarationStmt [5-15]:
                symbol_id: 8
                ty_span: [5-8]
                ty_exprs: <empty>
                init_expr: Expr [13-14]:
                    ty: int
                    kind: Lit: Int(2)
            IfStmt [20-44]:
                condition: Expr [24-28]:
                    ty: const bool
                    kind: Lit: Bool(true)
                if_body: Stmt [30-44]:
                    annotations: <empty>
                    kind: Block [30-44]:
                        Stmt [32-42]:
                            annotations: <empty>
                            kind: ClassicalDeclarationStmt [32-42]:
                                symbol_id: 9
                                ty_span: [32-35]
                                ty_exprs: <empty>
                                init_expr: Expr [40-41]:
                                    ty: int
                                    kind: Lit: Int(1)
                else_body: <none>
        "#]],
    );
}

#[test]
fn if_scope_and_else_scope_are_different() {
    check_stmt_kinds(
        "
    int a = 2;
    if (true) { int a = 1; }
    else { int a = 2; }
    ",
        &expect![[r#"
            ClassicalDeclarationStmt [5-15]:
                symbol_id: 8
                ty_span: [5-8]
                ty_exprs: <empty>
                init_expr: Expr [13-14]:
                    ty: int
                    kind: Lit: Int(2)
            IfStmt [20-68]:
                condition: Expr [24-28]:
                    ty: const bool
                    kind: Lit: Bool(true)
                if_body: Stmt [30-44]:
                    annotations: <empty>
                    kind: Block [30-44]:
                        Stmt [32-42]:
                            annotations: <empty>
                            kind: ClassicalDeclarationStmt [32-42]:
                                symbol_id: 9
                                ty_span: [32-35]
                                ty_exprs: <empty>
                                init_expr: Expr [40-41]:
                                    ty: int
                                    kind: Lit: Int(1)
                else_body: Stmt [54-68]:
                    annotations: <empty>
                    kind: Block [54-68]:
                        Stmt [56-66]:
                            annotations: <empty>
                            kind: ClassicalDeclarationStmt [56-66]:
                                symbol_id: 10
                                ty_span: [56-59]
                                ty_exprs: <empty>
                                init_expr: Expr [64-65]:
                                    ty: int
                                    kind: Lit: Int(2)
        "#]],
    );
}

#[test]
fn condition_cast() {
    check_stmt_kinds(
        "if (1) true;",
        &expect![[r#"
            IfStmt [0-12]:
                condition: Expr [4-5]:
                    ty: const bool
                    kind: Cast [4-5]:
                        ty: const bool
                        ty_exprs: <empty>
                        expr: Expr [4-5]:
                            ty: const int
                            kind: Lit: Int(1)
                        kind: Implicit
                if_body: Stmt [7-12]:
                    annotations: <empty>
                    kind: Block [7-12]:
                        Stmt [7-12]:
                            annotations: <empty>
                            kind: ExprStmt [7-12]:
                                expr: Expr [7-11]:
                                    ty: const bool
                                    kind: Lit: Bool(true)
                else_body: <none>
        "#]],
    );
}
