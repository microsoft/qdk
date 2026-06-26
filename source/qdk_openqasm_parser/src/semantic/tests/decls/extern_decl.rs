// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kind;
use expect_test::expect;

#[test]
fn void_no_args() {
    check_stmt_kind(
        "extern f();",
        &expect![[r#"
            ExternDecl [0-11]:
                symbol_id: 8
                ty_exprs: <empty>
                return_ty_exprs: <empty>"#]],
    );
}

#[test]
fn void_one_arg() {
    check_stmt_kind(
        "extern f(int);",
        &expect![[r#"
            ExternDecl [0-14]:
                symbol_id: 8
                ty_exprs: <empty>
                return_ty_exprs: <empty>"#]],
    );
}

#[test]
fn void_multiple_args() {
    check_stmt_kind(
        "extern f(uint, int, float, bit, bool);",
        &expect![[r#"
            ExternDecl [0-38]:
                symbol_id: 8
                ty_exprs: <empty>
                return_ty_exprs: <empty>"#]],
    );
}

#[test]
fn return_type() {
    check_stmt_kind(
        "extern f() -> int;",
        &expect![[r#"
            ExternDecl [0-18]:
                symbol_id: 8
                ty_exprs: <empty>
                return_ty_exprs: <empty>"#]],
    );
}

#[test]
fn return_type_can_be_duration() {
    check_stmt_kind(
        "extern f() -> duration;",
        &expect![[r#"
            ExternDecl [0-23]:
                symbol_id: 8
                ty_exprs: <empty>
                return_ty_exprs: <empty>"#]],
    );
}

#[test]
fn return_type_cannot_be_stretch() {
    check_stmt_kind(
        "extern f() -> stretch;",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-22]:
                        annotations: <empty>
                        kind: ExternDecl [0-22]:
                            symbol_id: 8
                            ty_exprs: <empty>
                            return_ty_exprs: <empty>

            [Qasm.Lowerer.ExternDeclarationCannotReturnStretch

              x extern declarations cannot return stretches
               ,-[test:1:15]
             1 | extern f() -> stretch;
               :               ^^^^^^^
               `----
            ]"#]],
    );
}

#[test]
fn not_allowed_in_non_global_scope() {
    check_stmt_kind(
        "{ extern f(); }",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-15]:
                        annotations: <empty>
                        kind: Block [0-15]:
                            Stmt [2-13]:
                                annotations: <empty>
                                kind: ExternDecl [2-13]:
                                    symbol_id: 8
                                    ty_exprs: <empty>
                                    return_ty_exprs: <empty>

            [Qasm.Lowerer.ExternDeclarationInNonGlobalScope

              x extern declarations must be done in global scope
               ,-[test:1:3]
             1 | { extern f(); }
               :   ^^^^^^^^^^^
               `----
            ]"#]],
    );
}
