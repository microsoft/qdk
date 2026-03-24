// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn arctan() {
    let source = "
        arctan(0.);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ExprStmt [9-20]:
                expr: Expr [9-19]:
                    ty: const float
                    const_value: Float(0.0)
                    kind: BuiltinFunctionCall [9-19]:
                        fn_name_span: [9-15]
                        name: arctan
                        function_ty: def (const float) -> const float
                        args:
                            Expr [16-18]:
                                ty: const float
                                const_value: Float(0.0)
                                kind: Lit: Float(0.0)
        "#]],
    );
}
