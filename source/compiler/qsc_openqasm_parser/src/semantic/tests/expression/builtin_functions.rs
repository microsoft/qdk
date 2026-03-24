// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod arccos;
mod arcsin;
mod arctan;
mod ceiling;
mod cos;
mod exp;
mod floor;
mod log;
mod mod_;
mod popcount;
mod pow;
mod rotl;
mod rotr;
mod sin;
mod sizeof;
mod sqrt;
mod tan;

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn builtin_call_with_invalid_input_types_fails() {
    let source = "
        mod(9, true);
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

            [Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `mod` for inputs: (const int, const bool)
              | Overloads available are:
              |     def mod(const int, const int) -> const int
              |     def mod(const float, const float) -> const float
               ,-[test:2:9]
             1 | 
             2 |         mod(9, true);
               :         ^^^^^^^^^^^^
             3 |     
               `----
            ]"#]],
    );
}

#[test]
fn builtin_call_with_zero_arguments_fails() {
    let source = "
        mod();
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-15]:
                        annotations: <empty>
                        kind: Err

            [Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `mod` for inputs: ()
              | Overloads available are:
              |     def mod(const int, const int) -> const int
              |     def mod(const float, const float) -> const float
               ,-[test:2:9]
             1 | 
             2 |         mod();
               :         ^^^^^
             3 |     
               `----
            ]"#]],
    );
}

#[test]
fn builtin_call_with_lower_arity_fails() {
    let source = "
        mod(9);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-16]:
                        annotations: <empty>
                        kind: Err

            [Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `mod` for inputs: (const int)
              | Overloads available are:
              |     def mod(const int, const int) -> const int
              |     def mod(const float, const float) -> const float
               ,-[test:2:9]
             1 | 
             2 |         mod(9);
               :         ^^^^^^
             3 |     
               `----
            ]"#]],
    );
}

#[test]
fn builtin_call_with_higher_arity_fails() {
    let source = "
        mod(9, 7, 2);
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

            [Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `mod` for inputs: (const int, const int,
              | const int)
              | Overloads available are:
              |     def mod(const int, const int) -> const int
              |     def mod(const float, const float) -> const float
               ,-[test:2:9]
             1 | 
             2 |         mod(9, 7, 2);
               :         ^^^^^^^^^^^^
             3 |     
               `----
            ]"#]],
    );
}

#[test]
fn builtin_call_with_const_expr_succeeds() {
    let source = "
        const int a = 9;
        const int b = 7;
        mod(a + b, b);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-25]:
                symbol_id: 8
                ty_span: [15-18]
                ty_exprs: <empty>
                init_expr: Expr [23-24]:
                    ty: const int
                    const_value: Int(9)
                    kind: Lit: Int(9)
            ClassicalDeclarationStmt [34-50]:
                symbol_id: 9
                ty_span: [40-43]
                ty_exprs: <empty>
                init_expr: Expr [48-49]:
                    ty: const int
                    const_value: Int(7)
                    kind: Lit: Int(7)
            ExprStmt [59-73]:
                expr: Expr [59-72]:
                    ty: const int
                    const_value: Int(2)
                    kind: BuiltinFunctionCall [59-72]:
                        fn_name_span: [59-62]
                        name: mod
                        function_ty: def (const int, const int) -> const int
                        args:
                            Expr [63-68]:
                                ty: const int
                                const_value: Int(16)
                                kind: BinaryOpExpr:
                                    op: Add
                                    lhs: Expr [63-64]:
                                        ty: const int
                                        kind: SymbolId(8)
                                    rhs: Expr [67-68]:
                                        ty: const int
                                        kind: SymbolId(9)
                            Expr [70-71]:
                                ty: const int
                                const_value: Int(7)
                                kind: SymbolId(9)
        "#]],
    );
}

#[test]
fn nested_builtin_call_succeeds() {
    let source = "
        const int a = 9;
        const int b = 7;
        mod(a, mod(a, b));
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-25]:
                symbol_id: 8
                ty_span: [15-18]
                ty_exprs: <empty>
                init_expr: Expr [23-24]:
                    ty: const int
                    const_value: Int(9)
                    kind: Lit: Int(9)
            ClassicalDeclarationStmt [34-50]:
                symbol_id: 9
                ty_span: [40-43]
                ty_exprs: <empty>
                init_expr: Expr [48-49]:
                    ty: const int
                    const_value: Int(7)
                    kind: Lit: Int(7)
            ExprStmt [59-77]:
                expr: Expr [59-76]:
                    ty: const int
                    const_value: Int(1)
                    kind: BuiltinFunctionCall [59-76]:
                        fn_name_span: [59-62]
                        name: mod
                        function_ty: def (const int, const int) -> const int
                        args:
                            Expr [63-64]:
                                ty: const int
                                const_value: Int(9)
                                kind: SymbolId(8)
                            Expr [66-75]:
                                ty: const int
                                const_value: Int(2)
                                kind: BuiltinFunctionCall [66-75]:
                                    fn_name_span: [66-69]
                                    name: mod
                                    function_ty: def (const int, const int) -> const int
                                    args:
                                        Expr [70-71]:
                                            ty: const int
                                            const_value: Int(9)
                                            kind: SymbolId(8)
                                        Expr [73-74]:
                                            ty: const int
                                            const_value: Int(7)
                                            kind: SymbolId(9)
        "#]],
    );
}
