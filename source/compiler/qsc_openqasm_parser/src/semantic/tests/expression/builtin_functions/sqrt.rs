// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn sqrt_float() {
    let source = "
        sqrt(4.);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ExprStmt [9-18]:
                expr: Expr [9-17]:
                    ty: const float
                    const_value: Float(2.0)
                    kind: BuiltinFunctionCall [9-17]:
                        fn_name_span: [9-13]
                        name: sqrt
                        function_ty: def (const float) -> const float
                        args:
                            Expr [14-16]:
                                ty: const float
                                const_value: Float(4.0)
                                kind: Lit: Float(4.0)
        "#]],
    );
}

#[test]
fn sqrt_float_domain_error() {
    let source = "
        sqrt(-4.);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: Err

            [Qasm.Lowerer.DomainError

              x cannot compute square root of negative floats
               ,-[test:2:9]
             1 | 
             2 |         sqrt(-4.);
               :         ^^^^^^^^^
             3 |     
               `----
            ]"#]],
    );
}

#[test]
fn sqrt_complex() {
    let source = "
        sqrt(7 + 24 im);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ExprStmt [9-25]:
                expr: Expr [9-24]:
                    ty: const complex[float]
                    const_value: Complex(4.0, 3.0)
                    kind: BuiltinFunctionCall [9-24]:
                        fn_name_span: [9-13]
                        name: sqrt
                        function_ty: def (const complex[float]) -> const complex[float]
                        args:
                            Expr [14-23]:
                                ty: const complex[float]
                                const_value: Complex(7.0, 24.0)
                                kind: BinaryOpExpr:
                                    op: Add
                                    lhs: Expr [14-15]:
                                        ty: const complex[float]
                                        kind: Lit: Complex(7.0, 0.0)
                                    rhs: Expr [18-23]:
                                        ty: const complex[float]
                                        kind: Lit: Complex(0.0, 24.0)
        "#]],
    );
}

#[test]
fn casting_large_int_to_float_errors() {
    let source = "sqrt(888888888888888888);";
    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-25]:
                        annotations: <empty>
                        kind: Err

            [Qasm.Lowerer.InvalidCastValueRange

              x assigning const int values to const float must be in a range that can be
              | converted to const float
               ,-[test:1:6]
             1 | sqrt(888888888888888888);
               :      ^^^^^^^^^^^^^^^^^^
               `----
            ]"#]],
    );
}
