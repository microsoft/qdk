// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn exp_float() {
    let source = "
        exp(2.);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-17]:
            expr: Expr [9-16]:
                ty: const float
                const_value: Float(7.38905609893065)
                kind: BuiltinFunctionCall [9-16]:
                    fn_name_span: [9-12]
                    name: exp
                    function_ty: def (const float) -> const float
                    args:
                        Expr [13-15]:
                            ty: const float
                            const_value: Float(2.0)
                            kind: Lit: Float(2.0)
    "#]],
    );
}

#[test]
fn exp_complex() {
    let source = "
        const complex a = 2 + 3 im;
        exp(a);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-36]:
                symbol_id: 8
                ty_span: [15-22]
                ty_exprs: <empty>
                init_expr: Expr [27-35]:
                    ty: const complex[float]
                    const_value: Complex(2.0, 3.0)
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [27-28]:
                            ty: const complex[float]
                            kind: Lit: Complex(2.0, 0.0)
                        rhs: Expr [31-35]:
                            ty: const complex[float]
                            kind: Lit: Complex(0.0, 3.0)
            ExprStmt [45-52]:
                expr: Expr [45-51]:
                    ty: const complex[float]
                    const_value: Complex(-7.315110094901102, 1.0427436562359043)
                    kind: BuiltinFunctionCall [45-51]:
                        fn_name_span: [45-48]
                        name: exp
                        function_ty: def (const complex[float]) -> const complex[float]
                        args:
                            Expr [49-50]:
                                ty: const complex[float]
                                const_value: Complex(2.0, 3.0)
                                kind: SymbolId(8)
        "#]],
    );
}
