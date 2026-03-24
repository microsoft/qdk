// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn not_supported_before_version_3_1() {
    check_stmt_kinds(
        r#"
    OPENQASM 3.0;
    switch (1) { case 1 {} }
    "#,
        &expect![[r#"
            Program:
                version: 3.0
                pragmas: <empty>
                statements:
                    Stmt [23-47]:
                        annotations: <empty>
                        kind: SwitchStmt [23-47]:
                            target: Expr [31-32]:
                                ty: const int
                                kind: Lit: Int(1)
                            cases:
                                SwitchCase [36-45]:
                                    labels:
                                        Expr [41-42]:
                                            ty: const int
                                            kind: Lit: Int(1)
                                    block: Block [43-45]: <empty>
                            default_case: <none>

            [Qasm.Lowerer.NotSupportedInThisVersion

              x switch statements were introduced in version 3.1
               ,-[test:3:5]
             2 |     OPENQASM 3.0;
             3 |     switch (1) { case 1 {} }
               :     ^^^^^^^^^^^^^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn cases_introduce_their_own_scope() {
    check_stmt_kinds(
        r#"
    int a = 3;
    switch (1) {
        case 1 { int a = 1; }
        case 2, 3 { int a = 2; }
    }
    "#,
        &expect![[r#"
            ClassicalDeclarationStmt [5-15]:
                symbol_id: 8
                ty_span: [5-8]
                ty_exprs: <empty>
                init_expr: Expr [13-14]:
                    ty: int
                    kind: Lit: Int(3)
            SwitchStmt [20-101]:
                target: Expr [28-29]:
                    ty: const int
                    kind: Lit: Int(1)
                cases:
                    SwitchCase [41-62]:
                        labels:
                            Expr [46-47]:
                                ty: const int
                                kind: Lit: Int(1)
                        block: Block [48-62]:
                            Stmt [50-60]:
                                annotations: <empty>
                                kind: ClassicalDeclarationStmt [50-60]:
                                    symbol_id: 9
                                    ty_span: [50-53]
                                    ty_exprs: <empty>
                                    init_expr: Expr [58-59]:
                                        ty: int
                                        kind: Lit: Int(1)
                    SwitchCase [71-95]:
                        labels:
                            Expr [76-77]:
                                ty: const int
                                kind: Lit: Int(2)
                            Expr [79-80]:
                                ty: const int
                                kind: Lit: Int(3)
                        block: Block [81-95]:
                            Stmt [83-93]:
                                annotations: <empty>
                                kind: ClassicalDeclarationStmt [83-93]:
                                    symbol_id: 10
                                    ty_span: [83-86]
                                    ty_exprs: <empty>
                                    init_expr: Expr [91-92]:
                                        ty: int
                                        kind: Lit: Int(2)
                default_case: <none>
        "#]],
    );
}

#[test]
fn target_cast() {
    check_stmt_kinds(
        "switch (true) { case false {} }",
        &expect![[r#"
            SwitchStmt [0-31]:
                target: Expr [8-12]:
                    ty: const int
                    kind: Cast [8-12]:
                        ty: const int
                        ty_exprs: <empty>
                        expr: Expr [8-12]:
                            ty: const bool
                            kind: Lit: Bool(true)
                        kind: Implicit
                cases:
                    SwitchCase [16-29]:
                        labels:
                            Expr [21-26]:
                                ty: const int
                                kind: Cast [21-26]:
                                    ty: const int
                                    ty_exprs: <empty>
                                    expr: Expr [21-26]:
                                        ty: const bool
                                        kind: Lit: Bool(false)
                                    kind: Implicit
                        block: Block [27-29]: <empty>
                default_case: <none>
        "#]],
    );
}
