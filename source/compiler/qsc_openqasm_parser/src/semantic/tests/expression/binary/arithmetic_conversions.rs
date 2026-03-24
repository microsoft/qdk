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

#[test]
fn int_add_bitarray_promotes_to_int() {
    let source = r#"
        int a = 5;
        bit[4] b = "1010";
        a + b;
    "#;

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
            ClassicalDeclarationStmt [28-46]:
                symbol_id: 9
                ty_span: [28-34]
                ty_exprs:
                    Expr [32-33]:
                        ty: const uint
                        const_value: Int(4)
                        kind: Lit: Int(4)
                init_expr: Expr [39-45]:
                    ty: bit[4]
                    kind: Lit: Bitstring("1010")
            ExprStmt [55-61]:
                expr: Expr [55-60]:
                    ty: int
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [55-56]:
                            ty: int
                            kind: SymbolId(8)
                        rhs: Expr [59-60]:
                            ty: int
                            kind: Cast [59-60]:
                                ty: int
                                ty_exprs: <empty>
                                expr: Expr [59-60]:
                                    ty: bit[4]
                                    kind: SymbolId(9)
                                kind: Implicit
        "#]],
    );
}

#[test]
fn bitarray_add_int_promotes_to_int() {
    let source = r#"
        bit[4] a = "1010";
        int b = 5;
        a + b;
    "#;

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-27]:
                symbol_id: 8
                ty_span: [9-15]
                ty_exprs:
                    Expr [13-14]:
                        ty: const uint
                        const_value: Int(4)
                        kind: Lit: Int(4)
                init_expr: Expr [20-26]:
                    ty: bit[4]
                    kind: Lit: Bitstring("1010")
            ClassicalDeclarationStmt [36-46]:
                symbol_id: 9
                ty_span: [36-39]
                ty_exprs: <empty>
                init_expr: Expr [44-45]:
                    ty: int
                    kind: Lit: Int(5)
            ExprStmt [55-61]:
                expr: Expr [55-60]:
                    ty: int
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [55-56]:
                            ty: int
                            kind: Cast [55-56]:
                                ty: int
                                ty_exprs: <empty>
                                expr: Expr [55-56]:
                                    ty: bit[4]
                                    kind: SymbolId(8)
                                kind: Implicit
                        rhs: Expr [59-60]:
                            ty: int
                            kind: SymbolId(9)
        "#]],
    );
}

#[test]
fn const_int_add_non_const_bitarray_result_is_non_const() {
    let source = r#"
        const int a = 5;
        bit[4] b = "1010";
        a + b;
    "#;

    check_stmt_kinds(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-25]:
                symbol_id: 8
                ty_span: [15-18]
                ty_exprs: <empty>
                init_expr: Expr [23-24]:
                    ty: const int
                    const_value: Int(5)
                    kind: Lit: Int(5)
            ClassicalDeclarationStmt [34-52]:
                symbol_id: 9
                ty_span: [34-40]
                ty_exprs:
                    Expr [38-39]:
                        ty: const uint
                        const_value: Int(4)
                        kind: Lit: Int(4)
                init_expr: Expr [45-51]:
                    ty: bit[4]
                    kind: Lit: Bitstring("1010")
            ExprStmt [61-67]:
                expr: Expr [61-66]:
                    ty: int
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [61-62]:
                            ty: int
                            kind: SymbolId(8)
                        rhs: Expr [65-66]:
                            ty: int
                            kind: Cast [65-66]:
                                ty: int
                                ty_exprs: <empty>
                                expr: Expr [65-66]:
                                    ty: bit[4]
                                    kind: SymbolId(9)
                                kind: Implicit
        "#]],
    );
}
