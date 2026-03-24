// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn ceiling_positive() {
    let source = "
        ceiling(0.5);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-22]:
            expr: Expr [9-21]:
                ty: const float
                const_value: Float(1.0)
                kind: BuiltinFunctionCall [9-21]:
                    fn_name_span: [9-16]
                    name: ceiling
                    function_ty: def (const float) -> const float
                    args:
                        Expr [17-20]:
                            ty: const float
                            const_value: Float(0.5)
                            kind: Lit: Float(0.5)
    "#]],
    );
}

#[test]
fn ceiling_positive_edge_case() {
    let source = "
        ceiling(1.0);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-22]:
            expr: Expr [9-21]:
                ty: const float
                const_value: Float(1.0)
                kind: BuiltinFunctionCall [9-21]:
                    fn_name_span: [9-16]
                    name: ceiling
                    function_ty: def (const float) -> const float
                    args:
                        Expr [17-20]:
                            ty: const float
                            const_value: Float(1.0)
                            kind: Lit: Float(1.0)
    "#]],
    );
}

#[test]
fn ceiling_negative() {
    let source = "
        ceiling(-0.5);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-23]:
            expr: Expr [9-22]:
                ty: const float
                const_value: Float(-0.0)
                kind: BuiltinFunctionCall [9-22]:
                    fn_name_span: [9-16]
                    name: ceiling
                    function_ty: def (const float) -> const float
                    args:
                        Expr [18-21]:
                            ty: const float
                            const_value: Float(-0.5)
                            kind: UnaryOpExpr [18-21]:
                                op: Neg
                                expr: Expr [18-21]:
                                    ty: const float
                                    kind: Lit: Float(0.5)
    "#]],
    );
}

#[test]
fn ceiling_negative_edge_case() {
    let source = "
        ceiling(-1.0);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-23]:
            expr: Expr [9-22]:
                ty: const float
                const_value: Float(-1.0)
                kind: BuiltinFunctionCall [9-22]:
                    fn_name_span: [9-16]
                    name: ceiling
                    function_ty: def (const float) -> const float
                    args:
                        Expr [18-21]:
                            ty: const float
                            const_value: Float(-1.0)
                            kind: UnaryOpExpr [18-21]:
                                op: Neg
                                expr: Expr [18-21]:
                                    ty: const float
                                    kind: Lit: Float(1.0)
    "#]],
    );
}
