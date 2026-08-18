// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decl;

#[test]
fn implicit_bitness_default() {
    check_classical_decl(
        "complex[float] x;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-17]:
                symbol_id: 8
                ty_span: [0-14]
                ty_exprs: <empty>
                init_expr: Expr [0-17]:
                    ty: const complex[float]
                    kind: Lit: Complex(0.0, 0.0)
            [8] Symbol [15-16]:
                name: x
                type: complex[float]
                ty_span: [0-14]
                io_kind: Default"#]],
    );
}

#[test]
fn explicit_bitness_default() {
    check_classical_decl(
        "complex[float[42]] x;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-21]:
                symbol_id: 8
                ty_span: [0-18]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(42)
                        kind: Lit: Int(42)
                init_expr: Expr [0-21]:
                    ty: const complex[float[42]]
                    kind: Lit: Complex(0.0, 0.0)
            [8] Symbol [19-20]:
                name: x
                type: complex[float[42]]
                ty_span: [0-18]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_double_img_only() {
    check_classical_decl(
        "const complex[float] x = 1.01im;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-32]:
                symbol_id: 8
                ty_span: [6-20]
                ty_exprs: <empty>
                init_expr: Expr [25-31]:
                    ty: const complex[float]
                    const_value: Complex(0.0, 1.01)
                    kind: Lit: Complex(0.0, 1.01)
            [8] Symbol [21-22]:
                name: x
                type: const complex[float]
                ty_span: [6-20]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_int_img_only() {
    check_classical_decl(
        "const complex[float] x = 1im;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-29]:
                symbol_id: 8
                ty_span: [6-20]
                ty_exprs: <empty>
                init_expr: Expr [25-28]:
                    ty: const complex[float]
                    const_value: Complex(0.0, 1.0)
                    kind: Lit: Complex(0.0, 1.0)
            [8] Symbol [21-22]:
                name: x
                type: const complex[float]
                ty_span: [6-20]
                io_kind: Default"#]],
    );
}

#[test]
fn const_explicit_bitness_double_img_only() {
    check_classical_decl(
        "const complex[float[42]] x = 1.01im;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-36]:
                symbol_id: 8
                ty_span: [6-24]
                ty_exprs:
                    Expr [20-22]:
                        ty: const uint
                        const_value: Int(42)
                        kind: Lit: Int(42)
                init_expr: Expr [29-35]:
                    ty: const complex[float[42]]
                    const_value: Complex(0.0, 1.01)
                    kind: Lit: Complex(0.0, 1.01)
            [8] Symbol [25-26]:
                name: x
                type: const complex[float[42]]
                ty_span: [6-24]
                io_kind: Default"#]],
    );
}

#[test]
fn const_explicit_bitness_int_img_only() {
    check_classical_decl(
        "const complex[float[42]] x = 1im;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-33]:
                symbol_id: 8
                ty_span: [6-24]
                ty_exprs:
                    Expr [20-22]:
                        ty: const uint
                        const_value: Int(42)
                        kind: Lit: Int(42)
                init_expr: Expr [29-32]:
                    ty: const complex[float[42]]
                    const_value: Complex(0.0, 1.0)
                    kind: Lit: Complex(0.0, 1.0)
            [8] Symbol [25-26]:
                name: x
                type: const complex[float[42]]
                ty_span: [6-24]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_double_img_only() {
    check_classical_decl(
        "complex[float] x = 1.01im;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-26]:
                symbol_id: 8
                ty_span: [0-14]
                ty_exprs: <empty>
                init_expr: Expr [19-25]:
                    ty: complex[float]
                    kind: Lit: Complex(0.0, 1.01)
            [8] Symbol [15-16]:
                name: x
                type: complex[float]
                ty_span: [0-14]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_int_img_only() {
    check_classical_decl(
        "complex[float] x = 1im;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [0-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: complex[float]
                    kind: Lit: Complex(0.0, 1.0)
            [8] Symbol [15-16]:
                name: x
                type: complex[float]
                ty_span: [0-14]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_double_real_only() {
    check_classical_decl(
        "const complex[float] x = 1.01;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-30]:
                symbol_id: 8
                ty_span: [6-20]
                ty_exprs: <empty>
                init_expr: Expr [25-29]:
                    ty: const complex[float]
                    const_value: Complex(1.01, 0.0)
                    kind: Lit: Complex(1.01, 0.0)
            [8] Symbol [21-22]:
                name: x
                type: const complex[float]
                ty_span: [6-20]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_int_real_only() {
    check_classical_decl(
        "const complex[float] x = 1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-27]:
                symbol_id: 8
                ty_span: [6-20]
                ty_exprs: <empty>
                init_expr: Expr [25-26]:
                    ty: const complex[float]
                    const_value: Complex(1.0, 0.0)
                    kind: Lit: Complex(1.0, 0.0)
            [8] Symbol [21-22]:
                name: x
                type: const complex[float]
                ty_span: [6-20]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_double_real_only() {
    check_classical_decl(
        "complex[float] x = 1.01;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-24]:
                symbol_id: 8
                ty_span: [0-14]
                ty_exprs: <empty>
                init_expr: Expr [19-23]:
                    ty: const complex[float]
                    kind: Lit: Complex(1.01, 0.0)
            [8] Symbol [15-16]:
                name: x
                type: complex[float]
                ty_span: [0-14]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_int_real_only() {
    check_classical_decl(
        "complex[float] x = 1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-21]:
                symbol_id: 8
                ty_span: [0-14]
                ty_exprs: <empty>
                init_expr: Expr [19-20]:
                    ty: const complex[float]
                    kind: Lit: Complex(1.0, 0.0)
            [8] Symbol [15-16]:
                name: x
                type: complex[float]
                ty_span: [0-14]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_simple_double_pos_im() {
    check_classical_decl(
        "complex[float] x = 1.1 + 2.2im;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-31]:
                symbol_id: 8
                ty_span: [0-14]
                ty_exprs: <empty>
                init_expr: Expr [19-30]:
                    ty: complex[float]
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [19-22]:
                            ty: const complex[float]
                            kind: Lit: Complex(1.1, 0.0)
                        rhs: Expr [25-30]:
                            ty: const complex[float]
                            kind: Lit: Complex(0.0, 2.2)
            [8] Symbol [15-16]:
                name: x
                type: complex[float]
                ty_span: [0-14]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_simple_double_neg_im() {
    check_classical_decl(
        "complex[float] x = 1.1 - 2.2im;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-31]:
                symbol_id: 8
                ty_span: [0-14]
                ty_exprs: <empty>
                init_expr: Expr [19-30]:
                    ty: complex[float]
                    kind: BinaryOpExpr:
                        op: Sub
                        lhs: Expr [19-22]:
                            ty: const complex[float]
                            kind: Lit: Complex(1.1, 0.0)
                        rhs: Expr [25-30]:
                            ty: const complex[float]
                            kind: Lit: Complex(0.0, 2.2)
            [8] Symbol [15-16]:
                name: x
                type: complex[float]
                ty_span: [0-14]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_simple_double_neg_im() {
    check_classical_decl(
        "const complex[float] x = 1.1 - 2.2im;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-37]:
                symbol_id: 8
                ty_span: [6-20]
                ty_exprs: <empty>
                init_expr: Expr [25-36]:
                    ty: const complex[float]
                    const_value: Complex(1.1, -2.2)
                    kind: BinaryOpExpr:
                        op: Sub
                        lhs: Expr [25-28]:
                            ty: const complex[float]
                            kind: Lit: Complex(1.1, 0.0)
                        rhs: Expr [31-36]:
                            ty: const complex[float]
                            kind: Lit: Complex(0.0, 2.2)
            [8] Symbol [21-22]:
                name: x
                type: const complex[float]
                ty_span: [6-20]
                io_kind: Default"#]],
    );
}

#[test]
fn implicit_bitness_simple_double_neg_real() {
    check_classical_decl(
        "complex[float] x = -1.1 + 2.2im;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-32]:
                symbol_id: 8
                ty_span: [0-14]
                ty_exprs: <empty>
                init_expr: Expr [19-31]:
                    ty: complex[float]
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [20-23]:
                            ty: const complex[float]
                            kind: Cast [20-23]:
                                ty: const complex[float]
                                ty_exprs: <empty>
                                expr: Expr [20-23]:
                                    ty: const float
                                    kind: UnaryOpExpr [20-23]:
                                        op: Neg
                                        expr: Expr [20-23]:
                                            ty: const float
                                            kind: Lit: Float(1.1)
                                kind: Implicit
                        rhs: Expr [26-31]:
                            ty: const complex[float]
                            kind: Lit: Complex(0.0, 2.2)
            [8] Symbol [15-16]:
                name: x
                type: complex[float]
                ty_span: [0-14]
                io_kind: Default"#]],
    );
}

#[test]
fn const_implicit_bitness_simple_double_neg_real() {
    check_classical_decl(
        "const complex[float] x = -1.1 + 2.2im;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-38]:
                symbol_id: 8
                ty_span: [6-20]
                ty_exprs: <empty>
                init_expr: Expr [25-37]:
                    ty: const complex[float]
                    const_value: Complex(-1.1, 2.2)
                    kind: BinaryOpExpr:
                        op: Add
                        lhs: Expr [26-29]:
                            ty: const complex[float]
                            kind: Cast [26-29]:
                                ty: const complex[float]
                                ty_exprs: <empty>
                                expr: Expr [26-29]:
                                    ty: const float
                                    kind: UnaryOpExpr [26-29]:
                                        op: Neg
                                        expr: Expr [26-29]:
                                            ty: const float
                                            kind: Lit: Float(1.1)
                                kind: Implicit
                        rhs: Expr [32-37]:
                            ty: const complex[float]
                            kind: Lit: Complex(0.0, 2.2)
            [8] Symbol [21-22]:
                name: x
                type: const complex[float]
                ty_span: [6-20]
                io_kind: Default"#]],
    );
}
