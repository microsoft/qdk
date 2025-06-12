// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod bit_to_bit;
mod bool_to_bool;
mod float_to_float;
mod int_to_int;

mod uint_to_uint;

use expect_test::expect;

use crate::semantic::tests::check_stmt_kinds;

#[allow(clippy::too_many_lines)]
#[test]
fn bitarray_var_comparisons_can_be_translated() {
    let input = r#"
        bit[1] x = "1";
        bit[1] y = "0";
        bool f = x > y;
        bool e = x >= y;
        bool a = x < y;
        bool c = x <= y;
        bool b = x == y;
        bool d = x != y;
    "#;

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-24]:
                symbol_id: 8
                ty_span: [9-15]
                init_expr: Expr [20-23]:
                    ty: bit[1]
                    kind: Lit: Bitstring("1")
            ClassicalDeclarationStmt [33-48]:
                symbol_id: 9
                ty_span: [33-39]
                init_expr: Expr [44-47]:
                    ty: bit[1]
                    kind: Lit: Bitstring("0")
            ClassicalDeclarationStmt [57-72]:
                symbol_id: 10
                ty_span: [57-61]
                init_expr: Expr [66-71]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Gt
                        lhs: Expr [66-67]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [66-67]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
                        rhs: Expr [70-71]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [70-71]:
                                    ty: bit[1]
                                    kind: SymbolId(9)
            ClassicalDeclarationStmt [81-97]:
                symbol_id: 11
                ty_span: [81-85]
                init_expr: Expr [90-96]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Gte
                        lhs: Expr [90-91]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [90-91]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
                        rhs: Expr [95-96]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [95-96]:
                                    ty: bit[1]
                                    kind: SymbolId(9)
            ClassicalDeclarationStmt [106-121]:
                symbol_id: 12
                ty_span: [106-110]
                init_expr: Expr [115-120]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Lt
                        lhs: Expr [115-116]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [115-116]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
                        rhs: Expr [119-120]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [119-120]:
                                    ty: bit[1]
                                    kind: SymbolId(9)
            ClassicalDeclarationStmt [130-146]:
                symbol_id: 13
                ty_span: [130-134]
                init_expr: Expr [139-145]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Lte
                        lhs: Expr [139-140]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [139-140]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
                        rhs: Expr [144-145]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [144-145]:
                                    ty: bit[1]
                                    kind: SymbolId(9)
            ClassicalDeclarationStmt [155-171]:
                symbol_id: 14
                ty_span: [155-159]
                init_expr: Expr [164-170]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Eq
                        lhs: Expr [164-165]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [164-165]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
                        rhs: Expr [169-170]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [169-170]:
                                    ty: bit[1]
                                    kind: SymbolId(9)
            ClassicalDeclarationStmt [180-196]:
                symbol_id: 15
                ty_span: [180-184]
                init_expr: Expr [189-195]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Neq
                        lhs: Expr [189-190]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [189-190]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
                        rhs: Expr [194-195]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [194-195]:
                                    ty: bit[1]
                                    kind: SymbolId(9)
        "#]],
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn bitarray_var_comparison_to_int_can_be_translated() {
    let input = r#"
        bit[1] x = "1";
        input int y;
        bool a = x > y;
        bool b = x >= y;
        bool c = x < y;
        bool d = x <= y;
        bool e = x == y;
        bool f = x != y;
        bool g = y > x;
        bool h = y >= x;
        bool i = y < x;
        bool j = y <= x;
        bool k = y == x;
        bool l = y != x;
    "#;

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-24]:
                symbol_id: 8
                ty_span: [9-15]
                init_expr: Expr [20-23]:
                    ty: bit[1]
                    kind: Lit: Bitstring("1")
            InputDeclaration [33-45]:
                symbol_id: 9
            ClassicalDeclarationStmt [54-69]:
                symbol_id: 10
                ty_span: [54-58]
                init_expr: Expr [63-68]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Gt
                        lhs: Expr [63-64]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [63-64]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
                        rhs: Expr [67-68]:
                            ty: int
                            kind: SymbolId(9)
            ClassicalDeclarationStmt [78-94]:
                symbol_id: 11
                ty_span: [78-82]
                init_expr: Expr [87-93]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Gte
                        lhs: Expr [87-88]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [87-88]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
                        rhs: Expr [92-93]:
                            ty: int
                            kind: SymbolId(9)
            ClassicalDeclarationStmt [103-118]:
                symbol_id: 12
                ty_span: [103-107]
                init_expr: Expr [112-117]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Lt
                        lhs: Expr [112-113]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [112-113]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
                        rhs: Expr [116-117]:
                            ty: int
                            kind: SymbolId(9)
            ClassicalDeclarationStmt [127-143]:
                symbol_id: 13
                ty_span: [127-131]
                init_expr: Expr [136-142]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Lte
                        lhs: Expr [136-137]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [136-137]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
                        rhs: Expr [141-142]:
                            ty: int
                            kind: SymbolId(9)
            ClassicalDeclarationStmt [152-168]:
                symbol_id: 14
                ty_span: [152-156]
                init_expr: Expr [161-167]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Eq
                        lhs: Expr [161-162]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [161-162]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
                        rhs: Expr [166-167]:
                            ty: int
                            kind: SymbolId(9)
            ClassicalDeclarationStmt [177-193]:
                symbol_id: 15
                ty_span: [177-181]
                init_expr: Expr [186-192]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Neq
                        lhs: Expr [186-187]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [186-187]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
                        rhs: Expr [191-192]:
                            ty: int
                            kind: SymbolId(9)
            ClassicalDeclarationStmt [202-217]:
                symbol_id: 16
                ty_span: [202-206]
                init_expr: Expr [211-216]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Gt
                        lhs: Expr [211-212]:
                            ty: int
                            kind: SymbolId(9)
                        rhs: Expr [215-216]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [215-216]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
            ClassicalDeclarationStmt [226-242]:
                symbol_id: 17
                ty_span: [226-230]
                init_expr: Expr [235-241]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Gte
                        lhs: Expr [235-236]:
                            ty: int
                            kind: SymbolId(9)
                        rhs: Expr [240-241]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [240-241]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
            ClassicalDeclarationStmt [251-266]:
                symbol_id: 18
                ty_span: [251-255]
                init_expr: Expr [260-265]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Lt
                        lhs: Expr [260-261]:
                            ty: int
                            kind: SymbolId(9)
                        rhs: Expr [264-265]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [264-265]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
            ClassicalDeclarationStmt [275-291]:
                symbol_id: 19
                ty_span: [275-279]
                init_expr: Expr [284-290]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Lte
                        lhs: Expr [284-285]:
                            ty: int
                            kind: SymbolId(9)
                        rhs: Expr [289-290]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [289-290]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
            ClassicalDeclarationStmt [300-316]:
                symbol_id: 20
                ty_span: [300-304]
                init_expr: Expr [309-315]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Eq
                        lhs: Expr [309-310]:
                            ty: int
                            kind: SymbolId(9)
                        rhs: Expr [314-315]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [314-315]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
            ClassicalDeclarationStmt [325-341]:
                symbol_id: 21
                ty_span: [325-329]
                init_expr: Expr [334-340]:
                    ty: bool
                    kind: BinaryOpExpr:
                        op: Neq
                        lhs: Expr [334-335]:
                            ty: int
                            kind: SymbolId(9)
                        rhs: Expr [339-340]:
                            ty: int
                            kind: Cast [0-0]:
                                ty: int
                                expr: Expr [339-340]:
                                    ty: bit[1]
                                    kind: SymbolId(8)
        "#]],
    );
}
