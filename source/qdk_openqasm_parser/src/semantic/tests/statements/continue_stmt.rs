// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn for_loop() {
    check_stmt_kinds(
        "for int i in {1, 2, 3} continue;",
        &expect![[r#"
            ForStmt [0-32]:
                loop_variable: 8
                ty_exprs: <empty>
                iterable: Set [13-22]:
                    values:
                        Expr [14-15]:
                            ty: const int
                            kind: Lit: Int(1)
                        Expr [17-18]:
                            ty: const int
                            kind: Lit: Int(2)
                        Expr [20-21]:
                            ty: const int
                            kind: Lit: Int(3)
                body: Stmt [23-32]:
                    annotations: <empty>
                    kind: Block [23-32]:
                        Stmt [23-32]:
                            annotations: <empty>
                            kind: ContinueStmt [23-32]:
        "#]],
    );
}

#[test]
fn while_loop() {
    check_stmt_kinds(
        "while (true) continue;",
        &expect![[r#"
            WhileLoop [0-22]:
                condition: Expr [7-11]:
                    ty: const bool
                    kind: Lit: Bool(true)
                body: Stmt [13-22]:
                    annotations: <empty>
                    kind: Block [13-22]:
                        Stmt [13-22]:
                            annotations: <empty>
                            kind: ContinueStmt [13-22]:
        "#]],
    );
}

#[test]
fn nested_scopes() {
    check_stmt_kinds(
        "while (true) { { continue; } }",
        &expect![[r#"
            WhileLoop [0-30]:
                condition: Expr [7-11]:
                    ty: const bool
                    kind: Lit: Bool(true)
                body: Stmt [13-30]:
                    annotations: <empty>
                    kind: Block [13-30]:
                        Stmt [15-28]:
                            annotations: <empty>
                            kind: Block [15-28]:
                                Stmt [17-26]:
                                    annotations: <empty>
                                    kind: ContinueStmt [17-26]:
        "#]],
    );
}

#[test]
fn continue_in_non_loop_scope_fails() {
    check_stmt_kinds(
        "continue;",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-9]:
                        annotations: <empty>
                        kind: Err

            [Qasm.Lowerer.InvalidScope

              x continue can only appear in loop scopes
               ,-[test:1:1]
             1 | continue;
               : ^^^^^^^^^
               `----
            ]"#]],
    );
}

#[test]
fn intermediate_def_scope_fails() {
    check_stmt_kinds(
        "
        while (true) {
            def f() { continue; }
        }
        ",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-67]:
                        annotations: <empty>
                        kind: WhileLoop [9-67]:
                            condition: Expr [16-20]:
                                ty: const bool
                                kind: Lit: Bool(true)
                            body: Stmt [22-67]:
                                annotations: <empty>
                                kind: Block [22-67]:
                                    Stmt [36-57]:
                                        annotations: <empty>
                                        kind: DefStmt [36-57]:
                                            symbol_id: 8
                                            has_qubit_params: false
                                            parameters: <empty>
                                            return_type_span: [0-0]
                                            return_ty_exprs: <empty>
                                            body: Block [44-57]:
                                                Stmt [46-55]:
                                                    annotations: <empty>
                                                    kind: Err

            [Qasm.Lowerer.DefDeclarationInNonGlobalScope

              x def declarations must be done in global scope
               ,-[test:3:13]
             2 |         while (true) {
             3 |             def f() { continue; }
               :             ^^^^^^^^^^^^^^^^^^^^^
             4 |         }
               `----
            , Qasm.Lowerer.InvalidScope

              x continue can only appear in loop scopes
               ,-[test:3:23]
             2 |         while (true) {
             3 |             def f() { continue; }
               :                       ^^^^^^^^^
             4 |         }
               `----
            ]"#]],
    );
}
