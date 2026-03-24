// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn mod_int() {
    let source = "
        mod(9, 7);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ExprStmt [9-19]:
                expr: Expr [9-18]:
                    ty: const int
                    const_value: Int(2)
                    kind: BuiltinFunctionCall [9-18]:
                        fn_name_span: [9-12]
                        name: mod
                        function_ty: def (const int, const int) -> const int
                        args:
                            Expr [13-14]:
                                ty: const int
                                const_value: Int(9)
                                kind: Lit: Int(9)
                            Expr [16-17]:
                                ty: const int
                                const_value: Int(7)
                                kind: Lit: Int(7)
        "#]],
    );
}

#[test]
fn mod_int_divide_by_zero_error() {
    let source = "
        mod(9, 0);
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

            [Qasm.Lowerer.DivisionByZero

              x division by zero error during const evaluation
               ,-[test:2:9]
             1 | 
             2 |         mod(9, 0);
               :         ^^^^^^^^^
             3 |     
               `----
            ]"#]],
    );
}

#[test]
fn mod_float() {
    let source = "
        mod(9, 7.);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ExprStmt [9-20]:
                expr: Expr [9-19]:
                    ty: const float
                    const_value: Float(2.0)
                    kind: BuiltinFunctionCall [9-19]:
                        fn_name_span: [9-12]
                        name: mod
                        function_ty: def (const float, const float) -> const float
                        args:
                            Expr [13-14]:
                                ty: const int
                                const_value: Int(9)
                                kind: Lit: Int(9)
                            Expr [16-18]:
                                ty: const float
                                const_value: Float(7.0)
                                kind: Lit: Float(7.0)
        "#]],
    );
}

#[test]
fn mod_float_divide_by_zero_error() {
    let source = "
        mod(9., 0.);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-21]:
                        annotations: <empty>
                        kind: Err

            [Qasm.Lowerer.DivisionByZero

              x division by zero error during const evaluation
               ,-[test:2:9]
             1 | 
             2 |         mod(9., 0.);
               :         ^^^^^^^^^^^
             3 |     
               `----
            ]"#]],
    );
}
