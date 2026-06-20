// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decl;

#[test]
fn implicit_bitness_default() {
    check_classical_decl(
        "float x;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-8]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [0-8]:
                    ty: const float
                    kind: Lit: Float(0.0)
            [8] Symbol [6-7]:
                name: x
                type: float
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn lit() {
    check_classical_decl(
        "float x = 42.1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-15]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-14]:
                    ty: float
                    kind: Lit: Float(42.1)
            [8] Symbol [6-7]:
                name: x
                type: float
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit() {
    check_classical_decl(
        "const float x = 42.1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-21]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-20]:
                    ty: const float
                    const_value: Float(42.1)
                    kind: Lit: Float(42.1)
            [8] Symbol [12-13]:
                name: x
                type: const float
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_explicit_width() {
    check_classical_decl(
        "float[64] x = 42.1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-19]:
                symbol_id: 8
                ty_span: [0-9]
                ty_exprs:
                    Expr [6-8]:
                        ty: const uint
                        const_value: Int(64)
                        kind: Lit: Int(64)
                init_expr: Expr [14-18]:
                    ty: const float[64]
                    kind: Lit: Float(42.1)
            [8] Symbol [10-11]:
                name: x
                type: float[64]
                ty_span: [0-9]
                io_kind: Default"#]],
    );
}

#[test]
fn const_explicit_width_lit() {
    check_classical_decl(
        "const float[64] x = 42.1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-25]:
                symbol_id: 8
                ty_span: [6-15]
                ty_exprs:
                    Expr [12-14]:
                        ty: const uint
                        const_value: Int(64)
                        kind: Lit: Int(64)
                init_expr: Expr [20-24]:
                    ty: const float[64]
                    const_value: Float(42.1)
                    kind: Lit: Float(42.1)
            [8] Symbol [16-17]:
                name: x
                type: const float[64]
                ty_span: [6-15]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_decl_leading_dot() {
    check_classical_decl(
        "float x = .421;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-15]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-14]:
                    ty: float
                    kind: Lit: Float(0.421)
            [8] Symbol [6-7]:
                name: x
                type: float
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_leading_dot() {
    check_classical_decl(
        "const float x = .421;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-21]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-20]:
                    ty: const float
                    const_value: Float(0.421)
                    kind: Lit: Float(0.421)
            [8] Symbol [12-13]:
                name: x
                type: const float
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_leading_dot_scientific() {
    check_classical_decl(
        "const float x = .421e2;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-22]:
                    ty: const float
                    const_value: Float(42.1)
                    kind: Lit: Float(42.1)
            [8] Symbol [12-13]:
                name: x
                type: const float
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_decl_trailing_dot() {
    check_classical_decl(
        "float x = 421.;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-15]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-14]:
                    ty: float
                    kind: Lit: Float(421.0)
            [8] Symbol [6-7]:
                name: x
                type: float
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_trailing_dot() {
    check_classical_decl(
        "const float x = 421.;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-21]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-20]:
                    ty: const float
                    const_value: Float(421.0)
                    kind: Lit: Float(421.0)
            [8] Symbol [12-13]:
                name: x
                type: const float
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_decl_scientific() {
    check_classical_decl(
        "float x = 4.21e1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-17]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-16]:
                    ty: float
                    kind: Lit: Float(42.1)
            [8] Symbol [6-7]:
                name: x
                type: float
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_scientific() {
    check_classical_decl(
        "const float x = 4.21e1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-22]:
                    ty: const float
                    const_value: Float(42.1)
                    kind: Lit: Float(42.1)
            [8] Symbol [12-13]:
                name: x
                type: const float
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_decl_scientific_signed_pos() {
    check_classical_decl(
        "float x = 4.21e+1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-18]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-17]:
                    ty: float
                    kind: Lit: Float(42.1)
            [8] Symbol [6-7]:
                name: x
                type: float
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_scientific_signed_pos() {
    check_classical_decl(
        "const float x = 4.21e+1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-24]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-23]:
                    ty: const float
                    const_value: Float(42.1)
                    kind: Lit: Float(42.1)
            [8] Symbol [12-13]:
                name: x
                type: const float
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_decl_scientific_cap_e() {
    check_classical_decl(
        "float x = 4.21E1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-17]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-16]:
                    ty: float
                    kind: Lit: Float(42.1)
            [8] Symbol [6-7]:
                name: x
                type: float
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_scientific_cap_e() {
    check_classical_decl(
        "const float x = 4.21E1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-22]:
                    ty: const float
                    const_value: Float(42.1)
                    kind: Lit: Float(42.1)
            [8] Symbol [12-13]:
                name: x
                type: const float
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_decl_scientific_signed_neg() {
    check_classical_decl(
        "float x = 421.0e-1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-19]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-18]:
                    ty: float
                    kind: Lit: Float(42.1)
            [8] Symbol [6-7]:
                name: x
                type: float
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_scientific_signed_neg() {
    check_classical_decl(
        "const float x = 421.0e-1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-25]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-24]:
                    ty: const float
                    const_value: Float(42.1)
                    kind: Lit: Float(42.1)
            [8] Symbol [12-13]:
                name: x
                type: const float
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_signed_float_lit_cast_neg() {
    check_classical_decl(
        "const float x = -7.;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-20]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: const float
                    const_value: Float(-7.0)
                    kind: UnaryOpExpr [17-19]:
                        op: Neg
                        expr: Expr [17-19]:
                            ty: const float
                            kind: Lit: Float(7.0)
            [8] Symbol [12-13]:
                name: x
                type: const float
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_signed_int_lit_cast_neg() {
    check_classical_decl(
        "const float x = -7;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-19]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [17-18]:
                    ty: const float
                    const_value: Float(-7.0)
                    kind: Cast [17-18]:
                        ty: const float
                        ty_exprs: <empty>
                        expr: Expr [17-18]:
                            ty: const int
                            kind: UnaryOpExpr [17-18]:
                                op: Neg
                                expr: Expr [17-18]:
                                    ty: const int
                                    kind: Lit: Int(7)
                        kind: Implicit
            [8] Symbol [12-13]:
                name: x
                type: const float
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn init_float_with_int_value_equal_max_safely_representable_values() {
    let max_exact_int = 2i64.pow(f64::MANTISSA_DIGITS);
    check_classical_decl(
        format!("float a = {max_exact_int};"),
        &expect![[r#"
            ClassicalDeclarationStmt [0-27]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-26]:
                    ty: const float
                    kind: Lit: Float(9007199254740992.0)
            [8] Symbol [6-7]:
                name: a
                type: float
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn init_float_with_int_value_greater_than_safely_representable_values() {
    let max_exact_int = 2i64.pow(f64::MANTISSA_DIGITS);
    let next = max_exact_int + 1;
    check_classical_decl(
        format!("float a = {next};"),
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-27]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [0-27]:
                            symbol_id: 8
                            ty_span: [0-5]
                            ty_exprs: <empty>
                            init_expr: Expr [10-26]:
                                ty: const int
                                kind: Lit: Int(9007199254740993)

            [Qasm.Lowerer.InvalidCastValueRange

              x assigning const int values to float must be in a range that can be
              | converted to float
               ,-[test:1:11]
             1 | float a = 9007199254740993;
               :           ^^^^^^^^^^^^^^^^
               `----
            ]"#]],
    );
}

#[test]
fn init_float_with_int_value_equal_min_safely_representable_values() {
    let min_exact_int = -(2i64.pow(f64::MANTISSA_DIGITS));
    check_classical_decl(
        format!("float a = {min_exact_int};"),
        &expect![[r#"
            ClassicalDeclarationStmt [0-28]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [11-27]:
                    ty: float
                    kind: Cast [11-27]:
                        ty: float
                        ty_exprs: <empty>
                        expr: Expr [11-27]:
                            ty: const int
                            kind: UnaryOpExpr [11-27]:
                                op: Neg
                                expr: Expr [11-27]:
                                    ty: const int
                                    kind: Lit: Int(9007199254740992)
                        kind: Implicit
            [8] Symbol [6-7]:
                name: a
                type: float
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}
