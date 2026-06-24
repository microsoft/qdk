// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn floor_positive() {
    let source = "
        floor(0.5);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-20]:
            expr: Expr [9-19]:
                ty: const float
                const_value: Float(0.0)
                kind: BuiltinFunctionCall [9-19]:
                    fn_name_span: [9-14]
                    name: floor
                    function_ty: def (const float) -> const float
                    args:
                        Expr [15-18]:
                            ty: const float
                            const_value: Float(0.5)
                            kind: Lit: Float(0.5)
    "#]],
    );
}

#[test]
fn floor_positive_edge_case() {
    let source = "
        floor(1.0);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-20]:
            expr: Expr [9-19]:
                ty: const float
                const_value: Float(1.0)
                kind: BuiltinFunctionCall [9-19]:
                    fn_name_span: [9-14]
                    name: floor
                    function_ty: def (const float) -> const float
                    args:
                        Expr [15-18]:
                            ty: const float
                            const_value: Float(1.0)
                            kind: Lit: Float(1.0)
    "#]],
    );
}

#[test]
fn floor_negative() {
    let source = "
        floor(-0.5);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-21]:
            expr: Expr [9-20]:
                ty: const float
                const_value: Float(-1.0)
                kind: BuiltinFunctionCall [9-20]:
                    fn_name_span: [9-14]
                    name: floor
                    function_ty: def (const float) -> const float
                    args:
                        Expr [16-19]:
                            ty: const float
                            const_value: Float(-0.5)
                            kind: UnaryOpExpr [16-19]:
                                op: Neg
                                expr: Expr [16-19]:
                                    ty: const float
                                    kind: Lit: Float(0.5)
    "#]],
    );
}

#[test]
fn floor_negative_edge_case() {
    let source = "
        floor(-1.0);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-21]:
            expr: Expr [9-20]:
                ty: const float
                const_value: Float(-1.0)
                kind: BuiltinFunctionCall [9-20]:
                    fn_name_span: [9-14]
                    name: floor
                    function_ty: def (const float) -> const float
                    args:
                        Expr [16-19]:
                            ty: const float
                            const_value: Float(-1.0)
                            kind: UnaryOpExpr [16-19]:
                                op: Neg
                                expr: Expr [16-19]:
                                    ty: const float
                                    kind: Lit: Float(1.0)
    "#]],
    );
}
