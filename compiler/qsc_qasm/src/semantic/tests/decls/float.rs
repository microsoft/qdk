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
                init_expr: Expr [0-0]:
                    ty: Float(None, true)
                    kind: Lit: Float(0.0)
            [8] Symbol [6-7]:
                name: x
                type: Float(None, false)
                qsharp_type: Double
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
                init_expr: Expr [10-14]:
                    ty: Float(None, false)
                    kind: Lit: Float(42.1)
            [8] Symbol [6-7]:
                name: x
                type: Float(None, false)
                qsharp_type: Double
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
                init_expr: Expr [16-20]:
                    ty: Float(None, true)
                    kind: Lit: Float(42.1)
            [8] Symbol [12-13]:
                name: x
                type: Float(None, true)
                qsharp_type: Double
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
                init_expr: Expr [14-18]:
                    ty: Float(Some(64), true)
                    kind: Lit: Float(42.1)
            [8] Symbol [10-11]:
                name: x
                type: Float(Some(64), false)
                qsharp_type: Double
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
                init_expr: Expr [20-24]:
                    ty: Float(Some(64), true)
                    kind: Lit: Float(42.1)
            [8] Symbol [16-17]:
                name: x
                type: Float(Some(64), true)
                qsharp_type: Double
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
                init_expr: Expr [10-14]:
                    ty: Float(None, false)
                    kind: Lit: Float(0.421)
            [8] Symbol [6-7]:
                name: x
                type: Float(None, false)
                qsharp_type: Double
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
                init_expr: Expr [16-20]:
                    ty: Float(None, true)
                    kind: Lit: Float(0.421)
            [8] Symbol [12-13]:
                name: x
                type: Float(None, true)
                qsharp_type: Double
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
                init_expr: Expr [16-22]:
                    ty: Float(None, true)
                    kind: Lit: Float(42.1)
            [8] Symbol [12-13]:
                name: x
                type: Float(None, true)
                qsharp_type: Double
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
                init_expr: Expr [10-14]:
                    ty: Float(None, false)
                    kind: Lit: Float(421.0)
            [8] Symbol [6-7]:
                name: x
                type: Float(None, false)
                qsharp_type: Double
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
                init_expr: Expr [16-20]:
                    ty: Float(None, true)
                    kind: Lit: Float(421.0)
            [8] Symbol [12-13]:
                name: x
                type: Float(None, true)
                qsharp_type: Double
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
                init_expr: Expr [10-16]:
                    ty: Float(None, false)
                    kind: Lit: Float(42.1)
            [8] Symbol [6-7]:
                name: x
                type: Float(None, false)
                qsharp_type: Double
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
                init_expr: Expr [16-22]:
                    ty: Float(None, true)
                    kind: Lit: Float(42.1)
            [8] Symbol [12-13]:
                name: x
                type: Float(None, true)
                qsharp_type: Double
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
                init_expr: Expr [10-17]:
                    ty: Float(None, false)
                    kind: Lit: Float(42.1)
            [8] Symbol [6-7]:
                name: x
                type: Float(None, false)
                qsharp_type: Double
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
                init_expr: Expr [16-23]:
                    ty: Float(None, true)
                    kind: Lit: Float(42.1)
            [8] Symbol [12-13]:
                name: x
                type: Float(None, true)
                qsharp_type: Double
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
                init_expr: Expr [10-16]:
                    ty: Float(None, false)
                    kind: Lit: Float(42.1)
            [8] Symbol [6-7]:
                name: x
                type: Float(None, false)
                qsharp_type: Double
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
                init_expr: Expr [16-22]:
                    ty: Float(None, true)
                    kind: Lit: Float(42.1)
            [8] Symbol [12-13]:
                name: x
                type: Float(None, true)
                qsharp_type: Double
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
                init_expr: Expr [10-18]:
                    ty: Float(None, false)
                    kind: Lit: Float(42.1)
            [8] Symbol [6-7]:
                name: x
                type: Float(None, false)
                qsharp_type: Double
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
                init_expr: Expr [16-24]:
                    ty: Float(None, true)
                    kind: Lit: Float(42.1)
            [8] Symbol [12-13]:
                name: x
                type: Float(None, true)
                qsharp_type: Double
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
                init_expr: Expr [17-19]:
                    ty: Float(None, true)
                    kind: UnaryOpExpr [17-19]:
                        op: Neg
                        expr: Expr [17-19]:
                            ty: Float(None, true)
                            kind: Lit: Float(7.0)
            [8] Symbol [12-13]:
                name: x
                type: Float(None, true)
                qsharp_type: Double
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
                init_expr: Expr [17-18]:
                    ty: Float(None, true)
                    kind: Cast [0-0]:
                        ty: Float(None, true)
                        expr: Expr [17-18]:
                            ty: Int(None, true)
                            kind: UnaryOpExpr [17-18]:
                                op: Neg
                                expr: Expr [17-18]:
                                    ty: Int(None, true)
                                    kind: Lit: Int(7)
            [8] Symbol [12-13]:
                name: x
                type: Float(None, true)
                qsharp_type: Double
                io_kind: Default"#]],
    );
}

#[test]
fn init_float_with_int_value_equal_max_safely_representable_values() {
    let max_exact_int = 2i64.pow(f64::MANTISSA_DIGITS);
    check_classical_decl(
        &format!("float a = {max_exact_int};"),
        &expect![[r#"
            ClassicalDeclarationStmt [0-27]:
                symbol_id: 8
                ty_span: [0-5]
                init_expr: Expr [10-26]:
                    ty: Float(None, true)
                    kind: Lit: Float(9007199254740992.0)
            [8] Symbol [6-7]:
                name: a
                type: Float(None, false)
                qsharp_type: Double
                io_kind: Default"#]],
    );
}

#[test]
fn init_float_with_int_value_greater_than_safely_representable_values() {
    let max_exact_int = 2i64.pow(f64::MANTISSA_DIGITS);
    let next = max_exact_int + 1;
    check_classical_decl(
        &format!("float a = {next};"),
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [0-27]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [0-27]:
                            symbol_id: 8
                            ty_span: [0-5]
                            init_expr: Expr [10-26]:
                                ty: Int(None, true)
                                kind: Lit: Int(9007199254740993)

            [Qasm.Lowerer.InvalidCastValueRange

              x assigning Int(None, true) values to Float(None, false) must be in a range
              | that be converted to Float(None, false)
               ,-[test:1:11]
             1 | float a = 9007199254740993;
               :           ^^^^^^^^^^^^^^^^
               `----
            , Qasm.Lowerer.CannotCastLiteral

              x cannot cast literal expression of type Int(None, true) to type Float(None,
              | false)
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
        &format!("float a = {min_exact_int};"),
        &expect![[r#"
            ClassicalDeclarationStmt [0-28]:
                symbol_id: 8
                ty_span: [0-5]
                init_expr: Expr [11-27]:
                    ty: Float(None, false)
                    kind: Cast [0-0]:
                        ty: Float(None, false)
                        expr: Expr [11-27]:
                            ty: Int(None, true)
                            kind: UnaryOpExpr [11-27]:
                                op: Neg
                                expr: Expr [11-27]:
                                    ty: Int(None, true)
                                    kind: Lit: Int(9007199254740992)
            [8] Symbol [6-7]:
                name: a
                type: Float(None, false)
                qsharp_type: Double
                io_kind: Default"#]],
    );
}
