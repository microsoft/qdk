// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decls;

#[test]
fn to_bit_implicitly() {
    let input = "
        bool x = true;
        bit y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                qsharp_type: bool
                io_kind: Default
            ClassicalDeclarationStmt [32-42]:
                symbol_id: 9
                ty_span: [32-35]
                init_expr: Expr [40-41]:
                    ty: bit
                    kind: Cast [0-0]:
                        ty: bit
                        expr: Expr [40-41]:
                            ty: bool
                            kind: SymbolId(8)
            [9] Symbol [36-37]:
                name: y
                type: bit
                qsharp_type: Result
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_implicit_int_implicitly() {
    let input = "
        bool x = true;
        int y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                qsharp_type: bool
                io_kind: Default
            ClassicalDeclarationStmt [32-42]:
                symbol_id: 9
                ty_span: [32-35]
                init_expr: Expr [40-41]:
                    ty: int
                    kind: Cast [0-0]:
                        ty: int
                        expr: Expr [40-41]:
                            ty: bool
                            kind: SymbolId(8)
            [9] Symbol [36-37]:
                name: y
                type: int
                qsharp_type: Int
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_int_implicitly() {
    let input = "
        bool x = true;
        int[32] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                qsharp_type: bool
                io_kind: Default
            ClassicalDeclarationStmt [32-46]:
                symbol_id: 9
                ty_span: [32-39]
                init_expr: Expr [44-45]:
                    ty: int[32]
                    kind: Cast [0-0]:
                        ty: int[32]
                        expr: Expr [44-45]:
                            ty: bool
                            kind: SymbolId(8)
            [9] Symbol [40-41]:
                name: y
                type: int[32]
                qsharp_type: Int
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_implicit_uint_implicitly() {
    let input = "
        bool x = true;
        uint y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                qsharp_type: bool
                io_kind: Default
            ClassicalDeclarationStmt [32-43]:
                symbol_id: 9
                ty_span: [32-36]
                init_expr: Expr [41-42]:
                    ty: uint
                    kind: Cast [0-0]:
                        ty: uint
                        expr: Expr [41-42]:
                            ty: bool
                            kind: SymbolId(8)
            [9] Symbol [37-38]:
                name: y
                type: uint
                qsharp_type: Int
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_uint_implicitly() {
    let input = "
        bool x = true;
        uint[32] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                qsharp_type: bool
                io_kind: Default
            ClassicalDeclarationStmt [32-47]:
                symbol_id: 9
                ty_span: [32-40]
                init_expr: Expr [45-46]:
                    ty: uint[32]
                    kind: Cast [0-0]:
                        ty: uint[32]
                        expr: Expr [45-46]:
                            ty: bool
                            kind: SymbolId(8)
            [9] Symbol [41-42]:
                name: y
                type: uint[32]
                qsharp_type: Int
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_bigint_implicitly() {
    let input = "
        bool x = true;
        int[65] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                qsharp_type: bool
                io_kind: Default
            ClassicalDeclarationStmt [32-46]:
                symbol_id: 9
                ty_span: [32-39]
                init_expr: Expr [44-45]:
                    ty: int[65]
                    kind: Cast [0-0]:
                        ty: int[65]
                        expr: Expr [44-45]:
                            ty: bool
                            kind: SymbolId(8)
            [9] Symbol [40-41]:
                name: y
                type: int[65]
                qsharp_type: BigInt
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_implicit_float_implicitly() {
    let input = "
        bool x = true;
        float y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                qsharp_type: bool
                io_kind: Default
            ClassicalDeclarationStmt [32-44]:
                symbol_id: 9
                ty_span: [32-37]
                init_expr: Expr [42-43]:
                    ty: float
                    kind: Cast [0-0]:
                        ty: float
                        expr: Expr [42-43]:
                            ty: bool
                            kind: SymbolId(8)
            [9] Symbol [38-39]:
                name: y
                type: float
                qsharp_type: Double
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_float_implicitly() {
    let input = "
        bool x = true;
        float[32] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-13]
                init_expr: Expr [18-22]:
                    ty: bool
                    kind: Lit: Bool(true)
            [8] Symbol [14-15]:
                name: x
                type: bool
                qsharp_type: bool
                io_kind: Default
            ClassicalDeclarationStmt [32-48]:
                symbol_id: 9
                ty_span: [32-41]
                init_expr: Expr [46-47]:
                    ty: float[32]
                    kind: Cast [0-0]:
                        ty: float[32]
                        expr: Expr [46-47]:
                            ty: bool
                            kind: SymbolId(8)
            [9] Symbol [42-43]:
                name: y
                type: float[32]
                qsharp_type: Double
                io_kind: Default
        "#]],
    );
}
