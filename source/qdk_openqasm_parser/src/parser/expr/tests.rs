// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::expr;
use crate::parser::stmt::parse as stmt;
use crate::{
    parser::ast::StmtKind,
    parser::{scan::ParserContext, stmt, tests::check},
};
use expect_test::{Expect, expect};

/// This function checks two things:
///   1. That the input `Expr` is parsed correctly.
///   2. That if we add a semicolon at the end it parses correctly as a `ExprStmt`
///      containing the same `Expr` inside.
fn check_expr(input: &str, expect: &Expect) {
    // Do the usual expect test check.
    check(expr, input, expect);

    // Parse the expr with the expr parser.
    let expr = expr(&mut ParserContext::new(input)).map(Some);

    // Add a semicolon and parser with the stmt parser.
    let expr_stmt = stmt::parse(&mut ParserContext::new(&format!("{input};")));

    // Extract the inner expr.
    let inner_expr = expr_stmt.map(|ok| match *ok.kind {
        StmtKind::ExprStmt(expr) => Some(expr.expr),
        _ => None,
    });

    // Check that they are equal.
    assert_eq!(format!("{expr:?}"), format!("{inner_expr:?}"));
}

#[test]
fn lit_int() {
    check_expr("123", &expect!["Expr [0-3]: Lit: Int(123)"]);
}

#[test]
fn lit_int_underscore() {
    check_expr("123_456", &expect!["Expr [0-7]: Lit: Int(123456)"]);
}

#[test]
fn lit_int_leading_zero() {
    check_expr("0123", &expect!["Expr [0-4]: Lit: Int(123)"]);
}

#[test]
fn lit_int_max() {
    check_expr(
        "9_223_372_036_854_775_807",
        &expect!["Expr [0-25]: Lit: Int(9223372036854775807)"],
    );
}

// NOTE: Since we need to support literals of value i64::MIN while also parsing the negative sign
// as a unary operator, we need to allow one special case of overflow that is the absolute value
// of i64::MIN. This will wrap to a negative value. See the `lit_int_min` test below.
// To check for other issues with handling i64::MIN, hexadecimal and binary literals
// of i64::MIN also need to be tested.
#[test]
fn lit_int_overflow_min() {
    check_expr(
        "9_223_372_036_854_775_808",
        &expect!["Expr [0-25]: Lit: Int(-9223372036854775808)"],
    );
}

#[test]
fn lit_int_overflow_min_hexadecimal() {
    check_expr(
        "0x8000000000000000",
        &expect!["Expr [0-18]: Lit: Int(-9223372036854775808)"],
    );
}

#[test]
fn lit_int_overflow_min_binary() {
    check_expr(
        "0b1000000000000000000000000000000000000000000000000000000000000000",
        &expect!["Expr [0-66]: Lit: Int(-9223372036854775808)"],
    );
}

#[test]
fn lit_int_too_big_for_i64() {
    check_expr(
        "9_223_372_036_854_775_809",
        &expect!["Expr [0-25]: Lit: BigInt(9223372036854775809)"],
    );
}

#[test]
fn lit_int_too_big_hexadecimal_promotes_to_bigint() {
    check_expr(
        "0x8000000000000001",
        &expect!["Expr [0-18]: Lit: BigInt(9223372036854775809)"],
    );
}

#[test]
fn lit_int_too_big_binary_promotes_to_bigint() {
    check_expr(
        "0b1000000000000000000000000000000000000000000000000000000000000001",
        &expect!["Expr [0-66]: Lit: BigInt(9223372036854775809)"],
    );
}

// NOTE: Since we need to support literals of value i64::MIN while also parsing the negative sign
// as a unary operator, we need to allow one special case of overflow that is the absolute value
// of i64::MIN. This will wrap to a negative value, and then negate of i64::MIN is i64::MIN, so
// the correct value is achieved at runtime.
#[test]
fn lit_int_min() {
    check_expr(
        "-9_223_372_036_854_775_808",
        &expect![[r#"
            Expr [0-26]: UnaryOpExpr:
                op: Neg
                expr: Expr [1-26]: Lit: Int(-9223372036854775808)"#]],
    );
}

#[test]
fn lit_int_hexadecimal() {
    check_expr("0x1a2b3c", &expect!["Expr [0-8]: Lit: Int(1715004)"]);
}

#[test]
fn lit_int_octal() {
    check_expr("0o1234567", &expect!["Expr [0-9]: Lit: Int(342391)"]);
}

#[test]
fn lit_int_binary() {
    check_expr("0b10110", &expect!["Expr [0-7]: Lit: Int(22)"]);
}

#[test]
fn lit_bigint_hexadecimal() {
    check_expr(
        "0x1a2b3c1a2b3c1a2b3c1a",
        &expect!["Expr [0-22]: Lit: BigInt(123579069371309093501978)"],
    );
}

#[test]
fn lit_bigint_hexadecimal_capital_x() {
    check_expr(
        "0X1a2b3c1a2b3c1a2b3c1a",
        &expect!["Expr [0-22]: Lit: BigInt(123579069371309093501978)"],
    );
}

#[test]
fn lit_bigint_octal() {
    check_expr(
        "0o1234567123456712345671234",
        &expect!["Expr [0-27]: Lit: BigInt(6167970861177743307420)"],
    );
}

#[test]
fn lit_bigint_octal_capital_o() {
    check_expr(
        "0O1234567123456712345671234",
        &expect!["Expr [0-27]: Lit: BigInt(6167970861177743307420)"],
    );
}

#[test]
fn lit_bigint_binary() {
    check_expr(
        "0b1011010110101101011010110101101011010110101101011010110101101011",
        &expect!["Expr [0-66]: Lit: BigInt(13091237729729359211)"],
    );
}

#[test]
fn lit_bigint_binary_capital_b() {
    check_expr(
        "0B1011010110101101011010110101101011010110101101011010110101101011",
        &expect!["Expr [0-66]: Lit: BigInt(13091237729729359211)"],
    );
}

#[test]
fn lit_float() {
    check_expr("1.23", &expect!["Expr [0-4]: Lit: Float(1.23)"]);
}

#[test]
fn lit_float_leading_dot() {
    check_expr(".23", &expect!["Expr [0-3]: Lit: Float(0.23)"]);
}

#[test]
fn lit_float_trailing_dot() {
    check_expr("1.", &expect!["Expr [0-2]: Lit: Float(1.0)"]);
}

#[test]
fn lit_float_underscore() {
    check_expr("123_456.78", &expect!["Expr [0-10]: Lit: Float(123456.78)"]);
}

#[test]
fn lit_float_leading_zero() {
    check_expr("0.23", &expect!["Expr [0-4]: Lit: Float(0.23)"]);
}

#[test]
fn lit_float_trailing_exp_0() {
    check_expr(
        "0e",
        &expect![[r#"
            Error(
                Lit(
                    "floating-point",
                    Span {
                        lo: 0,
                        hi: 2,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn lit_float_trailing_exp_1() {
    check_expr(
        "1e",
        &expect![[r#"
            Error(
                Lit(
                    "floating-point",
                    Span {
                        lo: 0,
                        hi: 2,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn lit_float_trailing_dot_trailing_exp() {
    check_expr(
        "1.e",
        &expect![[r#"
            Error(
                Lit(
                    "floating-point",
                    Span {
                        lo: 0,
                        hi: 3,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn lit_float_dot_trailing_exp() {
    check_expr(
        "1.2e",
        &expect![[r#"
            Error(
                Lit(
                    "floating-point",
                    Span {
                        lo: 0,
                        hi: 4,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn lit_float_trailing_exp_dot() {
    check_expr(
        "1e.",
        &expect![[r#"
            Error(
                Lit(
                    "floating-point",
                    Span {
                        lo: 0,
                        hi: 2,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn lit_int_hexadecimal_dot() {
    check_expr("0x123.45", &expect!["Expr [0-5]: Lit: Int(291)"]);
}

#[test]
fn lit_string() {
    check_expr(r#""foo""#, &expect![[r#"Expr [0-5]: Lit: String("foo")"#]]);
}

#[test]
fn lit_string_single_quote() {
    check_expr(r#"'foo'"#, &expect![[r#"Expr [0-5]: Lit: String("foo")"#]]);
}

#[test]
fn lit_string_escape_quote() {
    check_expr(
        r#""foo\"bar""#,
        &expect![[r#"Expr [0-10]: Lit: String("foo\"bar")"#]],
    );
}

#[test]
fn lit_string_single_quote_escape_double_quote() {
    check_expr(
        r#"'foo\"bar'"#,
        &expect![[r#"Expr [0-10]: Lit: String("foo\"bar")"#]],
    );
}

#[test]
fn lit_string_escape_backslash() {
    check_expr(r#""\\""#, &expect![[r#"Expr [0-4]: Lit: String("\\")"#]]);
}

#[test]
fn lit_string_single_quote_escape_backslash() {
    check_expr(r#"'\\'"#, &expect![[r#"Expr [0-4]: Lit: String("\\")"#]]);
}

#[test]
fn lit_string_escape_newline() {
    check_expr(r#""\n""#, &expect![[r#"Expr [0-4]: Lit: String("\n")"#]]);
}

#[test]
fn lit_string_single_quote_escape_newline() {
    check_expr(r#"'\n'"#, &expect![[r#"Expr [0-4]: Lit: String("\n")"#]]);
}

#[test]
fn lit_string_escape_carriage_return() {
    check_expr(r#""\r""#, &expect![[r#"Expr [0-4]: Lit: String("\r")"#]]);
}

#[test]
fn lit_string_single_quote_escape_carriage_return() {
    check_expr(r#"'\r'"#, &expect![[r#"Expr [0-4]: Lit: String("\r")"#]]);
}

#[test]
fn lit_string_escape_tab() {
    check_expr(r#""\t""#, &expect![[r#"Expr [0-4]: Lit: String("\t")"#]]);
}

#[test]
fn lit_string_single_quote_escape_tab() {
    check_expr(r#"'\t'"#, &expect![[r#"Expr [0-4]: Lit: String("\t")"#]]);
}

#[test]
fn lit_string_unknown_escape() {
    check_expr(
        r#""\x""#,
        &expect![[r#"
            Error(
                Escape(
                    'x',
                    Span {
                        lo: 2,
                        hi: 3,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn lit_string_unmatched_quote() {
    check(
        expr,
        r#""Uh oh.."#,
        &expect![[r#"
            Error(
                Rule(
                    "expression",
                    Eof,
                    Span {
                        lo: 8,
                        hi: 8,
                    },
                ),
            )

            [
                Error(
                    Lex(
                        UnterminatedString(
                            Span {
                                lo: 0,
                                hi: 0,
                            },
                        ),
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn lit_string_empty() {
    check_expr(r#""""#, &expect![[r#"Expr [0-2]: Lit: String("")"#]]);
}

#[test]
fn lit_false() {
    check_expr("false", &expect!["Expr [0-5]: Lit: Bool(false)"]);
}

#[test]
fn lit_true() {
    check_expr("true", &expect!["Expr [0-4]: Lit: Bool(true)"]);
}

#[test]
fn lit_bitstring() {
    check_expr(
        r#""101010101""#,
        &expect![[r#"Expr [0-11]: Lit: Bitstring("101010101")"#]],
    );
}

#[test]
fn lit_bitstring_preserves_leading_zeroes() {
    check_expr(
        r#""00011000""#,
        &expect![[r#"Expr [0-10]: Lit: Bitstring("00011000")"#]],
    );
}

#[test]
fn lit_bitstring_separators() {
    check_expr(
        r#""10_10_10_101""#,
        &expect![[r#"Expr [0-14]: Lit: Bitstring("101010101")"#]],
    );
}

#[test]
fn lit_bitstring_unmatched_quote() {
    check(
        expr,
        r#""101010101"#,
        &expect![[r#"
            Error(
                Rule(
                    "expression",
                    Eof,
                    Span {
                        lo: 10,
                        hi: 10,
                    },
                ),
            )

            [
                Error(
                    Lex(
                        UnterminatedString(
                            Span {
                                lo: 0,
                                hi: 0,
                            },
                        ),
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn lit_float_imag() {
    check_expr(r#"10.3im"#, &expect!["Expr [0-6]: Lit: Imaginary(10.3)"]);
}

#[test]
fn lit_float_imag_with_spacing() {
    check_expr(r#"10.3  im"#, &expect!["Expr [0-8]: Lit: Imaginary(10.3)"]);
}

#[test]
fn lit_int_imag() {
    check_expr(r#"10im"#, &expect!["Expr [0-4]: Lit: Imaginary(10.0)"]);
}

#[test]
fn lit_int_imag_with_spacing() {
    check_expr(r#"10  im"#, &expect!["Expr [0-6]: Lit: Imaginary(10.0)"]);
}

#[test]
fn lit_float_imag_leading_dot() {
    check_expr(".23im", &expect!["Expr [0-5]: Lit: Imaginary(0.23)"]);
}

#[test]
fn lit_float_imag_trailing_dot() {
    check_expr("1.im", &expect!["Expr [0-4]: Lit: Imaginary(1.0)"]);
}

#[test]
fn lit_float_imag_underscore() {
    check_expr(
        "123_456.78im",
        &expect!["Expr [0-12]: Lit: Imaginary(123456.78)"],
    );
}

#[test]
fn lit_float_imag_leading_zero() {
    check_expr("0.23im", &expect!["Expr [0-6]: Lit: Imaginary(0.23)"]);
}

#[test]
fn pratt_parsing_binary_expr() {
    check_expr(
        "1 + 2",
        &expect![[r#"
            Expr [0-5]: BinaryOpExpr:
                op: Add
                lhs: Expr [0-1]: Lit: Int(1)
                rhs: Expr [4-5]: Lit: Int(2)"#]],
    );
}

#[test]
fn pratt_parsing_mul_add() {
    check_expr(
        "1 + 2 * 3",
        &expect![[r#"
            Expr [0-9]: BinaryOpExpr:
                op: Add
                lhs: Expr [0-1]: Lit: Int(1)
                rhs: Expr [4-9]: BinaryOpExpr:
                    op: Mul
                    lhs: Expr [4-5]: Lit: Int(2)
                    rhs: Expr [8-9]: Lit: Int(3)"#]],
    );
}

#[test]
fn pratt_parsing_parens() {
    check_expr(
        "(1 + 2) * 3",
        &expect![[r#"
            Expr [0-11]: BinaryOpExpr:
                op: Mul
                lhs: Expr [0-7]: Paren Expr [1-6]: BinaryOpExpr:
                    op: Add
                    lhs: Expr [1-2]: Lit: Int(1)
                    rhs: Expr [5-6]: Lit: Int(2)
                rhs: Expr [10-11]: Lit: Int(3)"#]],
    );
}

#[test]
fn prat_parsing_mul_unary() {
    check_expr(
        "2 * -3",
        &expect![[r#"
            Expr [0-6]: BinaryOpExpr:
                op: Mul
                lhs: Expr [0-1]: Lit: Int(2)
                rhs: Expr [4-6]: UnaryOpExpr:
                    op: Neg
                    expr: Expr [5-6]: Lit: Int(3)"#]],
    );
}

#[test]
fn prat_parsing_unary_mul() {
    check_expr(
        "-2 * 3",
        &expect![[r#"
            Expr [0-6]: BinaryOpExpr:
                op: Mul
                lhs: Expr [0-2]: UnaryOpExpr:
                    op: Neg
                    expr: Expr [1-2]: Lit: Int(2)
                rhs: Expr [5-6]: Lit: Int(3)"#]],
    );
}

#[test]
fn prat_parsing_exp_funcall() {
    check_expr(
        "2 ** square(3)",
        &expect![[r#"
            Expr [0-14]: BinaryOpExpr:
                op: Exp
                lhs: Expr [0-1]: Lit: Int(2)
                rhs: Expr [5-14]: FunctionCall [5-14]:
                    name: Ident [5-11] "square"
                    args:
                        Expr [12-13]: Lit: Int(3)"#]],
    );
}

#[test]
fn prat_parsing_funcall_exp() {
    check_expr(
        "square(2) ** 3",
        &expect![[r#"
            Expr [0-14]: BinaryOpExpr:
                op: Exp
                lhs: Expr [0-9]: FunctionCall [0-9]:
                    name: Ident [0-6] "square"
                    args:
                        Expr [7-8]: Lit: Int(2)
                rhs: Expr [13-14]: Lit: Int(3)"#]],
    );
}

#[test]
fn prat_parsing_funcall_exp_arg() {
    check_expr(
        "square(2 ** 3)",
        &expect![[r#"
            Expr [0-14]: FunctionCall [0-14]:
                name: Ident [0-6] "square"
                args:
                    Expr [7-13]: BinaryOpExpr:
                        op: Exp
                        lhs: Expr [7-8]: Lit: Int(2)
                        rhs: Expr [12-13]: Lit: Int(3)"#]],
    );
}

#[test]
fn funcall() {
    check_expr(
        "square(2)",
        &expect![[r#"
            Expr [0-9]: FunctionCall [0-9]:
                name: Ident [0-6] "square"
                args:
                    Expr [7-8]: Lit: Int(2)"#]],
    );
}

#[test]
fn funcall_multiple_args() {
    check_expr(
        "square(2, 3)",
        &expect![[r#"
            Expr [0-12]: FunctionCall [0-12]:
                name: Ident [0-6] "square"
                args:
                    Expr [7-8]: Lit: Int(2)
                    Expr [10-11]: Lit: Int(3)"#]],
    );
}

#[test]
fn funcall_multiple_args_trailing_comma() {
    check_expr(
        "square(2, 3,)",
        &expect![[r#"
            Expr [0-13]: FunctionCall [0-13]:
                name: Ident [0-6] "square"
                args:
                    Expr [7-8]: Lit: Int(2)
                    Expr [10-11]: Lit: Int(3)"#]],
    );
}

#[test]
fn cast_to_bit() {
    check_expr(
        "bit(0)",
        &expect![[r#"
            Expr [0-6]: Cast [0-6]:
                type: ScalarType [0-3]: BitType [0-3]:
                    size: <none>
                arg: Expr [4-5]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_bit_with_designator() {
    check_expr(
        "bit[4](0)",
        &expect![[r#"
            Expr [0-9]: Cast [0-9]:
                type: ScalarType [0-6]: BitType [0-6]:
                    size: Expr [4-5]: Lit: Int(4)
                arg: Expr [7-8]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_int() {
    check_expr(
        "int(0)",
        &expect![[r#"
            Expr [0-6]: Cast [0-6]:
                type: ScalarType [0-3]: IntType [0-3]:
                    size: <none>
                arg: Expr [4-5]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_int_with_designator() {
    check_expr(
        "int[64](0)",
        &expect![[r#"
            Expr [0-10]: Cast [0-10]:
                type: ScalarType [0-7]: IntType [0-7]:
                    size: Expr [4-6]: Lit: Int(64)
                arg: Expr [8-9]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_uint() {
    check_expr(
        "uint(0)",
        &expect![[r#"
            Expr [0-7]: Cast [0-7]:
                type: ScalarType [0-4]: UIntType [0-4]:
                    size: <none>
                arg: Expr [5-6]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_uint_with_designator() {
    check_expr(
        "uint[64](0)",
        &expect![[r#"
            Expr [0-11]: Cast [0-11]:
                type: ScalarType [0-8]: UIntType [0-8]:
                    size: Expr [5-7]: Lit: Int(64)
                arg: Expr [9-10]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_float() {
    check_expr(
        "float(0)",
        &expect![[r#"
            Expr [0-8]: Cast [0-8]:
                type: ScalarType [0-5]: FloatType [0-5]:
                    size: <none>
                arg: Expr [6-7]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_float_with_designator() {
    check_expr(
        "float[64](0)",
        &expect![[r#"
            Expr [0-12]: Cast [0-12]:
                type: ScalarType [0-9]: FloatType [0-9]:
                    size: Expr [6-8]: Lit: Int(64)
                arg: Expr [10-11]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_complex() {
    check_expr(
        "complex[float](0)",
        &expect![[r#"
            Expr [0-17]: Cast [0-17]:
                type: ScalarType [0-14]: ComplexType [0-14]:
                    base_size: FloatType [8-13]:
                        size: <none>
                arg: Expr [15-16]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_complex_with_designator() {
    check_expr(
        "complex[float[64]](0)",
        &expect![[r#"
            Expr [0-21]: Cast [0-21]:
                type: ScalarType [0-18]: ComplexType [0-18]:
                    base_size: FloatType [8-17]:
                        size: Expr [14-16]: Lit: Int(64)
                arg: Expr [19-20]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_bool() {
    check_expr(
        "bool(0)",
        &expect![[r#"
            Expr [0-7]: Cast [0-7]:
                type: ScalarType [0-4]: BoolType
                arg: Expr [5-6]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_duration() {
    check_expr(
        "duration(0)",
        &expect![[r#"
            Expr [0-11]: Cast [0-11]:
                type: ScalarType [0-8]: Duration
                arg: Expr [9-10]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_stretch() {
    check_expr(
        "stretch(0)",
        &expect![[r#"
            Expr [0-10]: Cast [0-10]:
                type: ScalarType [0-7]: Stretch
                arg: Expr [8-9]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_int_array() {
    check_expr(
        "array[int[64], 4](0)",
        &expect![[r#"
            Expr [0-20]: Cast [0-20]:
                type: ArrayType [0-17]:
                    base_type: ArrayBaseTypeKind IntType [6-13]:
                        size: Expr [10-12]: Lit: Int(64)
                    dimensions:
                        Expr [15-16]: Lit: Int(4)
                arg: Expr [18-19]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_uint_array() {
    check_expr(
        "array[uint[64], 4](0)",
        &expect![[r#"
            Expr [0-21]: Cast [0-21]:
                type: ArrayType [0-18]:
                    base_type: ArrayBaseTypeKind UIntType [6-14]:
                        size: Expr [11-13]: Lit: Int(64)
                    dimensions:
                        Expr [16-17]: Lit: Int(4)
                arg: Expr [19-20]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_float_array() {
    check_expr(
        "array[float[64], 4](0)",
        &expect![[r#"
            Expr [0-22]: Cast [0-22]:
                type: ArrayType [0-19]:
                    base_type: ArrayBaseTypeKind FloatType [6-15]:
                        size: Expr [12-14]: Lit: Int(64)
                    dimensions:
                        Expr [17-18]: Lit: Int(4)
                arg: Expr [20-21]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_angle_array() {
    check_expr(
        "array[angle[64], 4](0)",
        &expect![[r#"
            Expr [0-22]: Cast [0-22]:
                type: ArrayType [0-19]:
                    base_type: ArrayBaseTypeKind AngleType [6-15]:
                        size: Expr [12-14]: Lit: Int(64)
                    dimensions:
                        Expr [17-18]: Lit: Int(4)
                arg: Expr [20-21]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_bool_array() {
    check_expr(
        "array[bool, 4](0)",
        &expect![[r#"
            Expr [0-17]: Cast [0-17]:
                type: ArrayType [0-14]:
                    base_type: ArrayBaseTypeKind BoolType
                    dimensions:
                        Expr [12-13]: Lit: Int(4)
                arg: Expr [15-16]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_duration_array() {
    check_expr(
        "array[duration, 4](0)",
        &expect![[r#"
            Expr [0-21]: Cast [0-21]:
                type: ArrayType [0-18]:
                    base_type: ArrayBaseTypeKind DurationType
                    dimensions:
                        Expr [16-17]: Lit: Int(4)
                arg: Expr [19-20]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_to_complex_array() {
    check_expr(
        "array[complex[float[32]], 4](0)",
        &expect![[r#"
            Expr [0-31]: Cast [0-31]:
                type: ArrayType [0-28]:
                    base_type: ArrayBaseTypeKind ComplexType [6-24]:
                        base_size: FloatType [14-23]:
                            size: Expr [20-22]: Lit: Int(32)
                    dimensions:
                        Expr [26-27]: Lit: Int(4)
                arg: Expr [29-30]: Lit: Int(0)"#]],
    );
}

#[test]
fn index_expr() {
    check_expr(
        "foo[1]",
        &expect![[r#"
            Expr [0-6]: IndexExpr [0-6]:
                collection: Expr [0-3]: Ident [0-3] "foo"
                index: IndexList [4-5]:
                    values:
                        Expr [4-5]: Lit: Int(1)"#]],
    );
}

#[test]
fn index_set() {
    check_expr(
        "foo[{1, 4, 5}]",
        &expect![[r#"
            Expr [0-14]: IndexExpr [0-14]:
                collection: Expr [0-3]: Ident [0-3] "foo"
                index: Set [4-13]:
                    values:
                        Expr [5-6]: Lit: Int(1)
                        Expr [8-9]: Lit: Int(4)
                        Expr [11-12]: Lit: Int(5)"#]],
    );
}

#[test]
fn index_multiple_ranges() {
    check_expr(
        "foo[1:5, 3:7, 4:8]",
        &expect![[r#"
            Expr [0-18]: IndexExpr [0-18]:
                collection: Expr [0-3]: Ident [0-3] "foo"
                index: IndexList [4-17]:
                    values:
                        Range [4-7]:
                            start: Expr [4-5]: Lit: Int(1)
                            step: <none>
                            end: Expr [6-7]: Lit: Int(5)
                        Range [9-12]:
                            start: Expr [9-10]: Lit: Int(3)
                            step: <none>
                            end: Expr [11-12]: Lit: Int(7)
                        Range [14-17]:
                            start: Expr [14-15]: Lit: Int(4)
                            step: <none>
                            end: Expr [16-17]: Lit: Int(8)"#]],
    );
}

#[test]
fn index_range() {
    check_expr(
        "foo[1:5:2]",
        &expect![[r#"
            Expr [0-10]: IndexExpr [0-10]:
                collection: Expr [0-3]: Ident [0-3] "foo"
                index: IndexList [4-9]:
                    values:
                        Range [4-9]:
                            start: Expr [4-5]: Lit: Int(1)
                            step: Expr [6-7]: Lit: Int(5)
                            end: Expr [8-9]: Lit: Int(2)"#]],
    );
}

#[test]
fn index_full_range() {
    check_expr(
        "foo[:]",
        &expect![[r#"
            Expr [0-6]: IndexExpr [0-6]:
                collection: Expr [0-3]: Ident [0-3] "foo"
                index: IndexList [4-5]:
                    values:
                        Range [4-5]:
                            start: <none>
                            step: <none>
                            end: <none>"#]],
    );
}

#[test]
fn index_range_start() {
    check_expr(
        "foo[1:]",
        &expect![[r#"
            Expr [0-7]: IndexExpr [0-7]:
                collection: Expr [0-3]: Ident [0-3] "foo"
                index: IndexList [4-6]:
                    values:
                        Range [4-6]:
                            start: Expr [4-5]: Lit: Int(1)
                            step: <none>
                            end: <none>"#]],
    );
}

#[test]
fn index_range_end() {
    check_expr(
        "foo[:5]",
        &expect![[r#"
            Expr [0-7]: IndexExpr [0-7]:
                collection: Expr [0-3]: Ident [0-3] "foo"
                index: IndexList [4-6]:
                    values:
                        Range [4-6]:
                            start: <none>
                            step: <none>
                            end: Expr [5-6]: Lit: Int(5)"#]],
    );
}

#[test]
fn index_range_step() {
    check_expr(
        "foo[:2:]",
        &expect![[r#"
            Expr [0-8]: IndexExpr [0-8]:
                collection: Expr [0-3]: Ident [0-3] "foo"
                index: IndexList [4-7]:
                    values:
                        Range [4-7]:
                            start: <none>
                            step: Expr [5-6]: Lit: Int(2)
                            end: <none>"#]],
    );
}

#[test]
fn set_expr() {
    check(
        super::set_expr,
        "{2, 3, 4}",
        &expect![[r#"
            Set [0-9]:
                values:
                    Expr [1-2]: Lit: Int(2)
                    Expr [4-5]: Lit: Int(3)
                    Expr [7-8]: Lit: Int(4)"#]],
    );
}

#[test]
fn lit_array() {
    check(
        super::lit_array,
        "{{2, {5}}, 1 + z}",
        &expect![[r#"
            Expr [0-17]: Lit:     Array:
                    Expr [1-9]: Lit:     Array:
                            Expr [2-3]: Lit: Int(2)
                            Expr [5-8]: Lit:     Array:
                                    Expr [6-7]: Lit: Int(5)
                    Expr [11-16]: BinaryOpExpr:
                        op: Add
                        lhs: Expr [11-12]: Lit: Int(1)
                        rhs: Expr [15-16]: Ident [15-16] "z""#]],
    );
}

#[test]
fn hardware_qubit() {
    check(
        super::hardware_qubit,
        "$12",
        &expect!["HardwareQubit [0-3]: 12"],
    );
}

#[test]
fn indexed_identifier() {
    check(
        super::ident_or_indexed_ident,
        "arr[1][2]",
        &expect![[r#"
            IndexedIdent [0-9]:
                ident: Ident [0-3] "arr"
                index_span: [3-9]
                indices:
                    IndexList [4-5]:
                        values:
                            Expr [4-5]: Lit: Int(1)
                    IndexList [7-8]:
                        values:
                            Expr [7-8]: Lit: Int(2)"#]],
    );
}

#[test]
fn measure_hardware_qubit() {
    check(
        super::measure_expr,
        "measure $12",
        &expect![[r#"
            MeasureExpr [0-11]:
                operand: GateOperand [8-11]:
                    kind: HardwareQubit [8-11]: 12"#]],
    );
}

#[test]
fn measure_indexed_identifier() {
    check(
        super::measure_expr,
        "measure qubits[1][2]",
        &expect![[r#"
            MeasureExpr [0-20]:
                operand: GateOperand [8-20]:
                    kind: IndexedIdent [8-20]:
                        ident: Ident [8-14] "qubits"
                        index_span: [14-20]
                        indices:
                            IndexList [15-16]:
                                values:
                                    Expr [15-16]: Lit: Int(1)
                            IndexList [18-19]:
                                values:
                                    Expr [18-19]: Lit: Int(2)"#]],
    );
}

#[test]
fn addition_of_casts() {
    check_expr(
        "bit(0) + bit(1)",
        &expect![[r#"
            Expr [0-15]: BinaryOpExpr:
                op: Add
                lhs: Expr [0-6]: Cast [0-6]:
                    type: ScalarType [0-3]: BitType [0-3]:
                        size: <none>
                    arg: Expr [4-5]: Lit: Int(0)
                rhs: Expr [9-15]: Cast [9-15]:
                    type: ScalarType [9-12]: BitType [9-12]:
                        size: <none>
                    arg: Expr [13-14]: Lit: Int(1)"#]],
    );
}

#[test]
fn duration_of() {
    check_expr(
        "durationof({x [25ms] $0;})",
        &expect![[r#"
            Expr [0-26]: DurationofCall [0-26]:
                name_span: [0-10]
                scope: Block [11-25]:
                    Stmt [12-24]:
                        annotations: <empty>
                        kind: GateCall [12-24]:
                            modifiers: <empty>
                            name: Ident [12-13] "x"
                            args: <empty>
                            duration: Expr [15-19]: Lit: Duration(25.0, Ms)
                            qubits:
                                GateOperand [21-23]:
                                    kind: HardwareQubit [21-23]: 0"#]],
    );
}

#[test]
fn array_concatenation_is_not_part_of_the_expr_tree() {
    let source = "a ++ b;";

    check(
        stmt,
        source,
        &expect![[r#"
        Stmt [0-1]:
            annotations: <empty>
            kind: ExprStmt [0-1]:
                expr: Expr [0-1]: Ident [0-1] "a"

        [
            Error(
                Token(
                    Semicolon,
                    PlusPlus,
                    Span {
                        lo: 2,
                        hi: 4,
                    },
                ),
            ),
        ]"#]],
    );
}
