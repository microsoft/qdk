// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_stmt_kind;

#[test]
fn with_no_init_expr() {
    check_stmt_kind(
        "qreg a;",
        &expect![[r#"
            QubitDeclaration [0-7]:
                symbol_id: 8"#]],
    );
}

#[test]
fn with_no_init_expr_in_non_global_scope() {
    check_stmt_kind(
        "{qreg a;}",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-9]:
                        annotations: <empty>
                        kind: Block [0-9]:
                            Stmt [1-8]:
                                annotations: <empty>
                                kind: QubitDeclaration [1-8]:
                                    symbol_id: 8

            [Qasm.Lowerer.QubitDeclarationInNonGlobalScope

              x qubit declarations must be done in global scope
               ,-[test:1:2]
             1 | {qreg a;}
               :  ^^^^^^^
               `----
            ]"#]],
    );
}

#[test]
fn array_with_no_init_expr() {
    check_stmt_kind(
        "qreg a[3];",
        &expect![[r#"
            QubitArrayDeclaration [0-10]:
                symbol_id: 8
                size: Expr [7-8]:
                    ty: const uint
                    const_value: Int(3)
                    kind: Lit: Int(3)
                size_span: [7-8]"#]],
    );
}

#[test]
fn array_with_no_init_expr_in_non_global_scope() {
    check_stmt_kind(
        "{qreg a[3];}",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-12]:
                        annotations: <empty>
                        kind: Block [0-12]:
                            Stmt [1-11]:
                                annotations: <empty>
                                kind: QubitArrayDeclaration [1-11]:
                                    symbol_id: 8
                                    size: Expr [8-9]:
                                        ty: const uint
                                        const_value: Int(3)
                                        kind: Lit: Int(3)
                                    size_span: [8-9]

            [Qasm.Lowerer.QubitDeclarationInNonGlobalScope

              x qubit declarations must be done in global scope
               ,-[test:1:2]
             1 | {qreg a[3];}
               :  ^^^^^^^^^^
               `----
            ]"#]],
    );
}
