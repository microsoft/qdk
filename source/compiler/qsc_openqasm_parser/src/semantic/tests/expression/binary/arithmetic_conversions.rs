// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_stmt_kinds;

#[test]
fn int_idents_without_width_can_be_multiplied() {
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
fn int_idents_with_same_width_can_be_multiplied() {
    let input = "
        int[32] x = 5;
        int[32] y = 3;
        x * y;
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [21-22]:
                    ty: const int[32]
                    kind: Lit: Int(5)
            ClassicalDeclarationStmt [32-46]:
                symbol_id: 9
                ty_span: [32-39]
                ty_exprs:
                    Expr [36-38]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [44-45]:
                    ty: const int[32]
                    kind: Lit: Int(3)
            ExprStmt [55-61]:
                expr: Expr [55-60]:
                    ty: int[32]
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [55-56]:
                            ty: int[32]
                            kind: SymbolId(8)
                        rhs: Expr [59-60]:
                            ty: int[32]
                            kind: SymbolId(9)
        "#]],
    );
}

#[test]
fn int_idents_with_different_width_can_be_multiplied() {
    let input = "
        int[32] x = 5;
        int[64] y = 3;
        x * y;
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [21-22]:
                    ty: const int[32]
                    kind: Lit: Int(5)
            ClassicalDeclarationStmt [32-46]:
                symbol_id: 9
                ty_span: [32-39]
                ty_exprs:
                    Expr [36-38]:
                        ty: const uint
                        const_value: Int(64)
                        kind: Lit: Int(64)
                init_expr: Expr [44-45]:
                    ty: const int[64]
                    kind: Lit: Int(3)
            ExprStmt [55-61]:
                expr: Expr [55-60]:
                    ty: int[64]
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [55-56]:
                            ty: int[64]
                            kind: Cast [55-56]:
                                ty: int[64]
                                ty_exprs: <empty>
                                expr: Expr [55-56]:
                                    ty: int[32]
                                    kind: SymbolId(8)
                                kind: Implicit
                        rhs: Expr [59-60]:
                            ty: int[64]
                            kind: SymbolId(9)
        "#]],
    );
}

#[test]
fn multiplying_int_idents_with_different_width_result_in_higher_width_result() {
    let input = "
        int[32] x = 5;
        int[64] y = 3;
        int[64] z = x * y;
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [21-22]:
                    ty: const int[32]
                    kind: Lit: Int(5)
            ClassicalDeclarationStmt [32-46]:
                symbol_id: 9
                ty_span: [32-39]
                ty_exprs:
                    Expr [36-38]:
                        ty: const uint
                        const_value: Int(64)
                        kind: Lit: Int(64)
                init_expr: Expr [44-45]:
                    ty: const int[64]
                    kind: Lit: Int(3)
            ClassicalDeclarationStmt [55-73]:
                symbol_id: 10
                ty_span: [55-62]
                ty_exprs:
                    Expr [59-61]:
                        ty: const uint
                        const_value: Int(64)
                        kind: Lit: Int(64)
                init_expr: Expr [67-72]:
                    ty: int[64]
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [67-68]:
                            ty: int[64]
                            kind: Cast [67-68]:
                                ty: int[64]
                                ty_exprs: <empty>
                                expr: Expr [67-68]:
                                    ty: int[32]
                                    kind: SymbolId(8)
                                kind: Implicit
                        rhs: Expr [71-72]:
                            ty: int[64]
                            kind: SymbolId(9)
        "#]],
    );
}

#[test]
fn multiplying_int_idents_with_different_width_result_in_no_width_result() {
    let input = "
        int[32] x = 5;
        int[64] y = 3;
        int z = x * y;
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [21-22]:
                    ty: const int[32]
                    kind: Lit: Int(5)
            ClassicalDeclarationStmt [32-46]:
                symbol_id: 9
                ty_span: [32-39]
                ty_exprs:
                    Expr [36-38]:
                        ty: const uint
                        const_value: Int(64)
                        kind: Lit: Int(64)
                init_expr: Expr [44-45]:
                    ty: const int[64]
                    kind: Lit: Int(3)
            ClassicalDeclarationStmt [55-69]:
                symbol_id: 10
                ty_span: [55-58]
                ty_exprs: <empty>
                init_expr: Expr [63-68]:
                    ty: int
                    kind: Cast [63-68]:
                        ty: int
                        ty_exprs: <empty>
                        expr: Expr [63-68]:
                            ty: int[64]
                            kind: BinaryOpExpr:
                                op: Mul
                                lhs: Expr [63-64]:
                                    ty: int[64]
                                    kind: Cast [63-64]:
                                        ty: int[64]
                                        ty_exprs: <empty>
                                        expr: Expr [63-64]:
                                            ty: int[32]
                                            kind: SymbolId(8)
                                        kind: Implicit
                                rhs: Expr [67-68]:
                                    ty: int[64]
                                    kind: SymbolId(9)
                        kind: Implicit
        "#]],
    );
}

#[test]
fn multiplying_int_idents_with_width_greater_than_64_result_in_bigint_result() {
    let input = "
        int[32] x = 5;
        int[64] y = 3;
        int[67] z = x * y;
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [21-22]:
                    ty: const int[32]
                    kind: Lit: Int(5)
            ClassicalDeclarationStmt [32-46]:
                symbol_id: 9
                ty_span: [32-39]
                ty_exprs:
                    Expr [36-38]:
                        ty: const uint
                        const_value: Int(64)
                        kind: Lit: Int(64)
                init_expr: Expr [44-45]:
                    ty: const int[64]
                    kind: Lit: Int(3)
            ClassicalDeclarationStmt [55-73]:
                symbol_id: 10
                ty_span: [55-62]
                ty_exprs:
                    Expr [59-61]:
                        ty: const uint
                        const_value: Int(67)
                        kind: Lit: Int(67)
                init_expr: Expr [67-72]:
                    ty: int[67]
                    kind: Cast [67-72]:
                        ty: int[67]
                        ty_exprs: <empty>
                        expr: Expr [67-72]:
                            ty: int[64]
                            kind: BinaryOpExpr:
                                op: Mul
                                lhs: Expr [67-68]:
                                    ty: int[64]
                                    kind: Cast [67-68]:
                                        ty: int[64]
                                        ty_exprs: <empty>
                                        expr: Expr [67-68]:
                                            ty: int[32]
                                            kind: SymbolId(8)
                                        kind: Implicit
                                rhs: Expr [71-72]:
                                    ty: int[64]
                                    kind: SymbolId(9)
                        kind: Implicit
        "#]],
    );
}

#[test]
fn left_shift_casts_rhs_to_uint() {
    let input = "
        int x = 5;
        int y = 3;
        int z = x << y;
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
            ClassicalDeclarationStmt [47-62]:
                symbol_id: 10
                ty_span: [47-50]
                ty_exprs: <empty>
                init_expr: Expr [55-61]:
                    ty: int
                    kind: BinaryOpExpr:
                        op: Shl
                        lhs: Expr [55-56]:
                            ty: int
                            kind: SymbolId(8)
                        rhs: Expr [60-61]:
                            ty: uint
                            kind: Cast [60-61]:
                                ty: uint
                                ty_exprs: <empty>
                                expr: Expr [60-61]:
                                    ty: int
                                    kind: SymbolId(9)
                                kind: Implicit
        "#]],
    );
}

#[test]
fn bin_op_with_const_lhs_and_non_const_rhs() {
    let source = "
        int x = 5;
        int y = 2 * x;
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-18]:
                    ty: int
                    kind: Lit: Int(5)
            ClassicalDeclarationStmt [28-42]:
                symbol_id: 9
                ty_span: [28-31]
                ty_exprs: <empty>
                init_expr: Expr [36-41]:
                    ty: int
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [36-37]:
                            ty: const int
                            kind: Lit: Int(2)
                        rhs: Expr [40-41]:
                            ty: int
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bin_op_with_const_lhs_and_non_const_rhs_sized() {
    let source = "
        int[32] x = 5;
        int[32] y = 2 * x;
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [21-22]:
                    ty: const int[32]
                    kind: Lit: Int(5)
            ClassicalDeclarationStmt [32-50]:
                symbol_id: 9
                ty_span: [32-39]
                ty_exprs:
                    Expr [36-38]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [44-49]:
                    ty: int[32]
                    kind: Cast [44-49]:
                        ty: int[32]
                        ty_exprs: <empty>
                        expr: Expr [44-49]:
                            ty: int
                            kind: BinaryOpExpr:
                                op: Mul
                                lhs: Expr [44-45]:
                                    ty: const int
                                    kind: Lit: Int(2)
                                rhs: Expr [48-49]:
                                    ty: int
                                    kind: Cast [48-49]:
                                        ty: int
                                        ty_exprs: <empty>
                                        expr: Expr [48-49]:
                                            ty: int[32]
                                            kind: SymbolId(8)
                                        kind: Implicit
                        kind: Implicit
        "#]],
    );
}
