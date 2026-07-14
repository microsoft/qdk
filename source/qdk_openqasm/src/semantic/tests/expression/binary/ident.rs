// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_stmt_kinds;
#[test]
fn mutable_int_idents_without_width_can_be_multiplied() {
    let input = "
        int x = 5;
        int y = 3;
        x * y;
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-18]:
                    ty: int
                    kind: Lit: Int(5)
            ClassicalDeclarationStmt [28-38]:
                symbol_id: 9
                ty_span: [28-31]
                ty_exprs: <empty>
                init_expr: Expr [36-37]:
                    ty: int
                    kind: Lit: Int(3)
            ExprStmt [47-53]:
                expr: Expr [47-52]:
                    ty: int
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [47-48]:
                            ty: int
                            kind: SymbolId(8)
                        rhs: Expr [51-52]:
                            ty: int
                            kind: SymbolId(9)
        "#]],
    );
}

#[test]
fn const_int_idents_without_width_can_be_multiplied() {
    let input = "
        const int x = 5;
        const int y = 3;
        x * y;
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-25]:
                symbol_id: 8
                ty_span: [15-18]
                ty_exprs: <empty>
                init_expr: Expr [23-24]:
                    ty: const int
                    const_value: Int(5)
                    kind: Lit: Int(5)
            ClassicalDeclarationStmt [34-50]:
                symbol_id: 9
                ty_span: [40-43]
                ty_exprs: <empty>
                init_expr: Expr [48-49]:
                    ty: const int
                    const_value: Int(3)
                    kind: Lit: Int(3)
            ExprStmt [59-65]:
                expr: Expr [59-64]:
                    ty: const int
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [59-60]:
                            ty: const int
                            kind: SymbolId(8)
                        rhs: Expr [63-64]:
                            ty: const int
                            kind: SymbolId(9)
        "#]],
    );
}

#[test]
fn const_and_mut_int_idents_without_width_can_be_multiplied() {
    let input = "
        int x = 5;
        const int y = 3;
        x * y;
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-18]:
                    ty: int
                    kind: Lit: Int(5)
            ClassicalDeclarationStmt [28-44]:
                symbol_id: 9
                ty_span: [34-37]
                ty_exprs: <empty>
                init_expr: Expr [42-43]:
                    ty: const int
                    const_value: Int(3)
                    kind: Lit: Int(3)
            ExprStmt [53-59]:
                expr: Expr [53-58]:
                    ty: int
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [53-54]:
                            ty: int
                            kind: SymbolId(8)
                        rhs: Expr [57-58]:
                            ty: int
                            kind: SymbolId(9)
        "#]],
    );
}

#[test]
fn const_int_idents_widthless_lhs_can_be_multiplied_by_explicit_width_int() {
    let input = "
        const int[32] x = 5;
        const int y = 3;
        x * y;
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-29]:
                symbol_id: 8
                ty_span: [15-22]
                ty_exprs:
                    Expr [19-21]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [27-28]:
                    ty: const int[32]
                    const_value: Int(5)
                    kind: Lit: Int(5)
            ClassicalDeclarationStmt [38-54]:
                symbol_id: 9
                ty_span: [44-47]
                ty_exprs: <empty>
                init_expr: Expr [52-53]:
                    ty: const int
                    const_value: Int(3)
                    kind: Lit: Int(3)
            ExprStmt [63-69]:
                expr: Expr [63-68]:
                    ty: const int
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [63-64]:
                            ty: const int
                            kind: Cast [63-64]:
                                ty: const int
                                ty_exprs: <empty>
                                expr: Expr [63-64]:
                                    ty: const int[32]
                                    kind: SymbolId(8)
                                kind: Implicit
                        rhs: Expr [67-68]:
                            ty: const int
                            kind: SymbolId(9)
        "#]],
    );
}
