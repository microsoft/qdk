// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decls;

#[test]
fn logical_and() {
    let input = "
        bool x = true;
        bool y = false;
        bool a = x && y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [32-47]:
                symbol_id: 9
                ty_span: [32-36]
                ty_exprs: <empty>
                init_expr: Expr [41-46]:
                    ty: bool
                    kind: Lit: Bool(false)
            [9] Symbol [37-38]:
                name: y
                type: bool
                ty_span: [32-36]
                io_kind: Default
            ClassicalDeclarationStmt [56-72]:
                symbol_id: 10
                ty_span: [56-60]
                ty_exprs: <empty>
                init_expr: Expr [65-71]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: AndL
                        lhs: Expr [65-66]:
                            ty: bool
                            kind: SymbolId(8)
                        rhs: Expr [70-71]:
                            ty: bool
                            kind: SymbolId(9)
            [10] Symbol [61-62]:
                name: a
                type: bool
                ty_span: [56-60]
                io_kind: Default
        "#]],
    );
}

#[test]
fn logical_or() {
    let input = "
        bool x = true;
        bool y = false;
        bool a = x || y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [32-47]:
                symbol_id: 9
                ty_span: [32-36]
                ty_exprs: <empty>
                init_expr: Expr [41-46]:
                    ty: bool
                    kind: Lit: Bool(false)
            [9] Symbol [37-38]:
                name: y
                type: bool
                ty_span: [32-36]
                io_kind: Default
            ClassicalDeclarationStmt [56-72]:
                symbol_id: 10
                ty_span: [56-60]
                ty_exprs: <empty>
                init_expr: Expr [65-71]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: OrL
                        lhs: Expr [65-66]:
                            ty: bool
                            kind: SymbolId(8)
                        rhs: Expr [70-71]:
                            ty: bool
                            kind: SymbolId(9)
            [10] Symbol [61-62]:
                name: a
                type: bool
                ty_span: [56-60]
                io_kind: Default
        "#]],
    );
}

#[test]
fn unop_not_logical_and_unop_not() {
    let input = "
        bool x = true;
        bool y = false;
        bool a = !x && !y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [32-47]:
                symbol_id: 9
                ty_span: [32-36]
                ty_exprs: <empty>
                init_expr: Expr [41-46]:
                    ty: bool
                    kind: Lit: Bool(false)
            [9] Symbol [37-38]:
                name: y
                type: bool
                ty_span: [32-36]
                io_kind: Default
            ClassicalDeclarationStmt [56-74]:
                symbol_id: 10
                ty_span: [56-60]
                ty_exprs: <empty>
                init_expr: Expr [65-73]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: AndL
                        lhs: Expr [66-67]:
                            ty: bool
                            kind: UnaryOpExpr [66-67]:
                                op: NotL
                                expr: Expr [66-67]:
                                    ty: bool
                                    kind: SymbolId(8)
                        rhs: Expr [72-73]:
                            ty: bool
                            kind: UnaryOpExpr [72-73]:
                                op: NotL
                                expr: Expr [72-73]:
                                    ty: bool
                                    kind: SymbolId(9)
            [10] Symbol [61-62]:
                name: a
                type: bool
                ty_span: [56-60]
                io_kind: Default
        "#]],
    );
}

#[test]
fn unop_not_logical_or_unop_not() {
    let input = "
        bool x = true;
        bool y = false;
        bool a = !x || !y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [32-47]:
                symbol_id: 9
                ty_span: [32-36]
                ty_exprs: <empty>
                init_expr: Expr [41-46]:
                    ty: bool
                    kind: Lit: Bool(false)
            [9] Symbol [37-38]:
                name: y
                type: bool
                ty_span: [32-36]
                io_kind: Default
            ClassicalDeclarationStmt [56-74]:
                symbol_id: 10
                ty_span: [56-60]
                ty_exprs: <empty>
                init_expr: Expr [65-73]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: OrL
                        lhs: Expr [66-67]:
                            ty: bool
                            kind: UnaryOpExpr [66-67]:
                                op: NotL
                                expr: Expr [66-67]:
                                    ty: bool
                                    kind: SymbolId(8)
                        rhs: Expr [72-73]:
                            ty: bool
                            kind: UnaryOpExpr [72-73]:
                                op: NotL
                                expr: Expr [72-73]:
                                    ty: bool
                                    kind: SymbolId(9)
            [10] Symbol [61-62]:
                name: a
                type: bool
                ty_span: [56-60]
                io_kind: Default
        "#]],
    );
}

#[test]
fn unop_not_logical_and() {
    let input = "
        bool x = true;
        bool y = false;
        bool a = !x && y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [32-47]:
                symbol_id: 9
                ty_span: [32-36]
                ty_exprs: <empty>
                init_expr: Expr [41-46]:
                    ty: bool
                    kind: Lit: Bool(false)
            [9] Symbol [37-38]:
                name: y
                type: bool
                ty_span: [32-36]
                io_kind: Default
            ClassicalDeclarationStmt [56-73]:
                symbol_id: 10
                ty_span: [56-60]
                ty_exprs: <empty>
                init_expr: Expr [65-72]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: AndL
                        lhs: Expr [66-67]:
                            ty: bool
                            kind: UnaryOpExpr [66-67]:
                                op: NotL
                                expr: Expr [66-67]:
                                    ty: bool
                                    kind: SymbolId(8)
                        rhs: Expr [71-72]:
                            ty: bool
                            kind: SymbolId(9)
            [10] Symbol [61-62]:
                name: a
                type: bool
                ty_span: [56-60]
                io_kind: Default
        "#]],
    );
}

#[test]
fn unop_not_logical_or() {
    let input = "
        bool x = true;
        bool y = false;
        bool a = !x || y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [32-47]:
                symbol_id: 9
                ty_span: [32-36]
                ty_exprs: <empty>
                init_expr: Expr [41-46]:
                    ty: bool
                    kind: Lit: Bool(false)
            [9] Symbol [37-38]:
                name: y
                type: bool
                ty_span: [32-36]
                io_kind: Default
            ClassicalDeclarationStmt [56-73]:
                symbol_id: 10
                ty_span: [56-60]
                ty_exprs: <empty>
                init_expr: Expr [65-72]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: OrL
                        lhs: Expr [66-67]:
                            ty: bool
                            kind: UnaryOpExpr [66-67]:
                                op: NotL
                                expr: Expr [66-67]:
                                    ty: bool
                                    kind: SymbolId(8)
                        rhs: Expr [71-72]:
                            ty: bool
                            kind: SymbolId(9)
            [10] Symbol [61-62]:
                name: a
                type: bool
                ty_span: [56-60]
                io_kind: Default
        "#]],
    );
}

#[test]
fn logical_and_unop_not() {
    let input = "
        bool x = true;
        bool y = false;
        bool a = x && !y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [32-47]:
                symbol_id: 9
                ty_span: [32-36]
                ty_exprs: <empty>
                init_expr: Expr [41-46]:
                    ty: bool
                    kind: Lit: Bool(false)
            [9] Symbol [37-38]:
                name: y
                type: bool
                ty_span: [32-36]
                io_kind: Default
            ClassicalDeclarationStmt [56-73]:
                symbol_id: 10
                ty_span: [56-60]
                ty_exprs: <empty>
                init_expr: Expr [65-72]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: AndL
                        lhs: Expr [65-66]:
                            ty: bool
                            kind: SymbolId(8)
                        rhs: Expr [71-72]:
                            ty: bool
                            kind: UnaryOpExpr [71-72]:
                                op: NotL
                                expr: Expr [71-72]:
                                    ty: bool
                                    kind: SymbolId(9)
            [10] Symbol [61-62]:
                name: a
                type: bool
                ty_span: [56-60]
                io_kind: Default
        "#]],
    );
}

#[test]
fn logical_or_unop_not() {
    let input = "
        bool x = true;
        bool y = false;
        bool a = x || !y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                ty_span: [9-13]
                io_kind: Default
            ClassicalDeclarationStmt [32-47]:
                symbol_id: 9
                ty_span: [32-36]
                ty_exprs: <empty>
                init_expr: Expr [41-46]:
                    ty: bool
                    kind: Lit: Bool(false)
            [9] Symbol [37-38]:
                name: y
                type: bool
                ty_span: [32-36]
                io_kind: Default
            ClassicalDeclarationStmt [56-73]:
                symbol_id: 10
                ty_span: [56-60]
                ty_exprs: <empty>
                init_expr: Expr [65-72]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: OrL
                        lhs: Expr [65-66]:
                            ty: bool
                            kind: SymbolId(8)
                        rhs: Expr [71-72]:
                            ty: bool
                            kind: UnaryOpExpr [71-72]:
                                op: NotL
                                expr: Expr [71-72]:
                                    ty: bool
                                    kind: SymbolId(9)
            [10] Symbol [61-62]:
                name: a
                type: bool
                ty_span: [56-60]
                io_kind: Default
        "#]],
    );
}
