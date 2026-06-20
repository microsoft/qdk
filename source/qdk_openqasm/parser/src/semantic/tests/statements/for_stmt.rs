// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{semantic::tests::check_stmt_kinds, tests::check_qasm};
use expect_test::expect;

#[test]
fn shadowing_loop_variable_in_single_stmt_body() {
    check_stmt_kinds(
        "
    for int x in {}
        int x = 2;
    ",
        &expect![[r#"
            ForStmt [5-39]:
                loop_variable: 8
                ty_exprs: <empty>
                iterable: Set [18-20]:
                    values: <empty>
                body: Stmt [29-39]:
                    annotations: <empty>
                    kind: Block [29-39]:
                        Stmt [29-39]:
                            annotations: <empty>
                            kind: ClassicalDeclarationStmt [29-39]:
                                symbol_id: 9
                                ty_span: [29-32]
                                ty_exprs: <empty>
                                init_expr: Expr [37-38]:
                                    ty: int
                                    kind: Lit: Int(2)
        "#]],
    );
}

#[test]
fn shadowing_loop_variable_in_block_body_succeeds() {
    check_stmt_kinds(
        "
    for int x in {} {
        int x = 2;
    }
    ",
        &expect![[r#"
            ForStmt [5-47]:
                loop_variable: 8
                ty_exprs: <empty>
                iterable: Set [18-20]:
                    values: <empty>
                body: Stmt [21-47]:
                    annotations: <empty>
                    kind: Block [21-47]:
                        Stmt [31-41]:
                            annotations: <empty>
                            kind: ClassicalDeclarationStmt [31-41]:
                                symbol_id: 9
                                ty_span: [31-34]
                                ty_exprs: <empty>
                                init_expr: Expr [39-40]:
                                    ty: int
                                    kind: Lit: Int(2)
        "#]],
    );
}

#[test]
fn loop_creates_its_own_scope() {
    check_stmt_kinds(
        "
    int a = 0;
    for int x in {}
        // shadowing works because this
        // declaration is in a different
        // scope from `int a = 0;` scope.
        int a = 1;
    ",
        &expect![[r#"
            ClassicalDeclarationStmt [5-15]:
                symbol_id: 8
                ty_span: [5-8]
                ty_exprs: <empty>
                init_expr: Expr [13-14]:
                    ty: int
                    kind: Lit: Int(0)
            ForStmt [20-177]:
                loop_variable: 9
                ty_exprs: <empty>
                iterable: Set [33-35]:
                    values: <empty>
                body: Stmt [167-177]:
                    annotations: <empty>
                    kind: Block [167-177]:
                        Stmt [167-177]:
                            annotations: <empty>
                            kind: ClassicalDeclarationStmt [167-177]:
                                symbol_id: 10
                                ty_span: [167-170]
                                ty_exprs: <empty>
                                init_expr: Expr [175-176]:
                                    ty: int
                                    kind: Lit: Int(1)
        "#]],
    );
}

#[test]
fn omitted_start_in_for_range_fails() {
    let source = "
        for int i in [:5] {}
    ";

    check_qasm(
        source,
        &expect![[r#"
            Qasm.Lowerer.RangeExpressionsMustHaveStart

              x range expressions must have a start when used in for loops
               ,-[Test.qasm:2:22]
             1 | 
             2 |         for int i in [:5] {}
               :                      ^^^^
             3 |     
               `----
        "#]],
    );
}

#[test]
fn omitted_end_in_for_range_fails() {
    let source = "
        for int i in [1:] {}
    ";

    check_qasm(
        source,
        &expect![[r#"
            Qasm.Lowerer.RangeExpressionsMustHaveStop

              x range expressions must have a stop when used in for loops
               ,-[Test.qasm:2:22]
             1 | 
             2 |         for int i in [1:] {}
               :                      ^^^^
             3 |     
               `----
        "#]],
    );
}
