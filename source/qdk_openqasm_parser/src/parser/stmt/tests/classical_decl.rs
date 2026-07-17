// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::parser::tests::check;

use crate::parser::stmt::parse;

#[test]
fn bit_decl() {
    check(
        parse,
        "bit b;",
        &expect![[r#"
            Stmt [0-6]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-6]:
                    type: ScalarType [0-3]: BitType [0-3]:
                        size: <none>
                    ident: Ident [4-5] "b"
                    init_expr: <none>"#]],
    );
}

#[test]
fn bit_decl_bit_lit() {
    check(
        parse,
        "bit b = 1;",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-10]:
                    type: ScalarType [0-3]: BitType [0-3]:
                        size: <none>
                    ident: Ident [4-5] "b"
                    init_expr: Expr [8-9]: Lit: Int(1)"#]],
    );
}

#[test]
fn const_bit_decl_bit_lit() {
    check(
        parse,
        "const bit b = 1;",
        &expect![[r#"
            Stmt [0-16]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-16]:
                    type: ScalarType [6-9]: BitType [6-9]:
                        size: <none>
                    ident: Ident [10-11] "b"
                    init_expr: Expr [14-15]: Lit: Int(1)"#]],
    );
}

#[test]
fn bit_array_decl() {
    check(
        parse,
        "bit[2] b;",
        &expect![[r#"
            Stmt [0-9]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-9]:
                    type: ScalarType [0-6]: BitType [0-6]:
                        size: Expr [4-5]: Lit: Int(2)
                    ident: Ident [7-8] "b"
                    init_expr: <none>"#]],
    );
}

#[test]
fn bit_array_decl_bit_lit() {
    check(
        parse,
        "bit[2] b = \"11\";",
        &expect![[r#"
            Stmt [0-16]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-16]:
                    type: ScalarType [0-6]: BitType [0-6]:
                        size: Expr [4-5]: Lit: Int(2)
                    ident: Ident [7-8] "b"
                    init_expr: Expr [11-15]: Lit: Bitstring("11")"#]],
    );
}

#[test]
fn const_bit_array_decl_bit_lit() {
    check(
        parse,
        "const bit[2] b = \"11\";",
        &expect![[r#"
            Stmt [0-22]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-22]:
                    type: ScalarType [6-12]: BitType [6-12]:
                        size: Expr [10-11]: Lit: Int(2)
                    ident: Ident [13-14] "b"
                    init_expr: Expr [17-21]: Lit: Bitstring("11")"#]],
    );
}

#[test]
fn bool_decl() {
    check(
        parse,
        "bool b;",
        &expect![[r#"
            Stmt [0-7]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-7]:
                    type: ScalarType [0-4]: BoolType
                    ident: Ident [5-6] "b"
                    init_expr: <none>"#]],
    );
}

#[test]
fn bool_decl_int_lit() {
    check(
        parse,
        "bool b = 1;",
        &expect![[r#"
            Stmt [0-11]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-11]:
                    type: ScalarType [0-4]: BoolType
                    ident: Ident [5-6] "b"
                    init_expr: Expr [9-10]: Lit: Int(1)"#]],
    );
}

#[test]
fn const_bool_decl_bool_lit() {
    check(
        parse,
        "const bool b = true;",
        &expect![[r#"
            Stmt [0-20]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-20]:
                    type: ScalarType [6-10]: BoolType
                    ident: Ident [11-12] "b"
                    init_expr: Expr [15-19]: Lit: Bool(true)"#]],
    );
}

#[test]
fn complex_decl() {
    check(
        parse,
        "complex c;",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-10]:
                    type: ScalarType [0-7]: ComplexType [0-7]:
                        base_size: <none>
                    ident: Ident [8-9] "c"
                    init_expr: <none>"#]],
    );
}

#[test]
fn complex_decl_complex_lit() {
    check(
        parse,
        "complex c = 1im;",
        &expect![[r#"
            Stmt [0-16]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-16]:
                    type: ScalarType [0-7]: ComplexType [0-7]:
                        base_size: <none>
                    ident: Ident [8-9] "c"
                    init_expr: Expr [12-15]: Lit: Imaginary(1.0)"#]],
    );
}

#[test]
fn const_complex_decl_complex_lit() {
    check(
        parse,
        "const complex c = 1im;",
        &expect![[r#"
            Stmt [0-22]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-22]:
                    type: ScalarType [6-13]: ComplexType [6-13]:
                        base_size: <none>
                    ident: Ident [14-15] "c"
                    init_expr: Expr [18-21]: Lit: Imaginary(1.0)"#]],
    );
}

#[test]
fn const_complex_decl_complex_binary_lit() {
    check(
        parse,
        "const complex c = 23.5 + 1.7im;",
        &expect![[r#"
            Stmt [0-31]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-31]:
                    type: ScalarType [6-13]: ComplexType [6-13]:
                        base_size: <none>
                    ident: Ident [14-15] "c"
                    init_expr: Expr [18-30]: BinaryOpExpr:
                        op: Add
                        lhs: Expr [18-22]: Lit: Float(23.5)
                        rhs: Expr [25-30]: Lit: Imaginary(1.7)"#]],
    );
}

#[test]
fn complex_sized_decl() {
    check(
        parse,
        "complex[float[32]] c;",
        &expect![[r#"
            Stmt [0-21]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-21]:
                    type: ScalarType [0-18]: ComplexType [0-18]:
                        base_size: FloatType [8-17]:
                            size: Expr [14-16]: Lit: Int(32)
                    ident: Ident [19-20] "c"
                    init_expr: <none>"#]],
    );
}

#[test]
fn complex_sized_non_float_subty_decl() {
    check(
        parse,
        "complex[int[32]] c;",
        &expect![[r#"
            Error(
                Rule(
                    "scalar or array type",
                    Type(
                        Int,
                    ),
                    Span {
                        lo: 8,
                        hi: 11,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn complex_sized_decl_complex_lit() {
    check(
        parse,
        "complex[float[32]] c = 1im;",
        &expect![[r#"
            Stmt [0-27]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-27]:
                    type: ScalarType [0-18]: ComplexType [0-18]:
                        base_size: FloatType [8-17]:
                            size: Expr [14-16]: Lit: Int(32)
                    ident: Ident [19-20] "c"
                    init_expr: Expr [23-26]: Lit: Imaginary(1.0)"#]],
    );
}

#[test]
fn const_complex_sized_decl_complex_lit() {
    check(
        parse,
        "const complex[float[32]] c = 1im;",
        &expect![[r#"
            Stmt [0-33]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-33]:
                    type: ScalarType [6-24]: ComplexType [6-24]:
                        base_size: FloatType [14-23]:
                            size: Expr [20-22]: Lit: Int(32)
                    ident: Ident [25-26] "c"
                    init_expr: Expr [29-32]: Lit: Imaginary(1.0)"#]],
    );
}

#[test]
fn const_complex_implicit_bitness_default() {
    check(
        parse,
        "const complex[float] x;",
        &expect![[r#"
            Error(
                Token(
                    Eq,
                    Semicolon,
                    Span {
                        lo: 22,
                        hi: 23,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn const_complex_explicit_bitness_default() {
    check(
        parse,
        "const complex[float[42]] x;",
        &expect![[r#"
            Error(
                Token(
                    Eq,
                    Semicolon,
                    Span {
                        lo: 26,
                        hi: 27,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn int_decl() {
    check(
        parse,
        "int i;",
        &expect![[r#"
            Stmt [0-6]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-6]:
                    type: ScalarType [0-3]: IntType [0-3]:
                        size: <none>
                    ident: Ident [4-5] "i"
                    init_expr: <none>"#]],
    );
}

#[test]
fn int_decl_int_lit() {
    check(
        parse,
        "int i = 1;",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-10]:
                    type: ScalarType [0-3]: IntType [0-3]:
                        size: <none>
                    ident: Ident [4-5] "i"
                    init_expr: Expr [8-9]: Lit: Int(1)"#]],
    );
}

#[test]
fn const_int_explicit_bitness_int_default() {
    check(
        parse,
        "const int[10] x;",
        &expect![[r#"
            Error(
                Token(
                    Eq,
                    Semicolon,
                    Span {
                        lo: 15,
                        hi: 16,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn const_int_implicit_bitness_int_default() {
    check(
        parse,
        "const int x;",
        &expect![[r#"
            Error(
                Token(
                    Eq,
                    Semicolon,
                    Span {
                        lo: 11,
                        hi: 12,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn const_int_decl_int_lit() {
    check(
        parse,
        "const int i = 1;",
        &expect![[r#"
            Stmt [0-16]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-16]:
                    type: ScalarType [6-9]: IntType [6-9]:
                        size: <none>
                    ident: Ident [10-11] "i"
                    init_expr: Expr [14-15]: Lit: Int(1)"#]],
    );
}

#[test]
fn int_sized_decl() {
    check(
        parse,
        "int[32] i;",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-10]:
                    type: ScalarType [0-7]: IntType [0-7]:
                        size: Expr [4-6]: Lit: Int(32)
                    ident: Ident [8-9] "i"
                    init_expr: <none>"#]],
    );
}

#[test]
fn int_sized_decl_int_lit() {
    check(
        parse,
        "int[32] i = 1;",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-14]:
                    type: ScalarType [0-7]: IntType [0-7]:
                        size: Expr [4-6]: Lit: Int(32)
                    ident: Ident [8-9] "i"
                    init_expr: Expr [12-13]: Lit: Int(1)"#]],
    );
}

#[test]
fn const_int_sized_decl_int_lit() {
    check(
        parse,
        "const int[32] i = 1;",
        &expect![[r#"
            Stmt [0-20]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-20]:
                    type: ScalarType [6-13]: IntType [6-13]:
                        size: Expr [10-12]: Lit: Int(32)
                    ident: Ident [14-15] "i"
                    init_expr: Expr [18-19]: Lit: Int(1)"#]],
    );
}

#[test]
fn uint_decl() {
    check(
        parse,
        "uint i;",
        &expect![[r#"
            Stmt [0-7]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-7]:
                    type: ScalarType [0-4]: UIntType [0-4]:
                        size: <none>
                    ident: Ident [5-6] "i"
                    init_expr: <none>"#]],
    );
}

#[test]
fn uint_decl_uint_lit() {
    check(
        parse,
        "uint i = 1;",
        &expect![[r#"
            Stmt [0-11]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-11]:
                    type: ScalarType [0-4]: UIntType [0-4]:
                        size: <none>
                    ident: Ident [5-6] "i"
                    init_expr: Expr [9-10]: Lit: Int(1)"#]],
    );
}

#[test]
fn const_uint_explicit_bitness_uint_default() {
    check(
        parse,
        "const uint[10] x;",
        &expect![[r#"
            Error(
                Token(
                    Eq,
                    Semicolon,
                    Span {
                        lo: 16,
                        hi: 17,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn const_uint_implicit_bitness_uint_default() {
    check(
        parse,
        "const uint x;",
        &expect![[r#"
            Error(
                Token(
                    Eq,
                    Semicolon,
                    Span {
                        lo: 12,
                        hi: 13,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn const_uint_decl_uint_lit() {
    check(
        parse,
        "const uint i = 1;",
        &expect![[r#"
            Stmt [0-17]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-17]:
                    type: ScalarType [6-10]: UIntType [6-10]:
                        size: <none>
                    ident: Ident [11-12] "i"
                    init_expr: Expr [15-16]: Lit: Int(1)"#]],
    );
}

#[test]
fn uint_sized_decl() {
    check(
        parse,
        "uint[32] i;",
        &expect![[r#"
            Stmt [0-11]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-11]:
                    type: ScalarType [0-8]: UIntType [0-8]:
                        size: Expr [5-7]: Lit: Int(32)
                    ident: Ident [9-10] "i"
                    init_expr: <none>"#]],
    );
}

#[test]
fn uint_sized_decl_uint_lit() {
    check(
        parse,
        "uint[32] i = 1;",
        &expect![[r#"
            Stmt [0-15]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-15]:
                    type: ScalarType [0-8]: UIntType [0-8]:
                        size: Expr [5-7]: Lit: Int(32)
                    ident: Ident [9-10] "i"
                    init_expr: Expr [13-14]: Lit: Int(1)"#]],
    );
}

#[test]
fn const_uint_sized_decl_uint_lit() {
    check(
        parse,
        "const uint[32] i = 1;",
        &expect![[r#"
            Stmt [0-21]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-21]:
                    type: ScalarType [6-14]: UIntType [6-14]:
                        size: Expr [11-13]: Lit: Int(32)
                    ident: Ident [15-16] "i"
                    init_expr: Expr [19-20]: Lit: Int(1)"#]],
    );
}

#[test]
fn float_decl() {
    check(
        parse,
        "float f;",
        &expect![[r#"
            Stmt [0-8]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-8]:
                    type: ScalarType [0-5]: FloatType [0-5]:
                        size: <none>
                    ident: Ident [6-7] "f"
                    init_expr: <none>"#]],
    );
}

#[test]
fn float_decl_float_lit() {
    check(
        parse,
        "float f = 1;",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-12]:
                    type: ScalarType [0-5]: FloatType [0-5]:
                        size: <none>
                    ident: Ident [6-7] "f"
                    init_expr: Expr [10-11]: Lit: Int(1)"#]],
    );
}

#[test]
fn const_float_decl_float_lit() {
    check(
        parse,
        "const float f = 1.0;",
        &expect![[r#"
            Stmt [0-20]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-20]:
                    type: ScalarType [6-11]: FloatType [6-11]:
                        size: <none>
                    ident: Ident [12-13] "f"
                    init_expr: Expr [16-19]: Lit: Float(1.0)"#]],
    );
}

#[test]
fn const_float_default() {
    check(
        parse,
        "const float x;",
        &expect![[r#"
            Error(
                Token(
                    Eq,
                    Semicolon,
                    Span {
                        lo: 13,
                        hi: 14,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn const_float_sized_default() {
    check(
        parse,
        "const float[64] x;",
        &expect![[r#"
            Error(
                Token(
                    Eq,
                    Semicolon,
                    Span {
                        lo: 17,
                        hi: 18,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn float_sized_decl() {
    check(
        parse,
        "float[32] f;",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-12]:
                    type: ScalarType [0-9]: FloatType [0-9]:
                        size: Expr [6-8]: Lit: Int(32)
                    ident: Ident [10-11] "f"
                    init_expr: <none>"#]],
    );
}

#[test]
fn float_sized_decl_float_lit() {
    check(
        parse,
        "float[32] f = 1.0;",
        &expect![[r#"
            Stmt [0-18]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-18]:
                    type: ScalarType [0-9]: FloatType [0-9]:
                        size: Expr [6-8]: Lit: Int(32)
                    ident: Ident [10-11] "f"
                    init_expr: Expr [14-17]: Lit: Float(1.0)"#]],
    );
}

#[test]
fn const_float_sized_decl_float_lit() {
    check(
        parse,
        "const float[32] f = 1;",
        &expect![[r#"
            Stmt [0-22]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-22]:
                    type: ScalarType [6-15]: FloatType [6-15]:
                        size: Expr [12-14]: Lit: Int(32)
                    ident: Ident [16-17] "f"
                    init_expr: Expr [20-21]: Lit: Int(1)"#]],
    );
}

#[test]
fn angle_decl() {
    check(
        parse,
        "angle a;",
        &expect![[r#"
            Stmt [0-8]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-8]:
                    type: ScalarType [0-5]: AngleType [0-5]:
                        size: <none>
                    ident: Ident [6-7] "a"
                    init_expr: <none>"#]],
    );
}

#[test]
fn angle_decl_angle_lit() {
    check(
        parse,
        "angle a = 1.0;",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-14]:
                    type: ScalarType [0-5]: AngleType [0-5]:
                        size: <none>
                    ident: Ident [6-7] "a"
                    init_expr: Expr [10-13]: Lit: Float(1.0)"#]],
    );
}

#[test]
fn const_angle_decl_no_init() {
    check(
        parse,
        "const angle a;",
        &expect![[r#"
            Error(
                Token(
                    Eq,
                    Semicolon,
                    Span {
                        lo: 13,
                        hi: 14,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn const_angle_decl_angle_lit() {
    check(
        parse,
        "const angle a = 1.0;",
        &expect![[r#"
            Stmt [0-20]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-20]:
                    type: ScalarType [6-11]: AngleType [6-11]:
                        size: <none>
                    ident: Ident [12-13] "a"
                    init_expr: Expr [16-19]: Lit: Float(1.0)"#]],
    );
}

#[test]
fn angle_sized_decl() {
    check(
        parse,
        "angle[32] a;",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-12]:
                    type: ScalarType [0-9]: AngleType [0-9]:
                        size: Expr [6-8]: Lit: Int(32)
                    ident: Ident [10-11] "a"
                    init_expr: <none>"#]],
    );
}

#[test]
fn angle_sized_decl_angle_lit() {
    check(
        parse,
        "angle[32] a = 1.0;",
        &expect![[r#"
            Stmt [0-18]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-18]:
                    type: ScalarType [0-9]: AngleType [0-9]:
                        size: Expr [6-8]: Lit: Int(32)
                    ident: Ident [10-11] "a"
                    init_expr: Expr [14-17]: Lit: Float(1.0)"#]],
    );
}

#[test]
fn const_angle_sized_decl_angle_lit() {
    check(
        parse,
        "const angle[32] a = 1.0;",
        &expect![[r#"
            Stmt [0-24]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-24]:
                    type: ScalarType [6-15]: AngleType [6-15]:
                        size: Expr [12-14]: Lit: Int(32)
                    ident: Ident [16-17] "a"
                    init_expr: Expr [20-23]: Lit: Float(1.0)"#]],
    );
}

#[test]
fn duration_decl() {
    check(
        parse,
        "duration d;",
        &expect![[r#"
            Stmt [0-11]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-11]:
                    type: ScalarType [0-8]: Duration
                    ident: Ident [9-10] "d"
                    init_expr: <none>"#]],
    );
}

#[test]
fn duration_decl_ns_lit() {
    check(
        parse,
        "duration d = 1000ns;",
        &expect![[r#"
            Stmt [0-20]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-20]:
                    type: ScalarType [0-8]: Duration
                    ident: Ident [9-10] "d"
                    init_expr: Expr [13-19]: Lit: Duration(1000.0, Ns)"#]],
    );
}

#[test]
fn duration_decl_us_lit() {
    check(
        parse,
        "duration d = 1000us;",
        &expect![[r#"
            Stmt [0-20]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-20]:
                    type: ScalarType [0-8]: Duration
                    ident: Ident [9-10] "d"
                    init_expr: Expr [13-19]: Lit: Duration(1000.0, Us)"#]],
    );
}

#[test]
fn duration_decl_uus_lit() {
    // uus is for µ, disabling the lint must be done at the
    // crate level, so using uus here in the test name.
    check(
        parse,
        "duration d = 1000µs;",
        &expect![[r#"
            Stmt [0-21]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-21]:
                    type: ScalarType [0-8]: Duration
                    ident: Ident [9-10] "d"
                    init_expr: Expr [13-20]: Lit: Duration(1000.0, Us)"#]],
    );
}

#[test]
fn duration_decl_ms_lit() {
    check(
        parse,
        "duration d = 1000ms;",
        &expect![[r#"
            Stmt [0-20]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-20]:
                    type: ScalarType [0-8]: Duration
                    ident: Ident [9-10] "d"
                    init_expr: Expr [13-19]: Lit: Duration(1000.0, Ms)"#]],
    );
}

#[test]
fn duration_decl_s_lit() {
    check(
        parse,
        "duration d = 1000s;",
        &expect![[r#"
            Stmt [0-19]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-19]:
                    type: ScalarType [0-8]: Duration
                    ident: Ident [9-10] "d"
                    init_expr: Expr [13-18]: Lit: Duration(1000.0, S)"#]],
    );
}

#[test]
fn duration_decl_dt_lit() {
    check(
        parse,
        "duration d = 1000dt;",
        &expect![[r#"
            Stmt [0-20]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-20]:
                    type: ScalarType [0-8]: Duration
                    ident: Ident [9-10] "d"
                    init_expr: Expr [13-19]: Lit: Duration(1000.0, Dt)"#]],
    );
}

#[test]
fn const_duration_decl_dt_lit() {
    check(
        parse,
        "const duration d = 10dt;",
        &expect![[r#"
            Stmt [0-24]:
                annotations: <empty>
                kind: ConstantDeclStmt [0-24]:
                    type: ScalarType [6-14]: Duration
                    ident: Ident [15-16] "d"
                    init_expr: Expr [19-23]: Lit: Duration(10.0, Dt)"#]],
    );
}

#[test]
fn stretch_decl() {
    check(
        parse,
        "stretch s;",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-10]:
                    type: ScalarType [0-7]: Stretch
                    ident: Ident [8-9] "s"
                    init_expr: <none>"#]],
    );
}

#[test]
fn empty_array_decl() {
    check(
        parse,
        "array[int, 0] arr = {};",
        &expect![[r#"
            Stmt [0-23]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-23]:
                    type: ArrayType [0-13]:
                        base_type: ArrayBaseTypeKind IntType [6-9]:
                            size: <none>
                        dimensions:
                            Expr [11-12]: Lit: Int(0)
                    ident: Ident [14-17] "arr"
                    init_expr: Expr [20-22]: Lit:     Array: <empty>"#]],
    );
}

#[test]
fn simple_array_decl() {
    check(
        parse,
        "array[int[32], 3] arr = {1, 2, 3};",
        &expect![[r#"
            Stmt [0-34]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-34]:
                    type: ArrayType [0-17]:
                        base_type: ArrayBaseTypeKind IntType [6-13]:
                            size: Expr [10-12]: Lit: Int(32)
                        dimensions:
                            Expr [15-16]: Lit: Int(3)
                    ident: Ident [18-21] "arr"
                    init_expr: Expr [24-33]: Lit:     Array:
                            Expr [25-26]: Lit: Int(1)
                            Expr [28-29]: Lit: Int(2)
                            Expr [31-32]: Lit: Int(3)"#]],
    );
}

#[test]
fn nested_array_decl() {
    check(
        parse,
        "array[int[32], 3, 2] arr = {{1, 2}, {3, 4}, {5, 6}};",
        &expect![[r#"
            Stmt [0-52]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-52]:
                    type: ArrayType [0-20]:
                        base_type: ArrayBaseTypeKind IntType [6-13]:
                            size: Expr [10-12]: Lit: Int(32)
                        dimensions:
                            Expr [15-16]: Lit: Int(3)
                            Expr [18-19]: Lit: Int(2)
                    ident: Ident [21-24] "arr"
                    init_expr: Expr [27-51]: Lit:     Array:
                            Expr [28-34]: Lit:     Array:
                                    Expr [29-30]: Lit: Int(1)
                                    Expr [32-33]: Lit: Int(2)
                            Expr [36-42]: Lit:     Array:
                                    Expr [37-38]: Lit: Int(3)
                                    Expr [40-41]: Lit: Int(4)
                            Expr [44-50]: Lit:     Array:
                                    Expr [45-46]: Lit: Int(5)
                                    Expr [48-49]: Lit: Int(6)"#]],
    );
}

#[test]
fn measure_hardware_qubit_decl() {
    check(
        parse,
        "bit res = measure $12;",
        &expect![[r#"
            Stmt [0-22]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-22]:
                    type: ScalarType [0-3]: BitType [0-3]:
                        size: <none>
                    ident: Ident [4-7] "res"
                    init_expr: MeasureExpr [10-21]:
                        operand: GateOperand [18-21]:
                            kind: HardwareQubit [18-21]: 12"#]],
    );
}

#[test]
fn measure_register_decl() {
    check(
        parse,
        "bit res = measure qubits[2][3];",
        &expect![[r#"
            Stmt [0-31]:
                annotations: <empty>
                kind: ClassicalDeclarationStmt [0-31]:
                    type: ScalarType [0-3]: BitType [0-3]:
                        size: <none>
                    ident: Ident [4-7] "res"
                    init_expr: MeasureExpr [10-30]:
                        operand: GateOperand [18-30]:
                            kind: IndexedIdent [18-30]:
                                ident: Ident [18-24] "qubits"
                                index_span: [24-30]
                                indices:
                                    IndexList [25-26]:
                                        values:
                                            Expr [25-26]: Lit: Int(2)
                                    IndexList [28-29]:
                                        values:
                                            Expr [28-29]: Lit: Int(3)"#]],
    );
}

#[test]
fn const_decl_with_measurement_init_fails() {
    check(
        parse,
        "const bit res = measure q;",
        &expect![[r#"
            Error(
                Token(
                    Open(
                        Brace,
                    ),
                    Keyword(
                        Measure,
                    ),
                    Span {
                        lo: 16,
                        hi: 23,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn const_array_decl_fails() {
    check(
        parse,
        "const array[int, 2] a = {1, 2};",
        &expect![[r#"
            Error(
                Rule(
                    "scalar type",
                    Type(
                        Array,
                    ),
                    Span {
                        lo: 6,
                        hi: 11,
                    },
                ),
            )
        "#]],
    );
}
