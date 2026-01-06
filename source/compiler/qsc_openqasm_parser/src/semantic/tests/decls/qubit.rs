// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_stmt_kind;

#[test]
fn with_no_init_expr() {
    check_stmt_kind(
        "qubit a;",
        &expect![[r#"
            QubitDeclaration [0-8]:
                symbol_id: 8"#]],
    );
}

#[test]
fn with_no_init_expr_in_non_global_scope() {
    check_stmt_kind(
        "{qubit a;}",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-10]:
                        annotations: <empty>
                        kind: Block [0-10]:
                            Stmt [1-9]:
                                annotations: <empty>
                                kind: QubitDeclaration [1-9]:
                                    symbol_id: 8

            [Qasm.Lowerer.QubitDeclarationInNonGlobalScope

              x qubit declarations must be done in global scope
               ,-[test:1:2]
             1 | {qubit a;}
               :  ^^^^^^^^
               `----
            ]"#]],
    );
}

#[test]
fn array_with_no_init_expr() {
    check_stmt_kind(
        "qubit[3] a;",
        &expect![[r#"
            QubitArrayDeclaration [0-11]:
                symbol_id: 8
                size: Expr [6-7]:
                    ty: const uint
                    const_value: Int(3)
                    kind: Lit: Int(3)
                size_span: [6-7]"#]],
    );
}

#[test]
fn array_with_no_init_expr_in_non_global_scope() {
    check_stmt_kind(
        "{qubit[3] a;}",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-13]:
                        annotations: <empty>
                        kind: Block [0-13]:
                            Stmt [1-12]:
                                annotations: <empty>
                                kind: QubitArrayDeclaration [1-12]:
                                    symbol_id: 8
                                    size: Expr [7-8]:
                                        ty: const uint
                                        const_value: Int(3)
                                        kind: Lit: Int(3)
                                    size_span: [7-8]

            [Qasm.Lowerer.QubitDeclarationInNonGlobalScope

              x qubit declarations must be done in global scope
               ,-[test:1:2]
             1 | {qubit[3] a;}
               :  ^^^^^^^^^^^
               `----
            ]"#]],
    );
}
