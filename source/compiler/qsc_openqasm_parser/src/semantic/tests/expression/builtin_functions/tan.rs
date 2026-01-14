// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn tan_float() {
    let source = "
        tan(pi / 4.);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-22]:
            expr: Expr [9-21]:
                ty: const float
                const_value: Float(0.9999999999999999)
                kind: BuiltinFunctionCall [9-21]:
                    fn_name_span: [9-12]
                    name: tan
                    function_ty: def (const float) -> const float
                    args:
                        Expr [13-20]:
                            ty: const float
                            const_value: Float(0.7853981633974483)
                            kind: BinaryOpExpr:
                                op: Div
                                lhs: Expr [13-15]:
                                    ty: const float
                                    kind: SymbolId(2)
                                rhs: Expr [18-20]:
                                    ty: const float
                                    kind: Lit: Float(4.0)
    "#]],
    );
}

#[test]
fn tan_angle() {
    let source = "
        const angle a = pi / 4.;
        tan(a);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-33]:
                symbol_id: 8
                ty_span: [15-20]
                ty_exprs: <empty>
                init_expr: Expr [25-32]:
                    ty: const angle
                    const_value: Angle(0.7853981633974483)
                    kind: Cast [25-32]:
                        ty: const angle
                        ty_exprs: <empty>
                        expr: Expr [25-32]:
                            ty: const float
                            kind: BinaryOpExpr:
                                op: Div
                                lhs: Expr [25-27]:
                                    ty: const float
                                    kind: SymbolId(2)
                                rhs: Expr [30-32]:
                                    ty: const float
                                    kind: Lit: Float(4.0)
                        kind: Implicit
            ExprStmt [42-49]:
                expr: Expr [42-48]:
                    ty: const float
                    const_value: Float(0.9999999999999999)
                    kind: BuiltinFunctionCall [42-48]:
                        fn_name_span: [42-45]
                        name: tan
                        function_ty: def (const angle) -> const float
                        args:
                            Expr [46-47]:
                                ty: const angle
                                const_value: Angle(0.7853981633974483)
                                kind: SymbolId(8)
        "#]],
    );
}
