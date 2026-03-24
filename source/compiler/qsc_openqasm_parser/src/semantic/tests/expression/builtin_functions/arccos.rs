// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn arccos() {
    let source = "
        arccos(0.);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ExprStmt [9-20]:
                expr: Expr [9-19]:
                    ty: const float
                    const_value: Float(1.5707963267948966)
                    kind: BuiltinFunctionCall [9-19]:
                        fn_name_span: [9-15]
                        name: arccos
                        function_ty: def (const float) -> const float
                        args:
                            Expr [16-18]:
                                ty: const float
                                const_value: Float(0.0)
                                kind: Lit: Float(0.0)
        "#]],
    );
}

#[test]
fn arccos_negative_domain_edge_case() {
    let source = "
        arccos(-1.);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ExprStmt [9-21]:
            expr: Expr [9-20]:
                ty: const float
                const_value: Float(3.141592653589793)
                kind: BuiltinFunctionCall [9-20]:
                    fn_name_span: [9-15]
                    name: arccos
                    function_ty: def (const float) -> const float
                    args:
                        Expr [17-19]:
                            ty: const float
                            const_value: Float(-1.0)
                            kind: UnaryOpExpr [17-19]:
                                op: Neg
                                expr: Expr [17-19]:
                                    ty: const float
                                    kind: Lit: Float(1.0)
    "#]],
    );
}

#[test]
fn arccos_positive_domain_edge_case() {
    let source = "
        arccos(1.);
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
                    name: arccos
                    function_ty: def (const float) -> const float
                    args:
                        Expr [16-18]:
                            ty: const float
                            const_value: Float(1.0)
                            kind: Lit: Float(1.0)
    "#]],
    );
}

#[test]
fn arccos_negative_domain_error() {
    let source = "
        arccos(-1.01);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-23]:
                        annotations: <empty>
                        kind: Err

            [Qasm.Lowerer.DomainError

              x arccos input should be in the range [-1.0, 1.0]
               ,-[test:2:9]
             1 | 
             2 |         arccos(-1.01);
               :         ^^^^^^^^^^^^^
             3 |     
               `----
            ]"#]],
    );
}

#[test]
fn arccos_positive_domain_error() {
    let source = "
        arccos(1.01);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-22]:
                        annotations: <empty>
                        kind: Err

            [Qasm.Lowerer.DomainError

              x arccos input should be in the range [-1.0, 1.0]
               ,-[test:2:9]
             1 | 
             2 |         arccos(1.01);
               :         ^^^^^^^^^^^^
             3 |     
               `----
            ]"#]],
    );
}
