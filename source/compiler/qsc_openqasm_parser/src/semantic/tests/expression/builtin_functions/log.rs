// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn log() {
    let source = "
        log(2.);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-17]:
            expr: Expr [9-16]:
                ty: const float
                const_value: Float(0.6931471805599453)
                kind: BuiltinFunctionCall [9-16]:
                    fn_name_span: [9-12]
                    name: log
                    function_ty: def (const float) -> const float
                    args:
                        Expr [13-15]:
                            ty: const float
                            const_value: Float(2.0)
                            kind: Lit: Float(2.0)
    "#]],
    );
}
