// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decls;

#[test]
fn greater_than() {
    let input = "
        uint x = 5;
        uint y = 3;
        bool f = x > y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-19]:
                    ty: const uint
                    kind: Lit: Int(5)
            [8] Symbol [14-15]:
                name: x
                type: uint
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [29-40]:
                symbol_id: 9
                ty_span: [29-33]
                ty_exprs: <empty>
                init_expr: Expr [38-39]:
                    ty: const uint
                    kind: Lit: Int(3)
            [9] Symbol [34-35]:
                name: y
                type: uint
                ty_span: [29-33]
                io_kind: Default
            ClassicalDeclarationStmt [49-64]:
                symbol_id: 10
                ty_span: [49-53]
                ty_exprs: <empty>
                init_expr: Expr [58-63]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Gt
                        lhs: Expr [58-59]:
                            ty: uint
                            kind: SymbolId(8)
                        rhs: Expr [62-63]:
                            ty: uint
                            kind: SymbolId(9)
            [10] Symbol [54-55]:
                name: f
                type: bool
                ty_span: [49-53]
                io_kind: Default
        "#]],
    );
}

#[test]
fn greater_than_equals() {
    let input = "
        uint x = 5;
        uint y = 3;
        bool e = x >= y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-19]:
                    ty: const uint
                    kind: Lit: Int(5)
            [8] Symbol [14-15]:
                name: x
                type: uint
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [29-40]:
                symbol_id: 9
                ty_span: [29-33]
                ty_exprs: <empty>
                init_expr: Expr [38-39]:
                    ty: const uint
                    kind: Lit: Int(3)
            [9] Symbol [34-35]:
                name: y
                type: uint
                ty_span: [29-33]
                io_kind: Default
            ClassicalDeclarationStmt [49-65]:
                symbol_id: 10
                ty_span: [49-53]
                ty_exprs: <empty>
                init_expr: Expr [58-64]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Gte
                        lhs: Expr [58-59]:
                            ty: uint
                            kind: SymbolId(8)
                        rhs: Expr [63-64]:
                            ty: uint
                            kind: SymbolId(9)
            [10] Symbol [54-55]:
                name: e
                type: bool
                ty_span: [49-53]
                io_kind: Default
        "#]],
    );
}

#[test]
fn less_than() {
    let input = "
        uint x = 5;
        uint y = 3;
        bool a = x < y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-19]:
                    ty: const uint
                    kind: Lit: Int(5)
            [8] Symbol [14-15]:
                name: x
                type: uint
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [29-40]:
                symbol_id: 9
                ty_span: [29-33]
                ty_exprs: <empty>
                init_expr: Expr [38-39]:
                    ty: const uint
                    kind: Lit: Int(3)
            [9] Symbol [34-35]:
                name: y
                type: uint
                ty_span: [29-33]
                io_kind: Default
            ClassicalDeclarationStmt [49-64]:
                symbol_id: 10
                ty_span: [49-53]
                ty_exprs: <empty>
                init_expr: Expr [58-63]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Lt
                        lhs: Expr [58-59]:
                            ty: uint
                            kind: SymbolId(8)
                        rhs: Expr [62-63]:
                            ty: uint
                            kind: SymbolId(9)
            [10] Symbol [54-55]:
                name: a
                type: bool
                ty_span: [49-53]
                io_kind: Default
        "#]],
    );
}

#[test]
fn less_than_equals() {
    let input = "
        uint x = 5;
        uint y = 3;
        bool c = x <= y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-19]:
                    ty: const uint
                    kind: Lit: Int(5)
            [8] Symbol [14-15]:
                name: x
                type: uint
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [29-40]:
                symbol_id: 9
                ty_span: [29-33]
                ty_exprs: <empty>
                init_expr: Expr [38-39]:
                    ty: const uint
                    kind: Lit: Int(3)
            [9] Symbol [34-35]:
                name: y
                type: uint
                ty_span: [29-33]
                io_kind: Default
            ClassicalDeclarationStmt [49-65]:
                symbol_id: 10
                ty_span: [49-53]
                ty_exprs: <empty>
                init_expr: Expr [58-64]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Lte
                        lhs: Expr [58-59]:
                            ty: uint
                            kind: SymbolId(8)
                        rhs: Expr [63-64]:
                            ty: uint
                            kind: SymbolId(9)
            [10] Symbol [54-55]:
                name: c
                type: bool
                ty_span: [49-53]
                io_kind: Default
        "#]],
    );
}

#[test]
fn equals() {
    let input = "
        uint x = 5;
        uint y = 3;
        bool b = x == y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-19]:
                    ty: const uint
                    kind: Lit: Int(5)
            [8] Symbol [14-15]:
                name: x
                type: uint
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [29-40]:
                symbol_id: 9
                ty_span: [29-33]
                ty_exprs: <empty>
                init_expr: Expr [38-39]:
                    ty: const uint
                    kind: Lit: Int(3)
            [9] Symbol [34-35]:
                name: y
                type: uint
                ty_span: [29-33]
                io_kind: Default
            ClassicalDeclarationStmt [49-65]:
                symbol_id: 10
                ty_span: [49-53]
                ty_exprs: <empty>
                init_expr: Expr [58-64]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Eq
                        lhs: Expr [58-59]:
                            ty: uint
                            kind: SymbolId(8)
                        rhs: Expr [63-64]:
                            ty: uint
                            kind: SymbolId(9)
            [10] Symbol [54-55]:
                name: b
                type: bool
                ty_span: [49-53]
                io_kind: Default
        "#]],
    );
}

#[test]
fn not_equals() {
    let input = "
        uint x = 5;
        uint y = 3;
        bool d = x != y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-19]:
                    ty: const uint
                    kind: Lit: Int(5)
            [8] Symbol [14-15]:
                name: x
                type: uint
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [29-40]:
                symbol_id: 9
                ty_span: [29-33]
                ty_exprs: <empty>
                init_expr: Expr [38-39]:
                    ty: const uint
                    kind: Lit: Int(3)
            [9] Symbol [34-35]:
                name: y
                type: uint
                ty_span: [29-33]
                io_kind: Default
            ClassicalDeclarationStmt [49-65]:
                symbol_id: 10
                ty_span: [49-53]
                ty_exprs: <empty>
                init_expr: Expr [58-64]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Neq
                        lhs: Expr [58-59]:
                            ty: uint
                            kind: SymbolId(8)
                        rhs: Expr [63-64]:
                            ty: uint
                            kind: SymbolId(9)
            [10] Symbol [54-55]:
                name: d
                type: bool
                ty_span: [49-53]
                io_kind: Default
        "#]],
    );
}
