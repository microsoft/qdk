// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::parser::tests::check;

use crate::parser::stmt::parse;

#[test]
fn input_bit_decl() {
    check(
        parse,
        "input bit b;",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: IODeclaration [0-12]:
                    io_keyword: input
                    type: ScalarType [6-9]: BitType [6-9]:
                        size: <none>
                    ident: Ident [10-11] "b""#]],
    );
}

#[test]
fn output_bit_decl() {
    check(
        parse,
        "output bit b;",
        &expect![[r#"
            Stmt [0-13]:
                annotations: <empty>
                kind: IODeclaration [0-13]:
                    io_keyword: output
                    type: ScalarType [7-10]: BitType [7-10]:
                        size: <none>
                    ident: Ident [11-12] "b""#]],
    );
}

#[test]
fn input_bit_array_decl() {
    check(
        parse,
        "input bit[2] b;",
        &expect![[r#"
            Stmt [0-15]:
                annotations: <empty>
                kind: IODeclaration [0-15]:
                    io_keyword: input
                    type: ScalarType [6-12]: BitType [6-12]:
                        size: Expr [10-11]: Lit: Int(2)
                    ident: Ident [13-14] "b""#]],
    );
}

#[test]
fn output_bit_array_decl() {
    check(
        parse,
        "output bit[2] b;",
        &expect![[r#"
            Stmt [0-16]:
                annotations: <empty>
                kind: IODeclaration [0-16]:
                    io_keyword: output
                    type: ScalarType [7-13]: BitType [7-13]:
                        size: Expr [11-12]: Lit: Int(2)
                    ident: Ident [14-15] "b""#]],
    );
}

#[test]
fn intput_bool_decl() {
    check(
        parse,
        "input bool b;",
        &expect![[r#"
            Stmt [0-13]:
                annotations: <empty>
                kind: IODeclaration [0-13]:
                    io_keyword: input
                    type: ScalarType [6-10]: BoolType
                    ident: Ident [11-12] "b""#]],
    );
}

#[test]
fn output_bool_decl() {
    check(
        parse,
        "output bool b;",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: IODeclaration [0-14]:
                    io_keyword: output
                    type: ScalarType [7-11]: BoolType
                    ident: Ident [12-13] "b""#]],
    );
}

#[test]
fn input_complex_decl() {
    check(
        parse,
        "input complex c;",
        &expect![[r#"
            Stmt [0-16]:
                annotations: <empty>
                kind: IODeclaration [0-16]:
                    io_keyword: input
                    type: ScalarType [6-13]: ComplexType [6-13]:
                        base_size: <none>
                    ident: Ident [14-15] "c""#]],
    );
}

#[test]
fn output_complex_decl() {
    check(
        parse,
        "output complex c;",
        &expect![[r#"
            Stmt [0-17]:
                annotations: <empty>
                kind: IODeclaration [0-17]:
                    io_keyword: output
                    type: ScalarType [7-14]: ComplexType [7-14]:
                        base_size: <none>
                    ident: Ident [15-16] "c""#]],
    );
}

#[test]
fn input_complex_sized_decl() {
    check(
        parse,
        "input complex[float[32]] c;",
        &expect![[r#"
            Stmt [0-27]:
                annotations: <empty>
                kind: IODeclaration [0-27]:
                    io_keyword: input
                    type: ScalarType [6-24]: ComplexType [6-24]:
                        base_size: FloatType [14-23]:
                            size: Expr [20-22]: Lit: Int(32)
                    ident: Ident [25-26] "c""#]],
    );
}

#[test]
fn output_complex_sized_decl() {
    check(
        parse,
        "output complex[float[32]] c;",
        &expect![[r#"
            Stmt [0-28]:
                annotations: <empty>
                kind: IODeclaration [0-28]:
                    io_keyword: output
                    type: ScalarType [7-25]: ComplexType [7-25]:
                        base_size: FloatType [15-24]:
                            size: Expr [21-23]: Lit: Int(32)
                    ident: Ident [26-27] "c""#]],
    );
}

#[test]
fn input_int_decl() {
    check(
        parse,
        "input int i;",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: IODeclaration [0-12]:
                    io_keyword: input
                    type: ScalarType [6-9]: IntType [6-9]:
                        size: <none>
                    ident: Ident [10-11] "i""#]],
    );
}

#[test]
fn output_int_decl() {
    check(
        parse,
        "output int i;",
        &expect![[r#"
            Stmt [0-13]:
                annotations: <empty>
                kind: IODeclaration [0-13]:
                    io_keyword: output
                    type: ScalarType [7-10]: IntType [7-10]:
                        size: <none>
                    ident: Ident [11-12] "i""#]],
    );
}

#[test]
fn input_int_sized_decl() {
    check(
        parse,
        "input int[32] i;",
        &expect![[r#"
            Stmt [0-16]:
                annotations: <empty>
                kind: IODeclaration [0-16]:
                    io_keyword: input
                    type: ScalarType [6-13]: IntType [6-13]:
                        size: Expr [10-12]: Lit: Int(32)
                    ident: Ident [14-15] "i""#]],
    );
}

#[test]
fn output_int_sized_decl() {
    check(
        parse,
        "output int[32] i;",
        &expect![[r#"
            Stmt [0-17]:
                annotations: <empty>
                kind: IODeclaration [0-17]:
                    io_keyword: output
                    type: ScalarType [7-14]: IntType [7-14]:
                        size: Expr [11-13]: Lit: Int(32)
                    ident: Ident [15-16] "i""#]],
    );
}

#[test]
fn input_uint_decl() {
    check(
        parse,
        "input uint i;",
        &expect![[r#"
            Stmt [0-13]:
                annotations: <empty>
                kind: IODeclaration [0-13]:
                    io_keyword: input
                    type: ScalarType [6-10]: UIntType [6-10]:
                        size: <none>
                    ident: Ident [11-12] "i""#]],
    );
}

#[test]
fn output_uint_decl() {
    check(
        parse,
        "output uint i;",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: IODeclaration [0-14]:
                    io_keyword: output
                    type: ScalarType [7-11]: UIntType [7-11]:
                        size: <none>
                    ident: Ident [12-13] "i""#]],
    );
}

#[test]
fn input_uint_sized_decl() {
    check(
        parse,
        "input uint[32] i;",
        &expect![[r#"
            Stmt [0-17]:
                annotations: <empty>
                kind: IODeclaration [0-17]:
                    io_keyword: input
                    type: ScalarType [6-14]: UIntType [6-14]:
                        size: Expr [11-13]: Lit: Int(32)
                    ident: Ident [15-16] "i""#]],
    );
}

#[test]
fn output_uint_sized_decl() {
    check(
        parse,
        "output uint[32] i;",
        &expect![[r#"
            Stmt [0-18]:
                annotations: <empty>
                kind: IODeclaration [0-18]:
                    io_keyword: output
                    type: ScalarType [7-15]: UIntType [7-15]:
                        size: Expr [12-14]: Lit: Int(32)
                    ident: Ident [16-17] "i""#]],
    );
}

#[test]
fn input_float_decl() {
    check(
        parse,
        "input float f;",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: IODeclaration [0-14]:
                    io_keyword: input
                    type: ScalarType [6-11]: FloatType [6-11]:
                        size: <none>
                    ident: Ident [12-13] "f""#]],
    );
}

#[test]
fn output_float_decl() {
    check(
        parse,
        "output float f;",
        &expect![[r#"
            Stmt [0-15]:
                annotations: <empty>
                kind: IODeclaration [0-15]:
                    io_keyword: output
                    type: ScalarType [7-12]: FloatType [7-12]:
                        size: <none>
                    ident: Ident [13-14] "f""#]],
    );
}

#[test]
fn input_float_sized_decl() {
    check(
        parse,
        "input float[32] f;",
        &expect![[r#"
            Stmt [0-18]:
                annotations: <empty>
                kind: IODeclaration [0-18]:
                    io_keyword: input
                    type: ScalarType [6-15]: FloatType [6-15]:
                        size: Expr [12-14]: Lit: Int(32)
                    ident: Ident [16-17] "f""#]],
    );
}

#[test]
fn output_float_sized_decl() {
    check(
        parse,
        "output float[32] f;",
        &expect![[r#"
            Stmt [0-19]:
                annotations: <empty>
                kind: IODeclaration [0-19]:
                    io_keyword: output
                    type: ScalarType [7-16]: FloatType [7-16]:
                        size: Expr [13-15]: Lit: Int(32)
                    ident: Ident [17-18] "f""#]],
    );
}

#[test]
fn input_angle_decl() {
    check(
        parse,
        "input angle a;",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: IODeclaration [0-14]:
                    io_keyword: input
                    type: ScalarType [6-11]: AngleType [6-11]:
                        size: <none>
                    ident: Ident [12-13] "a""#]],
    );
}

#[test]
fn output_angle_decl() {
    check(
        parse,
        "output angle a;",
        &expect![[r#"
            Stmt [0-15]:
                annotations: <empty>
                kind: IODeclaration [0-15]:
                    io_keyword: output
                    type: ScalarType [7-12]: AngleType [7-12]:
                        size: <none>
                    ident: Ident [13-14] "a""#]],
    );
}

#[test]
fn input_angle_sized_decl() {
    check(
        parse,
        "input angle[32] a;",
        &expect![[r#"
            Stmt [0-18]:
                annotations: <empty>
                kind: IODeclaration [0-18]:
                    io_keyword: input
                    type: ScalarType [6-15]: AngleType [6-15]:
                        size: Expr [12-14]: Lit: Int(32)
                    ident: Ident [16-17] "a""#]],
    );
}

#[test]
fn output_angle_sized_decl() {
    check(
        parse,
        "output angle[32] a;",
        &expect![[r#"
            Stmt [0-19]:
                annotations: <empty>
                kind: IODeclaration [0-19]:
                    io_keyword: output
                    type: ScalarType [7-16]: AngleType [7-16]:
                        size: Expr [13-15]: Lit: Int(32)
                    ident: Ident [17-18] "a""#]],
    );
}

#[test]
fn input_duration_decl() {
    check(
        parse,
        "input duration d;",
        &expect![[r#"
            Stmt [0-17]:
                annotations: <empty>
                kind: IODeclaration [0-17]:
                    io_keyword: input
                    type: ScalarType [6-14]: Duration
                    ident: Ident [15-16] "d""#]],
    );
}

#[test]
fn output_duration_decl() {
    check(
        parse,
        "output duration d;",
        &expect![[r#"
            Stmt [0-18]:
                annotations: <empty>
                kind: IODeclaration [0-18]:
                    io_keyword: output
                    type: ScalarType [7-15]: Duration
                    ident: Ident [16-17] "d""#]],
    );
}

#[test]
fn input_stretch_decl() {
    check(
        parse,
        "input stretch s;",
        &expect![[r#"
            Stmt [0-16]:
                annotations: <empty>
                kind: IODeclaration [0-16]:
                    io_keyword: input
                    type: ScalarType [6-13]: Stretch
                    ident: Ident [14-15] "s""#]],
    );
}

#[test]
fn output_stretch_decl() {
    check(
        parse,
        "output stretch s;",
        &expect![[r#"
            Stmt [0-17]:
                annotations: <empty>
                kind: IODeclaration [0-17]:
                    io_keyword: output
                    type: ScalarType [7-14]: Stretch
                    ident: Ident [15-16] "s""#]],
    );
}
