// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn real_and_imag_preserve_machine_component_width() {
    let source = "
        const complex value = 1.25 + 2.5 im;
        real(value);
        imag(value);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ClassicalDeclarationStmt [9-45]:
            symbol_id: 8
            ty_span: [15-22]
            ty_exprs: <empty>
            init_expr: Expr [31-44]:
                ty: const complex[float]
                const_value: Complex(1.25, 2.5)
                kind: BinaryOpExpr:
                    op: Add
                    lhs: Expr [31-35]:
                        ty: const complex[float]
                        kind: Lit: Complex(1.25, 0.0)
                    rhs: Expr [38-44]:
                        ty: const complex[float]
                        kind: Lit: Complex(0.0, 2.5)
        ExprStmt [54-66]:
            expr: Expr [54-65]:
                ty: const float
                const_value: Float(1.25)
                kind: BuiltinFunctionCall [54-65]:
                    fn_name_span: [54-58]
                    name: real
                    function_ty: def (const complex[float]) -> const float
                    args:
                        Expr [59-64]:
                            ty: const complex[float]
                            const_value: Complex(1.25, 2.5)
                            kind: SymbolId(8)
        ExprStmt [75-87]:
            expr: Expr [75-86]:
                ty: const float
                const_value: Float(2.5)
                kind: BuiltinFunctionCall [75-86]:
                    fn_name_span: [75-79]
                    name: imag
                    function_ty: def (const complex[float]) -> const float
                    args:
                        Expr [80-85]:
                            ty: const complex[float]
                            const_value: Complex(1.25, 2.5)
                            kind: SymbolId(8)
    "#]],
    );
}

#[test]
fn real_and_imag_preserve_32_bit_component_width() {
    let source = "
        const complex[float[32]] value = 3.5 + 4.25 im;
        real(value);
        imag(value);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ClassicalDeclarationStmt [9-56]:
            symbol_id: 8
            ty_span: [15-33]
            ty_exprs:
                Expr [29-31]:
                    ty: const uint
                    const_value: Int(32)
                    kind: Lit: Int(32)
            init_expr: Expr [42-55]:
                ty: const complex[float[32]]
                const_value: Complex(3.5, 4.25)
                kind: Cast [42-55]:
                    ty: const complex[float[32]]
                    ty_exprs: <empty>
                    expr: Expr [42-55]:
                        ty: const complex[float]
                        kind: BinaryOpExpr:
                            op: Add
                            lhs: Expr [42-45]:
                                ty: const complex[float]
                                kind: Lit: Complex(3.5, 0.0)
                            rhs: Expr [48-55]:
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 4.25)
                    kind: Implicit
        ExprStmt [65-77]:
            expr: Expr [65-76]:
                ty: const float[32]
                const_value: Float(3.5)
                kind: BuiltinFunctionCall [65-76]:
                    fn_name_span: [65-69]
                    name: real
                    function_ty: def (const complex[float[32]]) -> const float[32]
                    args:
                        Expr [70-75]:
                            ty: const complex[float[32]]
                            const_value: Complex(3.5, 4.25)
                            kind: SymbolId(8)
        ExprStmt [86-98]:
            expr: Expr [86-97]:
                ty: const float[32]
                const_value: Float(4.25)
                kind: BuiltinFunctionCall [86-97]:
                    fn_name_span: [86-90]
                    name: imag
                    function_ty: def (const complex[float[32]]) -> const float[32]
                    args:
                        Expr [91-96]:
                            ty: const complex[float[32]]
                            const_value: Complex(3.5, 4.25)
                            kind: SymbolId(8)
    "#]],
    );
}

#[test]
fn real_and_imag_preserve_64_bit_component_width() {
    let source = "
        const complex[float[64]] value = 5.75 + 6.125 im;
        real(value);
        imag(value);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
        ClassicalDeclarationStmt [9-58]:
            symbol_id: 8
            ty_span: [15-33]
            ty_exprs:
                Expr [29-31]:
                    ty: const uint
                    const_value: Int(64)
                    kind: Lit: Int(64)
            init_expr: Expr [42-57]:
                ty: const complex[float[64]]
                const_value: Complex(5.75, 6.125)
                kind: Cast [42-57]:
                    ty: const complex[float[64]]
                    ty_exprs: <empty>
                    expr: Expr [42-57]:
                        ty: const complex[float]
                        kind: BinaryOpExpr:
                            op: Add
                            lhs: Expr [42-46]:
                                ty: const complex[float]
                                kind: Lit: Complex(5.75, 0.0)
                            rhs: Expr [49-57]:
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 6.125)
                    kind: Implicit
        ExprStmt [67-79]:
            expr: Expr [67-78]:
                ty: const float[64]
                const_value: Float(5.75)
                kind: BuiltinFunctionCall [67-78]:
                    fn_name_span: [67-71]
                    name: real
                    function_ty: def (const complex[float[64]]) -> const float[64]
                    args:
                        Expr [72-77]:
                            ty: const complex[float[64]]
                            const_value: Complex(5.75, 6.125)
                            kind: SymbolId(8)
        ExprStmt [88-100]:
            expr: Expr [88-99]:
                ty: const float[64]
                const_value: Float(6.125)
                kind: BuiltinFunctionCall [88-99]:
                    fn_name_span: [88-92]
                    name: imag
                    function_ty: def (const complex[float[64]]) -> const float[64]
                    args:
                        Expr [93-98]:
                            ty: const complex[float[64]]
                            const_value: Complex(5.75, 6.125)
                            kind: SymbolId(8)
    "#]],
    );
}

#[test]
fn real_rejects_non_complex_input() {
    check_stmt_kinds(
        "real(1.0);",
        &expect![[r#"
        Program:
            version: <none>
            pragmas: <empty>
            statements:
                Stmt [0-10]:
                    annotations: <empty>
                    kind: Err

        [Qdk.Qasm.Lowerer.NoValidOverloadForBuiltinFunction

          x There is no valid overload of `real` for inputs: (const float)
          | Overloads available are:
          |     def real(const complex[float]) -> const float
           ,-[test:1:1]
         1 | real(1.0);
           : ^^^^^^^^^
           `----
        ]"#]],
    );
}

#[test]
fn imag_rejects_zero_arguments() {
    check_stmt_kinds(
        "imag();",
        &expect![[r#"
        Program:
            version: <none>
            pragmas: <empty>
            statements:
                Stmt [0-7]:
                    annotations: <empty>
                    kind: Err

        [Qdk.Qasm.Lowerer.NoValidOverloadForBuiltinFunction

          x There is no valid overload of `imag` for inputs: ()
          | Overloads available are:
          |     def imag(const complex[float]) -> const float
           ,-[test:1:1]
         1 | imag();
           : ^^^^^^
           `----
        ]"#]],
    );
}

#[test]
fn imag_rejects_multiple_arguments() {
    check_stmt_kinds(
        "imag(1im, 2im);",
        &expect![[r#"
        Program:
            version: <none>
            pragmas: <empty>
            statements:
                Stmt [0-15]:
                    annotations: <empty>
                    kind: Err

        [Qdk.Qasm.Lowerer.NoValidOverloadForBuiltinFunction

          x There is no valid overload of `imag` for inputs: (const complex[float],
          | const complex[float])
          | Overloads available are:
          |     def imag(const complex[float]) -> const float
           ,-[test:1:1]
         1 | imag(1im, 2im);
           : ^^^^^^^^^^^^^^
           `----
        ]"#]],
    );
}

#[test]
fn real_rejects_nonconstant_input() {
    let source = "
        complex value = 1.0 + 2.0 im;
        real(value);
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-38]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-38]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [25-37]:
                                ty: complex[float]
                                kind: BinaryOpExpr:
                                    op: Add
                                    lhs: Expr [25-28]:
                                        ty: const complex[float]
                                        kind: Lit: Complex(1.0, 0.0)
                                    rhs: Expr [31-37]:
                                        ty: const complex[float]
                                        kind: Lit: Complex(0.0, 2.0)
                    Stmt [47-59]:
                        annotations: <empty>
                        kind: Err

            [Qdk.Qasm.Lowerer.ExprMustBeConst

              x expression must be const
               ,-[test:3:14]
             2 |         complex value = 1.0 + 2.0 im;
             3 |         real(value);
               :              ^^^^^
             4 |     
               `----
            ]"#]],
    );
}
