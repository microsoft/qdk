// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn for_loop() {
    check_stmt_kinds(
        "for int i in {1, 2, 3} break;",
        &expect![[r#"
            ForStmt [0-29]:
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
                body: Stmt [23-29]:
                    annotations: <empty>
                    kind: Block [23-29]:
                        Stmt [23-29]:
                            annotations: <empty>
                            kind: BreakStmt [23-29]:
        "#]],
    );
}

#[test]
fn while_loop() {
    check_stmt_kinds(
        "while (true) break;",
        &expect![[r#"
            WhileLoop [0-19]:
                condition: Expr [7-11]:
                    ty: const bool
                    kind: Lit: Bool(true)
                body: Stmt [13-19]:
                    annotations: <empty>
                    kind: Block [13-19]:
                        Stmt [13-19]:
                            annotations: <empty>
                            kind: BreakStmt [13-19]:
        "#]],
    );
}

#[test]
fn nested_scopes() {
    check_stmt_kinds(
        "while (true) { { break; } }",
        &expect![[r#"
            WhileLoop [0-27]:
                condition: Expr [7-11]:
                    ty: const bool
                    kind: Lit: Bool(true)
                body: Stmt [13-27]:
                    annotations: <empty>
                    kind: Block [13-27]:
                        Stmt [15-25]:
                            annotations: <empty>
                            kind: Block [15-25]:
                                Stmt [17-23]:
                                    annotations: <empty>
                                    kind: BreakStmt [17-23]:
        "#]],
    );
}

#[test]
fn break_in_non_loop_scope_fails() {
    check_stmt_kinds(
        "break;",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-6]:
                        annotations: <empty>
                        kind: Err

            [Qasm.Lowerer.InvalidScope

              x break can only appear in loop scopes
               ,-[test:1:1]
             1 | break;
               : ^^^^^^
               `----
            ]"#]],
    );
}

#[test]
fn intermediate_def_scope_fails() {
    check_stmt_kinds(
        "
        while (true) {
            def f() { break; }
        }
        ",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-64]:
                        annotations: <empty>
                        kind: WhileLoop [9-64]:
                            condition: Expr [16-20]:
                                ty: const bool
                                kind: Lit: Bool(true)
                            body: Stmt [22-64]:
                                annotations: <empty>
                                kind: Block [22-64]:
                                    Stmt [36-54]:
                                        annotations: <empty>
                                        kind: DefStmt [36-54]:
                                            symbol_id: 8
                                            has_qubit_params: false
                                            parameters: <empty>
                                            return_type_span: [0-0]
                                            return_ty_exprs: <empty>
                                            body: Block [44-54]:
                                                Stmt [46-52]:
                                                    annotations: <empty>
                                                    kind: Err

            [Qasm.Lowerer.DefDeclarationInNonGlobalScope

              x def declarations must be done in global scope
               ,-[test:3:13]
             2 |         while (true) {
             3 |             def f() { break; }
               :             ^^^^^^^^^^^^^^^^^^
             4 |         }
               `----
            , Qasm.Lowerer.InvalidScope

              x break can only appear in loop scopes
               ,-[test:3:23]
             2 |         while (true) {
             3 |             def f() { break; }
               :                       ^^^^^^
             4 |         }
               `----
            ]"#]],
    );
}
