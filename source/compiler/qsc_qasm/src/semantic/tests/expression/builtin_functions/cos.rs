// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn cos_float() {
    let source = "
        cos(pi);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-17]:
            expr: Expr [9-16]:
                ty: const float
                const_value: Float(-1.0)
                kind: BuiltinFunctionCall [9-16]:
                    fn_name_span: [9-12]
                    name: cos
                    function_ty: def (const float) -> const float
                    args:
                        Expr [13-15]:
                            ty: const float
                            const_value: Float(3.141592653589793)
                            kind: SymbolId(2)
    "#]],
    );
}

#[test]
fn cos_angle() {
    let source = "
        const angle a = pi;
        cos(a);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-28]:
                symbol_id: 8
                ty_span: [15-20]
                init_expr: Expr [25-27]:
                    ty: const angle
                    const_value: Angle(3.141592653589793)
                    kind: Cast [25-27]:
                        ty: const angle
                        expr: Expr [25-27]:
                            ty: const float
                            kind: SymbolId(2)
                        kind: Implicit
            ExprStmt [37-44]:
                expr: Expr [37-43]:
                    ty: const float
                    const_value: Float(-1.0)
                    kind: BuiltinFunctionCall [37-43]:
                        fn_name_span: [37-40]
                        name: cos
                        function_ty: def (const angle) -> const float
                        args:
                            Expr [41-42]:
                                ty: const angle
                                const_value: Angle(3.141592653589793)
                                kind: SymbolId(8)
        "#]],
    );
}
