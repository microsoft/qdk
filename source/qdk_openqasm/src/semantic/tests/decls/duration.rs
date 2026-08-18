// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::{check_classical_decl, check_classical_decls};

#[test]
fn with_no_init_expr_has_generated_lit_expr() {
    check_classical_decl(
        "duration a;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-11]:
                symbol_id: 8
                ty_span: [0-8]
                ty_exprs: <empty>
                init_expr: Expr [0-11]:
                    ty: duration
                    kind: Lit: Duration(0.0 s)
            [8] Symbol [9-10]:
                name: a
                type: duration
                ty_span: [0-8]
                io_kind: Default"#]],
    );
}

#[test]
fn with_lit_init_expr_has_supplied_value() {
    check_classical_decl(
        "duration a = 5ns;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-17]:
                symbol_id: 8
                ty_span: [0-8]
                ty_exprs: <empty>
                init_expr: Expr [13-16]:
                    ty: const duration
                    kind: Lit: Duration(5.0 ns)
            [8] Symbol [9-10]:
                name: a
                type: duration
                ty_span: [0-8]
                io_kind: Default"#]],
    );
}

#[test]
fn with_var_init_expr_has_supplied_value() {
    check_classical_decls(
        "duration a = 5ns * 3.0; duration b = 2 * a;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [0-8]
                ty_exprs: <empty>
                init_expr: Expr [13-22]:
                    ty: const duration
                    const_value: Duration(15.0 ns)
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [13-16]:
                            ty: const duration
                            kind: Lit: Duration(5.0 ns)
                        rhs: Expr [19-22]:
                            ty: const float
                            kind: Lit: Float(3.0)
            [8] Symbol [9-10]:
                name: a
                type: duration
                ty_span: [0-8]
                io_kind: Default
            ClassicalDeclarationStmt [24-43]:
                symbol_id: 9
                ty_span: [24-32]
                ty_exprs: <empty>
                init_expr: Expr [37-42]:
                    ty: duration
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [37-38]:
                            ty: const int
                            kind: Lit: Int(2)
                        rhs: Expr [41-42]:
                            ty: duration
                            kind: SymbolId(8)
            [9] Symbol [33-34]:
                name: b
                type: duration
                ty_span: [24-32]
                io_kind: Default
        "#]],
    );
}

#[test]
fn with_binop_init_expr_has_supplied_value() {
    check_classical_decl(
        "duration a = 5ns * 3.0;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [0-8]
                ty_exprs: <empty>
                init_expr: Expr [13-22]:
                    ty: const duration
                    const_value: Duration(15.0 ns)
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [13-16]:
                            ty: const duration
                            kind: Lit: Duration(5.0 ns)
                        rhs: Expr [19-22]:
                            ty: const float
                            kind: Lit: Float(3.0)
            [8] Symbol [9-10]:
                name: a
                type: duration
                ty_span: [0-8]
                io_kind: Default"#]],
    );
}
