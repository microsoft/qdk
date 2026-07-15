// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decls;

#[test]
fn greater_than() {
    let input = "
        float x = 5.;
        float y = 3.;
        bool f = x > y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-22]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-21]:
                    ty: float
                    kind: Lit: Float(5.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [31-44]:
                symbol_id: 9
                ty_span: [31-36]
                ty_exprs: <empty>
                init_expr: Expr [41-43]:
                    ty: float
                    kind: Lit: Float(3.0)
            [9] Symbol [37-38]:
                name: y
                type: float
                ty_span: [31-36]
                io_kind: Default
            ClassicalDeclarationStmt [53-68]:
                symbol_id: 10
                ty_span: [53-57]
                ty_exprs: <empty>
                init_expr: Expr [62-67]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Gt
                        lhs: Expr [62-63]:
                            ty: float
                            kind: SymbolId(8)
                        rhs: Expr [66-67]:
                            ty: float
                            kind: SymbolId(9)
            [10] Symbol [58-59]:
                name: f
                type: bool
                ty_span: [53-57]
                io_kind: Default
        "#]],
    );
}

#[test]
fn greater_than_equals() {
    let input = "
        float x = 5.;
        float y = 3.;
        bool e = x >= y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-22]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-21]:
                    ty: float
                    kind: Lit: Float(5.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [31-44]:
                symbol_id: 9
                ty_span: [31-36]
                ty_exprs: <empty>
                init_expr: Expr [41-43]:
                    ty: float
                    kind: Lit: Float(3.0)
            [9] Symbol [37-38]:
                name: y
                type: float
                ty_span: [31-36]
                io_kind: Default
            ClassicalDeclarationStmt [53-69]:
                symbol_id: 10
                ty_span: [53-57]
                ty_exprs: <empty>
                init_expr: Expr [62-68]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Gte
                        lhs: Expr [62-63]:
                            ty: float
                            kind: SymbolId(8)
                        rhs: Expr [67-68]:
                            ty: float
                            kind: SymbolId(9)
            [10] Symbol [58-59]:
                name: e
                type: bool
                ty_span: [53-57]
                io_kind: Default
        "#]],
    );
}

#[test]
fn less_than() {
    let input = "
        float x = 5.;
        float y = 3.;
        bool a = x < y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-22]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-21]:
                    ty: float
                    kind: Lit: Float(5.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [31-44]:
                symbol_id: 9
                ty_span: [31-36]
                ty_exprs: <empty>
                init_expr: Expr [41-43]:
                    ty: float
                    kind: Lit: Float(3.0)
            [9] Symbol [37-38]:
                name: y
                type: float
                ty_span: [31-36]
                io_kind: Default
            ClassicalDeclarationStmt [53-68]:
                symbol_id: 10
                ty_span: [53-57]
                ty_exprs: <empty>
                init_expr: Expr [62-67]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Lt
                        lhs: Expr [62-63]:
                            ty: float
                            kind: SymbolId(8)
                        rhs: Expr [66-67]:
                            ty: float
                            kind: SymbolId(9)
            [10] Symbol [58-59]:
                name: a
                type: bool
                ty_span: [53-57]
                io_kind: Default
        "#]],
    );
}

#[test]
fn less_than_equals() {
    let input = "
        float x = 5.;
        float y = 3.;
        bool c = x <= y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-22]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-21]:
                    ty: float
                    kind: Lit: Float(5.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [31-44]:
                symbol_id: 9
                ty_span: [31-36]
                ty_exprs: <empty>
                init_expr: Expr [41-43]:
                    ty: float
                    kind: Lit: Float(3.0)
            [9] Symbol [37-38]:
                name: y
                type: float
                ty_span: [31-36]
                io_kind: Default
            ClassicalDeclarationStmt [53-69]:
                symbol_id: 10
                ty_span: [53-57]
                ty_exprs: <empty>
                init_expr: Expr [62-68]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Lte
                        lhs: Expr [62-63]:
                            ty: float
                            kind: SymbolId(8)
                        rhs: Expr [67-68]:
                            ty: float
                            kind: SymbolId(9)
            [10] Symbol [58-59]:
                name: c
                type: bool
                ty_span: [53-57]
                io_kind: Default
        "#]],
    );
}

#[test]
fn equals() {
    let input = "
        float x = 5.;
        float y = 3.;
        bool b = x == y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-22]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-21]:
                    ty: float
                    kind: Lit: Float(5.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [31-44]:
                symbol_id: 9
                ty_span: [31-36]
                ty_exprs: <empty>
                init_expr: Expr [41-43]:
                    ty: float
                    kind: Lit: Float(3.0)
            [9] Symbol [37-38]:
                name: y
                type: float
                ty_span: [31-36]
                io_kind: Default
            ClassicalDeclarationStmt [53-69]:
                symbol_id: 10
                ty_span: [53-57]
                ty_exprs: <empty>
                init_expr: Expr [62-68]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Eq
                        lhs: Expr [62-63]:
                            ty: float
                            kind: SymbolId(8)
                        rhs: Expr [67-68]:
                            ty: float
                            kind: SymbolId(9)
            [10] Symbol [58-59]:
                name: b
                type: bool
                ty_span: [53-57]
                io_kind: Default
        "#]],
    );
}

#[test]
fn not_equals() {
    let input = "
        float x = 5.;
        float y = 3.;
        bool d = x != y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-22]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-21]:
                    ty: float
                    kind: Lit: Float(5.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [31-44]:
                symbol_id: 9
                ty_span: [31-36]
                ty_exprs: <empty>
                init_expr: Expr [41-43]:
                    ty: float
                    kind: Lit: Float(3.0)
            [9] Symbol [37-38]:
                name: y
                type: float
                ty_span: [31-36]
                io_kind: Default
            ClassicalDeclarationStmt [53-69]:
                symbol_id: 10
                ty_span: [53-57]
                ty_exprs: <empty>
                init_expr: Expr [62-68]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Neq
                        lhs: Expr [62-63]:
                            ty: float
                            kind: SymbolId(8)
                        rhs: Expr [67-68]:
                            ty: float
                            kind: SymbolId(9)
            [10] Symbol [58-59]:
                name: d
                type: bool
                ty_span: [53-57]
                io_kind: Default
        "#]],
    );
}
