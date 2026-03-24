// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decl;

#[test]
fn implicit_bitness_int_default() {
    check_classical_decl(
        "uint x;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-7]:
                symbol_id: 8
                ty_span: [0-4]
                ty_exprs: <empty>
                init_expr: Expr [0-7]:
                    ty: const uint
                    kind: Lit: Int(0)
            [8] Symbol [5-6]:
                name: x
                type: uint
                ty_span: [0-4]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_int_lit() {
    check_classical_decl(
        "const uint x = 42;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-18]:
                symbol_id: 8
                ty_span: [6-10]
                ty_exprs: <empty>
                init_expr: Expr [15-17]:
                    ty: const uint
                    const_value: Int(42)
                    kind: Lit: Int(42)
            [8] Symbol [11-12]:
                name: x
                type: const uint
                ty_span: [6-10]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_int_hex_cap() {
    check_classical_decl(
        "uint x = 0XFa_1F;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-17]:
                symbol_id: 8
                ty_span: [0-4]
                ty_exprs: <empty>
                init_expr: Expr [9-16]:
                    ty: const uint
                    kind: Lit: Int(64031)
            [8] Symbol [5-6]:
                name: x
                type: uint
                ty_span: [0-4]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_int_hex_low() {
    check_classical_decl(
        "const uint x = 0xFa_1F;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [6-10]
                ty_exprs: <empty>
                init_expr: Expr [15-22]:
                    ty: const uint
                    const_value: Int(64031)
                    kind: Lit: Int(64031)
            [8] Symbol [11-12]:
                name: x
                type: const uint
                ty_span: [6-10]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_int_hex_cap() {
    check_classical_decl(
        "const uint y = 0XFa_1F;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [6-10]
                ty_exprs: <empty>
                init_expr: Expr [15-22]:
                    ty: const uint
                    const_value: Int(64031)
                    kind: Lit: Int(64031)
            [8] Symbol [11-12]:
                name: y
                type: const uint
                ty_span: [6-10]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_int_octal_low() {
    check_classical_decl(
        "uint x = 0o42;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-14]:
                symbol_id: 8
                ty_span: [0-4]
                ty_exprs: <empty>
                init_expr: Expr [9-13]:
                    ty: const uint
                    kind: Lit: Int(34)
            [8] Symbol [5-6]:
                name: x
                type: uint
                ty_span: [0-4]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_int_octal_cap() {
    check_classical_decl(
        "uint x = 0O42;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-14]:
                symbol_id: 8
                ty_span: [0-4]
                ty_exprs: <empty>
                init_expr: Expr [9-13]:
                    ty: const uint
                    kind: Lit: Int(34)
            [8] Symbol [5-6]:
                name: x
                type: uint
                ty_span: [0-4]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_int_octal_low() {
    check_classical_decl(
        "const uint x = 0o42;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-20]:
                symbol_id: 8
                ty_span: [6-10]
                ty_exprs: <empty>
                init_expr: Expr [15-19]:
                    ty: const uint
                    const_value: Int(34)
                    kind: Lit: Int(34)
            [8] Symbol [11-12]:
                name: x
                type: const uint
                ty_span: [6-10]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_int_octal_cap() {
    check_classical_decl(
        "const uint x = 0O42;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-20]:
                symbol_id: 8
                ty_span: [6-10]
                ty_exprs: <empty>
                init_expr: Expr [15-19]:
                    ty: const uint
                    const_value: Int(34)
                    kind: Lit: Int(34)
            [8] Symbol [11-12]:
                name: x
                type: const uint
                ty_span: [6-10]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_int_binary_low() {
    check_classical_decl(
        "uint x = 0b1001_1001;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-21]:
                symbol_id: 8
                ty_span: [0-4]
                ty_exprs: <empty>
                init_expr: Expr [9-20]:
                    ty: const uint
                    kind: Lit: Int(153)
            [8] Symbol [5-6]:
                name: x
                type: uint
                ty_span: [0-4]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_int_binary_cap() {
    check_classical_decl(
        "uint x = 0B1010;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-16]:
                symbol_id: 8
                ty_span: [0-4]
                ty_exprs: <empty>
                init_expr: Expr [9-15]:
                    ty: const uint
                    kind: Lit: Int(10)
            [8] Symbol [5-6]:
                name: x
                type: uint
                ty_span: [0-4]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_int_binary_low() {
    check_classical_decl(
        "const uint x = 0b1001_1001;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-27]:
                symbol_id: 8
                ty_span: [6-10]
                ty_exprs: <empty>
                init_expr: Expr [15-26]:
                    ty: const uint
                    const_value: Int(153)
                    kind: Lit: Int(153)
            [8] Symbol [11-12]:
                name: x
                type: const uint
                ty_span: [6-10]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_int_binary_cap() {
    check_classical_decl(
        "const uint x = 0B1010;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-22]:
                symbol_id: 8
                ty_span: [6-10]
                ty_exprs: <empty>
                init_expr: Expr [15-21]:
                    ty: const uint
                    const_value: Int(10)
                    kind: Lit: Int(10)
            [8] Symbol [11-12]:
                name: x
                type: const uint
                ty_span: [6-10]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_int_formatted() {
    check_classical_decl(
        "uint x = 2_0_00;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-16]:
                symbol_id: 8
                ty_span: [0-4]
                ty_exprs: <empty>
                init_expr: Expr [9-15]:
                    ty: const uint
                    kind: Lit: Int(2000)
            [8] Symbol [5-6]:
                name: x
                type: uint
                ty_span: [0-4]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_int_formatted() {
    check_classical_decl(
        "const uint x = 2_0_00;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-22]:
                symbol_id: 8
                ty_span: [6-10]
                ty_exprs: <empty>
                init_expr: Expr [15-21]:
                    ty: const uint
                    const_value: Int(2000)
                    kind: Lit: Int(2000)
            [8] Symbol [11-12]:
                name: x
                type: const uint
                ty_span: [6-10]
                io_kind: Default"#]],
    );
}

#[test]
fn const_explicit_bitness_int() {
    check_classical_decl(
        "uint[10] x;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-11]:
                symbol_id: 8
                ty_span: [0-8]
                ty_exprs:
                    Expr [5-7]:
                        ty: const uint
                        const_value: Int(10)
                        kind: Lit: Int(10)
                init_expr: Expr [0-11]:
                    ty: const uint[10]
                    kind: Lit: Int(0)
            [8] Symbol [9-10]:
                name: x
                type: uint[10]
                ty_span: [0-8]
                io_kind: Default"#]],
    );
}

#[test]
fn assigning_uint_to_negative_lit() {
    check_classical_decl(
        "const uint[10] x = -42;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [6-14]
                ty_exprs:
                    Expr [11-13]:
                        ty: const uint
                        const_value: Int(10)
                        kind: Lit: Int(10)
                init_expr: Expr [20-22]:
                    ty: const uint[10]
                    const_value: Int(-42)
                    kind: Cast [20-22]:
                        ty: const uint[10]
                        ty_exprs: <empty>
                        expr: Expr [20-22]:
                            ty: const int
                            kind: UnaryOpExpr [20-22]:
                                op: Neg
                                expr: Expr [20-22]:
                                    ty: const int
                                    kind: Lit: Int(42)
                        kind: Implicit
            [8] Symbol [15-16]:
                name: x
                type: const uint[10]
                ty_span: [6-14]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_uint_const_negative_decl() {
    check_classical_decl(
        "const uint x = -42;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-19]:
                symbol_id: 8
                ty_span: [6-10]
                ty_exprs: <empty>
                init_expr: Expr [16-18]:
                    ty: const uint
                    const_value: Int(-42)
                    kind: Cast [16-18]:
                        ty: const uint
                        ty_exprs: <empty>
                        expr: Expr [16-18]:
                            ty: const int
                            kind: UnaryOpExpr [16-18]:
                                op: Neg
                                expr: Expr [16-18]:
                                    ty: const int
                                    kind: Lit: Int(42)
                        kind: Implicit
            [8] Symbol [11-12]:
                name: x
                type: const uint
                ty_span: [6-10]
                io_kind: Default"#]],
    );
}

#[test]
fn explicit_bitness_uint_const_negative_decl() {
    check_classical_decl(
        "const uint[32] x = -42;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [6-14]
                ty_exprs:
                    Expr [11-13]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [20-22]:
                    ty: const uint[32]
                    const_value: Int(-42)
                    kind: Cast [20-22]:
                        ty: const uint[32]
                        ty_exprs: <empty>
                        expr: Expr [20-22]:
                            ty: const int
                            kind: UnaryOpExpr [20-22]:
                                op: Neg
                                expr: Expr [20-22]:
                                    ty: const int
                                    kind: Lit: Int(42)
                        kind: Implicit
            [8] Symbol [15-16]:
                name: x
                type: const uint[32]
                ty_span: [6-14]
                io_kind: Default"#]],
    );
}
