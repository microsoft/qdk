// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decls;

#[test]
fn to_bit_implicitly() {
    let input = "
        float x = 42.;
        bit y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-42]:
                symbol_id: 9
                ty_span: [32-35]
                ty_exprs: <empty>
                init_expr: Expr [40-41]:
                    ty: bit
                    kind: Cast [40-41]:
                        ty: bit
                        ty_exprs: <empty>
                        expr: Expr [40-41]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [36-37]:
                name: y
                type: bit
                ty_span: [32-35]
                io_kind: Default
        "#]],
    );
}

#[test]
fn explicit_width_to_bit_implicitly() {
    let input = "
        float[64] x = 42.;
        bit y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-27]:
                symbol_id: 8
                ty_span: [9-18]
                ty_exprs:
                    Expr [15-17]:
                        ty: const uint
                        const_value: Int(64)
                        kind: Lit: Int(64)
                init_expr: Expr [23-26]:
                    ty: const float[64]
                    kind: Lit: Float(42.0)
            [8] Symbol [19-20]:
                name: x
                type: float[64]
                ty_span: [9-18]
                io_kind: Default
            ClassicalDeclarationStmt [36-46]:
                symbol_id: 9
                ty_span: [36-39]
                ty_exprs: <empty>
                init_expr: Expr [44-45]:
                    ty: bit
                    kind: Cast [44-45]:
                        ty: bit
                        ty_exprs: <empty>
                        expr: Expr [44-45]:
                            ty: float[64]
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [40-41]:
                name: y
                type: bit
                ty_span: [36-39]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_bool_implicitly() {
    let input = "
        float x = 42.;
        bool y = x;
    ";
    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-43]:
                symbol_id: 9
                ty_span: [32-36]
                ty_exprs: <empty>
                init_expr: Expr [41-42]:
                    ty: bool
                    kind: Cast [41-42]:
                        ty: bool
                        ty_exprs: <empty>
                        expr: Expr [41-42]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [37-38]:
                name: y
                type: bool
                ty_span: [32-36]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_implicit_int_implicitly() {
    let input = "
        float x = 42.;
        int y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-42]:
                symbol_id: 9
                ty_span: [32-35]
                ty_exprs: <empty>
                init_expr: Expr [40-41]:
                    ty: int
                    kind: Cast [40-41]:
                        ty: int
                        ty_exprs: <empty>
                        expr: Expr [40-41]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [36-37]:
                name: y
                type: int
                ty_span: [32-35]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_int_implicitly() {
    let input = "
        float x = 42.;
        int[32] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-46]:
                symbol_id: 9
                ty_span: [32-39]
                ty_exprs:
                    Expr [36-38]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [44-45]:
                    ty: int[32]
                    kind: Cast [44-45]:
                        ty: int[32]
                        ty_exprs: <empty>
                        expr: Expr [44-45]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [40-41]:
                name: y
                type: int[32]
                ty_span: [32-39]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_implicit_uint_implicitly() {
    let input = "
        float x = 42.;
        uint y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-43]:
                symbol_id: 9
                ty_span: [32-36]
                ty_exprs: <empty>
                init_expr: Expr [41-42]:
                    ty: uint
                    kind: Cast [41-42]:
                        ty: uint
                        ty_exprs: <empty>
                        expr: Expr [41-42]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [37-38]:
                name: y
                type: uint
                ty_span: [32-36]
                io_kind: Default
        "#]],
    );
}

#[test]
fn negative_lit_to_implicit_uint_implicitly() {
    let input = "
        float x = -42.;
        uint y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-24]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [20-23]:
                    ty: float
                    kind: UnaryOpExpr [20-23]:
                        op: Neg
                        expr: Expr [20-23]:
                            ty: const float
                            kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [33-44]:
                symbol_id: 9
                ty_span: [33-37]
                ty_exprs: <empty>
                init_expr: Expr [42-43]:
                    ty: uint
                    kind: Cast [42-43]:
                        ty: uint
                        ty_exprs: <empty>
                        expr: Expr [42-43]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [38-39]:
                name: y
                type: uint
                ty_span: [33-37]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_uint_implicitly() {
    let input = "
        float x = 42.;
        uint[32] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-47]:
                symbol_id: 9
                ty_span: [32-40]
                ty_exprs:
                    Expr [37-39]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [45-46]:
                    ty: uint[32]
                    kind: Cast [45-46]:
                        ty: uint[32]
                        ty_exprs: <empty>
                        expr: Expr [45-46]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [41-42]:
                name: y
                type: uint[32]
                ty_span: [32-40]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_bigint_implicitly() {
    let input = "
        float x = 42.;
        int[65] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-46]:
                symbol_id: 9
                ty_span: [32-39]
                ty_exprs:
                    Expr [36-38]:
                        ty: const uint
                        const_value: Int(65)
                        kind: Lit: Int(65)
                init_expr: Expr [44-45]:
                    ty: int[65]
                    kind: Cast [44-45]:
                        ty: int[65]
                        ty_exprs: <empty>
                        expr: Expr [44-45]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [40-41]:
                name: y
                type: int[65]
                ty_span: [32-39]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_implicit_float_implicitly() {
    let input = "
        float x = 42.;
        float y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-44]:
                symbol_id: 9
                ty_span: [32-37]
                ty_exprs: <empty>
                init_expr: Expr [42-43]:
                    ty: float
                    kind: SymbolId(8)
            [9] Symbol [38-39]:
                name: y
                type: float
                ty_span: [32-37]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_float_implicitly() {
    let input = "
        float x = 42.;
        float[32] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-48]:
                symbol_id: 9
                ty_span: [32-41]
                ty_exprs:
                    Expr [38-40]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [46-47]:
                    ty: float[32]
                    kind: Cast [46-47]:
                        ty: float[32]
                        ty_exprs: <empty>
                        expr: Expr [46-47]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [42-43]:
                name: y
                type: float[32]
                ty_span: [32-41]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_implicit_complex_implicitly() {
    let input = "
        float x = 42.;
        complex[float] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-53]:
                symbol_id: 9
                ty_span: [32-46]
                ty_exprs: <empty>
                init_expr: Expr [51-52]:
                    ty: complex[float]
                    kind: Cast [51-52]:
                        ty: complex[float]
                        ty_exprs: <empty>
                        expr: Expr [51-52]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [47-48]:
                name: y
                type: complex[float]
                ty_span: [32-46]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_complex_implicitly() {
    let input = "
        float x = 42.;
        complex[float[32]] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-57]:
                symbol_id: 9
                ty_span: [32-50]
                ty_exprs:
                    Expr [46-48]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [55-56]:
                    ty: complex[float[32]]
                    kind: Cast [55-56]:
                        ty: complex[float[32]]
                        ty_exprs: <empty>
                        expr: Expr [55-56]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [51-52]:
                name: y
                type: complex[float[32]]
                ty_span: [32-50]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_angle_implicitly() {
    let input = "
        float x = 42.;
        angle y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-44]:
                symbol_id: 9
                ty_span: [32-37]
                ty_exprs: <empty>
                init_expr: Expr [42-43]:
                    ty: angle
                    kind: Cast [42-43]:
                        ty: angle
                        ty_exprs: <empty>
                        expr: Expr [42-43]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [38-39]:
                name: y
                type: angle
                ty_span: [32-37]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_angle_implicitly() {
    let input = "
        float x = 42.;
        angle[4] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: float
                    kind: Lit: Float(42.0)
            [8] Symbol [15-16]:
                name: x
                type: float
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-47]:
                symbol_id: 9
                ty_span: [32-40]
                ty_exprs:
                    Expr [38-39]:
                        ty: const uint
                        const_value: Int(4)
                        kind: Lit: Int(4)
                init_expr: Expr [45-46]:
                    ty: angle[4]
                    kind: Cast [45-46]:
                        ty: angle[4]
                        ty_exprs: <empty>
                        expr: Expr [45-46]:
                            ty: float
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [41-42]:
                name: y
                type: angle[4]
                ty_span: [32-40]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_angle_implicitly_in_add_bin_op() {
    let source = "
        angle a = pi;
        angle b = 1.0 + a;
        angle c = a + 2.0;
    ";

    check_classical_decls(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-22]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-21]:
                    ty: angle
                    kind: Cast [19-21]:
                        ty: angle
                        ty_exprs: <empty>
                        expr: Expr [19-21]:
                            ty: const float
                            kind: SymbolId(2)
                        kind: Implicit
            [8] Symbol [15-16]:
                name: a
                type: angle
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [31-49]:
                symbol_id: 9
                ty_span: [31-36]
                ty_exprs: <empty>
                init_expr: Expr [41-48]:
                    ty: angle
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [41-44]:
                            ty: angle
                            kind: Cast [41-44]:
                                ty: angle
                                ty_exprs: <empty>
                                expr: Expr [41-44]:
                                    ty: const float
                                    kind: Lit: Float(1.0)
                                kind: Implicit
                        rhs: Expr [47-48]:
                            ty: angle
                            kind: SymbolId(8)
            [9] Symbol [37-38]:
                name: b
                type: angle
                ty_span: [31-36]
                io_kind: Default
            ClassicalDeclarationStmt [58-76]:
                symbol_id: 10
                ty_span: [58-63]
                ty_exprs: <empty>
                init_expr: Expr [68-75]:
                    ty: angle
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [68-69]:
                            ty: angle
                            kind: SymbolId(8)
                        rhs: Expr [72-75]:
                            ty: angle
                            kind: Cast [72-75]:
                                ty: angle
                                ty_exprs: <empty>
                                expr: Expr [72-75]:
                                    ty: const float
                                    kind: Lit: Float(2.0)
                                kind: Implicit
            [10] Symbol [64-65]:
                name: c
                type: angle
                ty_span: [58-63]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_const_angle_implicitly_in_add_bin_op() {
    let source = "
        const angle a = pi;
        const angle b = 1.0 + a;
        const angle c = a + 2.0;
    ";

    check_classical_decls(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-28]:
                symbol_id: 8
                ty_span: [15-20]
                ty_exprs: <empty>
                init_expr: Expr [25-27]:
                    ty: const angle
                    const_value: Angle(3.141592653589793)
                    kind: Cast [25-27]:
                        ty: const angle
                        ty_exprs: <empty>
                        expr: Expr [25-27]:
                            ty: const float
                            kind: SymbolId(2)
                        kind: Implicit
            [8] Symbol [21-22]:
                name: a
                type: const angle
                ty_span: [15-20]
                io_kind: Default
            ClassicalDeclarationStmt [37-61]:
                symbol_id: 9
                ty_span: [43-48]
                ty_exprs: <empty>
                init_expr: Expr [53-60]:
                    ty: const angle
                    const_value: Angle(4.141592653589793)
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [53-56]:
                            ty: const angle
                            kind: Cast [53-56]:
                                ty: const angle
                                ty_exprs: <empty>
                                expr: Expr [53-56]:
                                    ty: const float
                                    kind: Lit: Float(1.0)
                                kind: Implicit
                        rhs: Expr [59-60]:
                            ty: const angle
                            kind: SymbolId(8)
            [9] Symbol [49-50]:
                name: b
                type: const angle
                ty_span: [43-48]
                io_kind: Default
            ClassicalDeclarationStmt [70-94]:
                symbol_id: 10
                ty_span: [76-81]
                ty_exprs: <empty>
                init_expr: Expr [86-93]:
                    ty: const angle
                    const_value: Angle(5.141592653589793)
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [86-87]:
                            ty: const angle
                            kind: SymbolId(8)
                        rhs: Expr [90-93]:
                            ty: const angle
                            kind: Cast [90-93]:
                                ty: const angle
                                ty_exprs: <empty>
                                expr: Expr [90-93]:
                                    ty: const float
                                    kind: Lit: Float(2.0)
                                kind: Implicit
            [10] Symbol [82-83]:
                name: c
                type: const angle
                ty_span: [76-81]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_angle_implicitly_in_sub_bin_op() {
    let source = "
        angle a = pi;
        angle b = 1.0 + a;
        angle c = a + 2.0;
    ";

    check_classical_decls(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-22]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-21]:
                    ty: angle
                    kind: Cast [19-21]:
                        ty: angle
                        ty_exprs: <empty>
                        expr: Expr [19-21]:
                            ty: const float
                            kind: SymbolId(2)
                        kind: Implicit
            [8] Symbol [15-16]:
                name: a
                type: angle
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [31-49]:
                symbol_id: 9
                ty_span: [31-36]
                ty_exprs: <empty>
                init_expr: Expr [41-48]:
                    ty: angle
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [41-44]:
                            ty: angle
                            kind: Cast [41-44]:
                                ty: angle
                                ty_exprs: <empty>
                                expr: Expr [41-44]:
                                    ty: const float
                                    kind: Lit: Float(1.0)
                                kind: Implicit
                        rhs: Expr [47-48]:
                            ty: angle
                            kind: SymbolId(8)
            [9] Symbol [37-38]:
                name: b
                type: angle
                ty_span: [31-36]
                io_kind: Default
            ClassicalDeclarationStmt [58-76]:
                symbol_id: 10
                ty_span: [58-63]
                ty_exprs: <empty>
                init_expr: Expr [68-75]:
                    ty: angle
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [68-69]:
                            ty: angle
                            kind: SymbolId(8)
                        rhs: Expr [72-75]:
                            ty: angle
                            kind: Cast [72-75]:
                                ty: angle
                                ty_exprs: <empty>
                                expr: Expr [72-75]:
                                    ty: const float
                                    kind: Lit: Float(2.0)
                                kind: Implicit
            [10] Symbol [64-65]:
                name: c
                type: angle
                ty_span: [58-63]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_const_angle_implicitly_in_sub_bin_op() {
    let source = "
        const angle a = pi;
        const angle b = 1.0 + a;
        const angle c = a + 2.0;
    ";

    check_classical_decls(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-28]:
                symbol_id: 8
                ty_span: [15-20]
                ty_exprs: <empty>
                init_expr: Expr [25-27]:
                    ty: const angle
                    const_value: Angle(3.141592653589793)
                    kind: Cast [25-27]:
                        ty: const angle
                        ty_exprs: <empty>
                        expr: Expr [25-27]:
                            ty: const float
                            kind: SymbolId(2)
                        kind: Implicit
            [8] Symbol [21-22]:
                name: a
                type: const angle
                ty_span: [15-20]
                io_kind: Default
            ClassicalDeclarationStmt [37-61]:
                symbol_id: 9
                ty_span: [43-48]
                ty_exprs: <empty>
                init_expr: Expr [53-60]:
                    ty: const angle
                    const_value: Angle(4.141592653589793)
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [53-56]:
                            ty: const angle
                            kind: Cast [53-56]:
                                ty: const angle
                                ty_exprs: <empty>
                                expr: Expr [53-56]:
                                    ty: const float
                                    kind: Lit: Float(1.0)
                                kind: Implicit
                        rhs: Expr [59-60]:
                            ty: const angle
                            kind: SymbolId(8)
            [9] Symbol [49-50]:
                name: b
                type: const angle
                ty_span: [43-48]
                io_kind: Default
            ClassicalDeclarationStmt [70-94]:
                symbol_id: 10
                ty_span: [76-81]
                ty_exprs: <empty>
                init_expr: Expr [86-93]:
                    ty: const angle
                    const_value: Angle(5.141592653589793)
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [86-87]:
                            ty: const angle
                            kind: SymbolId(8)
                        rhs: Expr [90-93]:
                            ty: const angle
                            kind: Cast [90-93]:
                                ty: const angle
                                ty_exprs: <empty>
                                expr: Expr [90-93]:
                                    ty: const float
                                    kind: Lit: Float(2.0)
                                kind: Implicit
            [10] Symbol [82-83]:
                name: c
                type: const angle
                ty_span: [76-81]
                io_kind: Default
        "#]],
    );
}
