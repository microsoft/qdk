// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decl;

#[test]
fn implicit_bitness_default() {
    check_classical_decl(
        "angle x;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-8]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [0-8]:
                    ty: const angle
                    kind: Lit: Angle(0)
            [8] Symbol [6-7]:
                name: x
                type: angle
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn lit() {
    check_classical_decl(
        "angle x = 42.1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-15]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-14]:
                    ty: const angle
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [6-7]:
                name: x
                type: angle
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit() {
    check_classical_decl(
        "const angle x = 42.1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-21]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-20]:
                    ty: const angle
                    const_value: Angle(4.400888156922484)
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [12-13]:
                name: x
                type: const angle
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_explicit_width() {
    check_classical_decl(
        "angle[64] x = 42.1;",
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
                    ty: const angle[64]
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [10-11]:
                name: x
                type: angle[64]
                ty_span: [0-9]
                io_kind: Default"#]],
    );
}

#[test]
fn const_explicit_width_lit() {
    check_classical_decl(
        "const angle[64] x = 42.1;",
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
                    ty: const angle[64]
                    const_value: Angle(4.400888156922484)
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [16-17]:
                name: x
                type: const angle[64]
                ty_span: [6-15]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_decl_leading_dot() {
    check_classical_decl(
        "angle x = .421;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-15]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-14]:
                    ty: const angle
                    kind: Lit: Angle(0.4210000000000001)
            [8] Symbol [6-7]:
                name: x
                type: angle
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_leading_dot() {
    check_classical_decl(
        "const angle x = .421;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-21]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-20]:
                    ty: const angle
                    const_value: Angle(0.4210000000000001)
                    kind: Lit: Angle(0.4210000000000001)
            [8] Symbol [12-13]:
                name: x
                type: const angle
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_leading_dot_scientific() {
    check_classical_decl(
        "const angle x = .421e2;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-22]:
                    ty: const angle
                    const_value: Angle(4.400888156922484)
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [12-13]:
                name: x
                type: const angle
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_decl_trailing_dot() {
    check_classical_decl(
        "angle x = 421.;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-15]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-14]:
                    ty: const angle
                    kind: Lit: Angle(0.02658441896772248)
            [8] Symbol [6-7]:
                name: x
                type: angle
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_trailing_dot() {
    check_classical_decl(
        "const angle x = 421.;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-21]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-20]:
                    ty: const angle
                    const_value: Angle(0.02658441896772248)
                    kind: Lit: Angle(0.02658441896772248)
            [8] Symbol [12-13]:
                name: x
                type: const angle
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_decl_scientific() {
    check_classical_decl(
        "angle x = 4.21e1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-17]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-16]:
                    ty: const angle
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [6-7]:
                name: x
                type: angle
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_scientific() {
    check_classical_decl(
        "const angle x = 4.21e1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-22]:
                    ty: const angle
                    const_value: Angle(4.400888156922484)
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [12-13]:
                name: x
                type: const angle
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_decl_scientific_signed_pos() {
    check_classical_decl(
        "angle x = 4.21e+1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-18]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-17]:
                    ty: const angle
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [6-7]:
                name: x
                type: angle
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_scientific_signed_pos() {
    check_classical_decl(
        "const angle x = 4.21e+1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-24]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-23]:
                    ty: const angle
                    const_value: Angle(4.400888156922484)
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [12-13]:
                name: x
                type: const angle
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_decl_scientific_cap_e() {
    check_classical_decl(
        "angle x = 4.21E1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-17]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-16]:
                    ty: const angle
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [6-7]:
                name: x
                type: angle
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_scientific_cap_e() {
    check_classical_decl(
        "const angle x = 4.21E1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-23]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-22]:
                    ty: const angle
                    const_value: Angle(4.400888156922484)
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [12-13]:
                name: x
                type: const angle
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn lit_decl_scientific_signed_neg() {
    check_classical_decl(
        "angle x = 421.0e-1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-19]:
                symbol_id: 8
                ty_span: [0-5]
                ty_exprs: <empty>
                init_expr: Expr [10-18]:
                    ty: const angle
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [6-7]:
                name: x
                type: angle
                ty_span: [0-5]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_scientific_signed_neg() {
    check_classical_decl(
        "const angle x = 421.0e-1;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-25]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [16-24]:
                    ty: const angle
                    const_value: Angle(4.400888156922484)
                    kind: Lit: Angle(4.400888156922484)
            [8] Symbol [12-13]:
                name: x
                type: const angle
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_signed_float_lit_cast_neg() {
    check_classical_decl(
        "const angle x = -7.;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-20]:
                symbol_id: 8
                ty_span: [6-11]
                ty_exprs: <empty>
                init_expr: Expr [17-19]:
                    ty: const angle
                    const_value: Angle(5.5663706143591725)
                    kind: Cast [17-19]:
                        ty: const angle
                        ty_exprs: <empty>
                        expr: Expr [17-19]:
                            ty: const float
                            kind: UnaryOpExpr [17-19]:
                                op: Neg
                                expr: Expr [17-19]:
                                    ty: const float
                                    kind: Lit: Float(7.0)
                        kind: Implicit
            [8] Symbol [12-13]:
                name: x
                type: const angle
                ty_span: [6-11]
                io_kind: Default"#]],
    );
}

#[test]
fn const_lit_decl_signed_int_lit_cast_neg_fails() {
    check_classical_decl(
        "const angle x = -7;",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [0-19]:
                            symbol_id: 8
                            ty_span: [6-11]
                            ty_exprs: <empty>
                            init_expr: Expr [17-18]:
                                ty: const int
                                const_value: Int(-7)
                                kind: UnaryOpExpr [17-18]:
                                    op: Neg
                                    expr: Expr [17-18]:
                                        ty: const int
                                        kind: Lit: Int(7)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type const int to type const angle
               ,-[test:1:18]
             1 | const angle x = -7;
               :                  ^
               `----
            ]"#]],
    );
}

#[test]
fn explicit_zero_width_fails() {
    check_classical_decl(
        "angle[0] x = 42.1;",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-18]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [0-18]:
                            symbol_id: 8
                            ty_span: [0-8]
                            ty_exprs: <empty>
                            init_expr: Expr [0-18]:
                                ty: unknown
                                kind: Err

            [Qasm.Lowerer.ExprMustBePositiveInt

              x type width must be a positive integer
               ,-[test:1:7]
             1 | angle[0] x = 42.1;
               :       ^
               `----
            ]"#]],
    );
}

#[test]
fn explicit_width_over_64_fails() {
    check_classical_decl(
        "const angle[65] x = 42.1;",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-25]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [0-25]:
                            symbol_id: 8
                            ty_span: [6-15]
                            ty_exprs: <empty>
                            init_expr: Expr [0-25]:
                                ty: unknown
                                kind: Err

            [Qasm.Lowerer.TypeMaxWidthExceeded

              x angle max width is 64 but 65 was provided
               ,-[test:1:7]
             1 | const angle[65] x = 42.1;
               :       ^^^^^^^^^
               `----
            ]"#]],
    );
}
