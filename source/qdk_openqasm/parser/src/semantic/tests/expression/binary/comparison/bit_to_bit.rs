// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decls;

#[test]
fn logical_and() {
    let input = "
        bit x = 1;
        bit y = 0;
        bool a = x && y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-18]:
                    ty: const bit
                    kind: Lit: Bit(1)
            [8] Symbol [13-14]:
                name: x
                type: bit
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [28-38]:
                symbol_id: 9
                ty_span: [28-31]
                ty_exprs: <empty>
                init_expr: Expr [36-37]:
                    ty: const bit
                    kind: Lit: Bit(0)
            [9] Symbol [32-33]:
                name: y
                type: bit
                ty_span: [28-31]
                io_kind: Default
            ClassicalDeclarationStmt [47-63]:
                symbol_id: 10
                ty_span: [47-51]
                ty_exprs: <empty>
                init_expr: Expr [56-62]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: AndL
                        lhs: Expr [56-57]:
                            ty: bool
                            kind: Cast [56-57]:
                                ty: bool
                                ty_exprs: <empty>
                                expr: Expr [56-57]:
                                    ty: bit
                                    kind: SymbolId(8)
                                kind: Implicit
                        rhs: Expr [61-62]:
                            ty: bool
                            kind: Cast [61-62]:
                                ty: bool
                                ty_exprs: <empty>
                                expr: Expr [61-62]:
                                    ty: bit
                                    kind: SymbolId(9)
                                kind: Implicit
            [10] Symbol [52-53]:
                name: a
                type: bool
                ty_span: [47-51]
                io_kind: Default
        "#]],
    );
}

#[test]
fn logical_or() {
    let input = "
        bit x = 1;
        bit y = 0;
        bool a = x || y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-18]:
                    ty: const bit
                    kind: Lit: Bit(1)
            [8] Symbol [13-14]:
                name: x
                type: bit
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [28-38]:
                symbol_id: 9
                ty_span: [28-31]
                ty_exprs: <empty>
                init_expr: Expr [36-37]:
                    ty: const bit
                    kind: Lit: Bit(0)
            [9] Symbol [32-33]:
                name: y
                type: bit
                ty_span: [28-31]
                io_kind: Default
            ClassicalDeclarationStmt [47-63]:
                symbol_id: 10
                ty_span: [47-51]
                ty_exprs: <empty>
                init_expr: Expr [56-62]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: OrL
                        lhs: Expr [56-57]:
                            ty: bool
                            kind: Cast [56-57]:
                                ty: bool
                                ty_exprs: <empty>
                                expr: Expr [56-57]:
                                    ty: bit
                                    kind: SymbolId(8)
                                kind: Implicit
                        rhs: Expr [61-62]:
                            ty: bool
                            kind: Cast [61-62]:
                                ty: bool
                                ty_exprs: <empty>
                                expr: Expr [61-62]:
                                    ty: bit
                                    kind: SymbolId(9)
                                kind: Implicit
            [10] Symbol [52-53]:
                name: a
                type: bool
                ty_span: [47-51]
                io_kind: Default
        "#]],
    );
}

#[test]
fn unop_not_logical_and_unop_not() {
    let input = "
        bit x = 1;
        bit y = 0;
        bool a = !x && !y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-18]:
                    ty: const bit
                    kind: Lit: Bit(1)
            [8] Symbol [13-14]:
                name: x
                type: bit
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [28-38]:
                symbol_id: 9
                ty_span: [28-31]
                ty_exprs: <empty>
                init_expr: Expr [36-37]:
                    ty: const bit
                    kind: Lit: Bit(0)
            [9] Symbol [32-33]:
                name: y
                type: bit
                ty_span: [28-31]
                io_kind: Default
            ClassicalDeclarationStmt [47-65]:
                symbol_id: 10
                ty_span: [47-51]
                ty_exprs: <empty>
                init_expr: Expr [56-64]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: AndL
                        lhs: Expr [57-58]:
                            ty: bool
                            kind: UnaryOpExpr [57-58]:
                                op: NotL
                                expr: Expr [57-58]:
                                    ty: bool
                                    kind: Cast [57-58]:
                                        ty: bool
                                        ty_exprs: <empty>
                                        expr: Expr [57-58]:
                                            ty: bit
                                            kind: SymbolId(8)
                                        kind: Implicit
                        rhs: Expr [63-64]:
                            ty: bool
                            kind: UnaryOpExpr [63-64]:
                                op: NotL
                                expr: Expr [63-64]:
                                    ty: bool
                                    kind: Cast [63-64]:
                                        ty: bool
                                        ty_exprs: <empty>
                                        expr: Expr [63-64]:
                                            ty: bit
                                            kind: SymbolId(9)
                                        kind: Implicit
            [10] Symbol [52-53]:
                name: a
                type: bool
                ty_span: [47-51]
                io_kind: Default
        "#]],
    );
}

#[test]
fn unop_not_logical_or_unop_not() {
    let input = "
        bit x = 1;
        bit y = 0;
        bool a = !x || !y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-18]:
                    ty: const bit
                    kind: Lit: Bit(1)
            [8] Symbol [13-14]:
                name: x
                type: bit
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [28-38]:
                symbol_id: 9
                ty_span: [28-31]
                ty_exprs: <empty>
                init_expr: Expr [36-37]:
                    ty: const bit
                    kind: Lit: Bit(0)
            [9] Symbol [32-33]:
                name: y
                type: bit
                ty_span: [28-31]
                io_kind: Default
            ClassicalDeclarationStmt [47-65]:
                symbol_id: 10
                ty_span: [47-51]
                ty_exprs: <empty>
                init_expr: Expr [56-64]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: OrL
                        lhs: Expr [57-58]:
                            ty: bool
                            kind: UnaryOpExpr [57-58]:
                                op: NotL
                                expr: Expr [57-58]:
                                    ty: bool
                                    kind: Cast [57-58]:
                                        ty: bool
                                        ty_exprs: <empty>
                                        expr: Expr [57-58]:
                                            ty: bit
                                            kind: SymbolId(8)
                                        kind: Implicit
                        rhs: Expr [63-64]:
                            ty: bool
                            kind: UnaryOpExpr [63-64]:
                                op: NotL
                                expr: Expr [63-64]:
                                    ty: bool
                                    kind: Cast [63-64]:
                                        ty: bool
                                        ty_exprs: <empty>
                                        expr: Expr [63-64]:
                                            ty: bit
                                            kind: SymbolId(9)
                                        kind: Implicit
            [10] Symbol [52-53]:
                name: a
                type: bool
                ty_span: [47-51]
                io_kind: Default
        "#]],
    );
}

#[test]
fn unop_not_logical_and() {
    let input = "
        bit x = 1;
        bit y = 0;
        bool a = !x && y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-18]:
                    ty: const bit
                    kind: Lit: Bit(1)
            [8] Symbol [13-14]:
                name: x
                type: bit
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [28-38]:
                symbol_id: 9
                ty_span: [28-31]
                ty_exprs: <empty>
                init_expr: Expr [36-37]:
                    ty: const bit
                    kind: Lit: Bit(0)
            [9] Symbol [32-33]:
                name: y
                type: bit
                ty_span: [28-31]
                io_kind: Default
            ClassicalDeclarationStmt [47-64]:
                symbol_id: 10
                ty_span: [47-51]
                ty_exprs: <empty>
                init_expr: Expr [56-63]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: AndL
                        lhs: Expr [57-58]:
                            ty: bool
                            kind: UnaryOpExpr [57-58]:
                                op: NotL
                                expr: Expr [57-58]:
                                    ty: bool
                                    kind: Cast [57-58]:
                                        ty: bool
                                        ty_exprs: <empty>
                                        expr: Expr [57-58]:
                                            ty: bit
                                            kind: SymbolId(8)
                                        kind: Implicit
                        rhs: Expr [62-63]:
                            ty: bool
                            kind: Cast [62-63]:
                                ty: bool
                                ty_exprs: <empty>
                                expr: Expr [62-63]:
                                    ty: bit
                                    kind: SymbolId(9)
                                kind: Implicit
            [10] Symbol [52-53]:
                name: a
                type: bool
                ty_span: [47-51]
                io_kind: Default
        "#]],
    );
}

#[test]
fn unop_not_logical_or() {
    let input = "
        bit x = 1;
        bit y = 0;
        bool a = !x || y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-18]:
                    ty: const bit
                    kind: Lit: Bit(1)
            [8] Symbol [13-14]:
                name: x
                type: bit
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [28-38]:
                symbol_id: 9
                ty_span: [28-31]
                ty_exprs: <empty>
                init_expr: Expr [36-37]:
                    ty: const bit
                    kind: Lit: Bit(0)
            [9] Symbol [32-33]:
                name: y
                type: bit
                ty_span: [28-31]
                io_kind: Default
            ClassicalDeclarationStmt [47-64]:
                symbol_id: 10
                ty_span: [47-51]
                ty_exprs: <empty>
                init_expr: Expr [56-63]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: OrL
                        lhs: Expr [57-58]:
                            ty: bool
                            kind: UnaryOpExpr [57-58]:
                                op: NotL
                                expr: Expr [57-58]:
                                    ty: bool
                                    kind: Cast [57-58]:
                                        ty: bool
                                        ty_exprs: <empty>
                                        expr: Expr [57-58]:
                                            ty: bit
                                            kind: SymbolId(8)
                                        kind: Implicit
                        rhs: Expr [62-63]:
                            ty: bool
                            kind: Cast [62-63]:
                                ty: bool
                                ty_exprs: <empty>
                                expr: Expr [62-63]:
                                    ty: bit
                                    kind: SymbolId(9)
                                kind: Implicit
            [10] Symbol [52-53]:
                name: a
                type: bool
                ty_span: [47-51]
                io_kind: Default
        "#]],
    );
}

#[test]
fn logical_and_unop_not() {
    let input = "
        bit x = 1;
        bit y = 0;
        bool a = x && !y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-18]:
                    ty: const bit
                    kind: Lit: Bit(1)
            [8] Symbol [13-14]:
                name: x
                type: bit
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [28-38]:
                symbol_id: 9
                ty_span: [28-31]
                ty_exprs: <empty>
                init_expr: Expr [36-37]:
                    ty: const bit
                    kind: Lit: Bit(0)
            [9] Symbol [32-33]:
                name: y
                type: bit
                ty_span: [28-31]
                io_kind: Default
            ClassicalDeclarationStmt [47-64]:
                symbol_id: 10
                ty_span: [47-51]
                ty_exprs: <empty>
                init_expr: Expr [56-63]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: AndL
                        lhs: Expr [56-57]:
                            ty: bool
                            kind: Cast [56-57]:
                                ty: bool
                                ty_exprs: <empty>
                                expr: Expr [56-57]:
                                    ty: bit
                                    kind: SymbolId(8)
                                kind: Implicit
                        rhs: Expr [62-63]:
                            ty: bool
                            kind: UnaryOpExpr [62-63]:
                                op: NotL
                                expr: Expr [62-63]:
                                    ty: bool
                                    kind: Cast [62-63]:
                                        ty: bool
                                        ty_exprs: <empty>
                                        expr: Expr [62-63]:
                                            ty: bit
                                            kind: SymbolId(9)
                                        kind: Implicit
            [10] Symbol [52-53]:
                name: a
                type: bool
                ty_span: [47-51]
                io_kind: Default
        "#]],
    );
}

#[test]
fn logical_or_unop_not() {
    let input = "
        bit x = 1;
        bit y = 0;
        bool a = x || !y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-18]:
                    ty: const bit
                    kind: Lit: Bit(1)
            [8] Symbol [13-14]:
                name: x
                type: bit
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [28-38]:
                symbol_id: 9
                ty_span: [28-31]
                ty_exprs: <empty>
                init_expr: Expr [36-37]:
                    ty: const bit
                    kind: Lit: Bit(0)
            [9] Symbol [32-33]:
                name: y
                type: bit
                ty_span: [28-31]
                io_kind: Default
            ClassicalDeclarationStmt [47-64]:
                symbol_id: 10
                ty_span: [47-51]
                ty_exprs: <empty>
                init_expr: Expr [56-63]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: OrL
                        lhs: Expr [56-57]:
                            ty: bool
                            kind: Cast [56-57]:
                                ty: bool
                                ty_exprs: <empty>
                                expr: Expr [56-57]:
                                    ty: bit
                                    kind: SymbolId(8)
                                kind: Implicit
                        rhs: Expr [62-63]:
                            ty: bool
                            kind: UnaryOpExpr [62-63]:
                                op: NotL
                                expr: Expr [62-63]:
                                    ty: bool
                                    kind: Cast [62-63]:
                                        ty: bool
                                        ty_exprs: <empty>
                                        expr: Expr [62-63]:
                                            ty: bit
                                            kind: SymbolId(9)
                                        kind: Implicit
            [10] Symbol [52-53]:
                name: a
                type: bool
                ty_span: [47-51]
                io_kind: Default
        "#]],
    );
}
