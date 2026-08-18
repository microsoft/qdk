// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decls;

#[test]
fn to_bit_implicitly() {
    let input = "
        int x = 42;
        bit y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: int
                    kind: Lit: Int(42)
            [8] Symbol [13-14]:
                name: x
                type: int
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [29-39]:
                symbol_id: 9
                ty_span: [29-32]
                ty_exprs: <empty>
                init_expr: Expr [37-38]:
                    ty: bit
                    kind: Cast [37-38]:
                        ty: bit
                        ty_exprs: <empty>
                        expr: Expr [37-38]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [33-34]:
                name: y
                type: bit
                ty_span: [29-32]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_bool_implicitly() {
    let input = "
        int x = 42;
        bool y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: int
                    kind: Lit: Int(42)
            [8] Symbol [13-14]:
                name: x
                type: int
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [29-40]:
                symbol_id: 9
                ty_span: [29-33]
                ty_exprs: <empty>
                init_expr: Expr [38-39]:
                    ty: bool
                    kind: Cast [38-39]:
                        ty: bool
                        ty_exprs: <empty>
                        expr: Expr [38-39]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [34-35]:
                name: y
                type: bool
                ty_span: [29-33]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_implicit_int_implicitly() {
    let input = "
        int x = 42;
        int y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: int
                    kind: Lit: Int(42)
            [8] Symbol [13-14]:
                name: x
                type: int
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [29-39]:
                symbol_id: 9
                ty_span: [29-32]
                ty_exprs: <empty>
                init_expr: Expr [37-38]:
                    ty: int
                    kind: SymbolId(8)
            [9] Symbol [33-34]:
                name: y
                type: int
                ty_span: [29-32]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_int_implicitly() {
    let input = "
        int x = 42;
        int[32] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: int
                    kind: Lit: Int(42)
            [8] Symbol [13-14]:
                name: x
                type: int
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [29-43]:
                symbol_id: 9
                ty_span: [29-36]
                ty_exprs:
                    Expr [33-35]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [41-42]:
                    ty: int[32]
                    kind: Cast [41-42]:
                        ty: int[32]
                        ty_exprs: <empty>
                        expr: Expr [41-42]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [37-38]:
                name: y
                type: int[32]
                ty_span: [29-36]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_implicit_uint_implicitly() {
    let input = "
        int x = 42;
        uint y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: int
                    kind: Lit: Int(42)
            [8] Symbol [13-14]:
                name: x
                type: int
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [29-40]:
                symbol_id: 9
                ty_span: [29-33]
                ty_exprs: <empty>
                init_expr: Expr [38-39]:
                    ty: uint
                    kind: Cast [38-39]:
                        ty: uint
                        ty_exprs: <empty>
                        expr: Expr [38-39]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [34-35]:
                name: y
                type: uint
                ty_span: [29-33]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_uint_implicitly() {
    let input = "
        int x = 42;
        uint[32] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: int
                    kind: Lit: Int(42)
            [8] Symbol [13-14]:
                name: x
                type: int
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [29-44]:
                symbol_id: 9
                ty_span: [29-37]
                ty_exprs:
                    Expr [34-36]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [42-43]:
                    ty: uint[32]
                    kind: Cast [42-43]:
                        ty: uint[32]
                        ty_exprs: <empty>
                        expr: Expr [42-43]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [38-39]:
                name: y
                type: uint[32]
                ty_span: [29-37]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_bigint_implicitly() {
    let input = "
        int x = 42;
        int[65] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: int
                    kind: Lit: Int(42)
            [8] Symbol [13-14]:
                name: x
                type: int
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [29-43]:
                symbol_id: 9
                ty_span: [29-36]
                ty_exprs:
                    Expr [33-35]:
                        ty: const uint
                        const_value: Int(65)
                        kind: Lit: Int(65)
                init_expr: Expr [41-42]:
                    ty: int[65]
                    kind: Cast [41-42]:
                        ty: int[65]
                        ty_exprs: <empty>
                        expr: Expr [41-42]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [37-38]:
                name: y
                type: int[65]
                ty_span: [29-36]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_implicit_float_implicitly() {
    let input = "
        int x = 42;
        float y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: int
                    kind: Lit: Int(42)
            [8] Symbol [13-14]:
                name: x
                type: int
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [29-41]:
                symbol_id: 9
                ty_span: [29-34]
                ty_exprs: <empty>
                init_expr: Expr [39-40]:
                    ty: float
                    kind: Cast [39-40]:
                        ty: float
                        ty_exprs: <empty>
                        expr: Expr [39-40]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [35-36]:
                name: y
                type: float
                ty_span: [29-34]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_float_implicitly() {
    let input = "
        int x = 42;
        float[32] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: int
                    kind: Lit: Int(42)
            [8] Symbol [13-14]:
                name: x
                type: int
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [29-45]:
                symbol_id: 9
                ty_span: [29-38]
                ty_exprs:
                    Expr [35-37]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [43-44]:
                    ty: float[32]
                    kind: Cast [43-44]:
                        ty: float[32]
                        ty_exprs: <empty>
                        expr: Expr [43-44]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [39-40]:
                name: y
                type: float[32]
                ty_span: [29-38]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_implicit_complex_implicitly() {
    let input = "
        int x = 42;
        complex[float] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: int
                    kind: Lit: Int(42)
            [8] Symbol [13-14]:
                name: x
                type: int
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [29-50]:
                symbol_id: 9
                ty_span: [29-43]
                ty_exprs: <empty>
                init_expr: Expr [48-49]:
                    ty: complex[float]
                    kind: Cast [48-49]:
                        ty: complex[float]
                        ty_exprs: <empty>
                        expr: Expr [48-49]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [44-45]:
                name: y
                type: complex[float]
                ty_span: [29-43]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_complex_implicitly() {
    let input = "
        int x = 42;
        complex[float[32]] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: int
                    kind: Lit: Int(42)
            [8] Symbol [13-14]:
                name: x
                type: int
                ty_span: [9-12]
                io_kind: Default
            ClassicalDeclarationStmt [29-54]:
                symbol_id: 9
                ty_span: [29-47]
                ty_exprs:
                    Expr [43-45]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [52-53]:
                    ty: complex[float[32]]
                    kind: Cast [52-53]:
                        ty: complex[float[32]]
                        ty_exprs: <empty>
                        expr: Expr [52-53]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [48-49]:
                name: y
                type: complex[float[32]]
                ty_span: [29-47]
                io_kind: Default
        "#]],
    );
}
