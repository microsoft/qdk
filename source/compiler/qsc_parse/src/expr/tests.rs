// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::expr;
use crate::tests::check;
use expect_test::expect;

#[test]
fn lit_int() {
    check(expr, "123", &expect!["Expr _id_ [0-3]: Lit: Int(123)"]);
}

#[test]
fn lit_int_underscore() {
    check(
        expr,
        "123_456",
        &expect!["Expr _id_ [0-7]: Lit: Int(123456)"],
    );
}

#[test]
fn lit_int_leading_zero() {
    check(expr, "0123", &expect!["Expr _id_ [0-4]: Lit: Int(123)"]);
}

#[test]
fn lit_int_max() {
    check(
        expr,
        "9_223_372_036_854_775_807",
        &expect!["Expr _id_ [0-25]: Lit: Int(9223372036854775807)"],
    );
}

// NOTE: Since we need to support literals of value i64::MIN while also parsing the negative sign
// as a unary operator, we need to allow one special case of overflow that is the absolute value
// of i64::MIN. This will wrap to a negative value. See the `lit_int_min` test below.
// To check for other issues with handling i64::MIN, hexadecimal and binary literals
// of i64::MIN also need to be tested.
#[test]
fn lit_int_overflow_min() {
    check(
        expr,
        "9_223_372_036_854_775_808",
        &expect!["Expr _id_ [0-25]: Lit: Int(-9223372036854775808)"],
    );
}

#[test]
fn lit_int_overflow_min_hexadecimal() {
    check(
        expr,
        "0x8000000000000000",
        &expect!["Expr _id_ [0-18]: Lit: Int(-9223372036854775808)"],
    );
}

#[test]
fn lit_int_overflow_min_binary() {
    check(
        expr,
        "0b1000000000000000000000000000000000000000000000000000000000000000",
        &expect!["Expr _id_ [0-66]: Lit: Int(-9223372036854775808)"],
    );
}

#[test]
fn lit_int_too_big() {
    check(
        expr,
        "9_223_372_036_854_775_809",
        &expect![[r#"
            Error(
                Lit(
                    "integer",
                    Span {
                        lo: 0,
                        hi: 25,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn lit_int_too_big_hexadecimal() {
    check(
        expr,
        "0x8000000000000001",
        &expect![[r#"
            Error(
                Lit(
                    "integer",
                    Span {
                        lo: 0,
                        hi: 18,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn lit_int_too_big_binary() {
    check(
        expr,
        "0b1000000000000000000000000000000000000000000000000000000000000001",
        &expect![[r#"
            Error(
                Lit(
                    "integer",
                    Span {
                        lo: 0,
                        hi: 66,
                    },
                ),
            )
        "#]],
    );
}

// NOTE: Since we need to support literals of value i64::MIN while also parsing the negative sign
// as a unary operator, we need to allow one special case of overflow that is the absolute value
// of i64::MIN. This will wrap to a negative value, and then negate of i64::MIN is i64::MIN, so
// the correct value is achieved at runtime.
#[test]
fn lit_int_min() {
    check(
        expr,
        "-9_223_372_036_854_775_808",
        &expect![[r#"
            Expr _id_ [0-26]: UnOp (Neg):
                Expr _id_ [1-26]: Lit: Int(-9223372036854775808)"#]],
    );
}

#[test]
fn lit_int_hexadecimal() {
    check(
        expr,
        "0x1a2b3c",
        &expect!["Expr _id_ [0-8]: Lit: Int(1715004)"],
    );
}

#[test]
fn lit_int_octal() {
    check(
        expr,
        "0o1234567",
        &expect!["Expr _id_ [0-9]: Lit: Int(342391)"],
    );
}

#[test]
fn lit_int_binary() {
    check(expr, "0b10110", &expect!["Expr _id_ [0-7]: Lit: Int(22)"]);
}

#[test]
fn lit_bigint() {
    check(expr, "123L", &expect!["Expr _id_ [0-4]: Lit: BigInt(123)"]);
}

#[test]
fn lit_bigint_underscore() {
    check(
        expr,
        "123_456L",
        &expect!["Expr _id_ [0-8]: Lit: BigInt(123456)"],
    );
}

#[test]
fn lit_bigint_hexadecimal() {
    check(
        expr,
        "0x1a2b3cL",
        &expect!["Expr _id_ [0-9]: Lit: BigInt(1715004)"],
    );
}

#[test]
fn lit_bigint_octal() {
    check(
        expr,
        "0o1234567L",
        &expect!["Expr _id_ [0-10]: Lit: BigInt(342391)"],
    );
}

#[test]
fn lit_bigint_binary() {
    check(
        expr,
        "0b10110L",
        &expect!["Expr _id_ [0-8]: Lit: BigInt(22)"],
    );
}

#[test]
fn lit_double() {
    check(expr, "1.23", &expect!["Expr _id_ [0-4]: Lit: Double(1.23)"]);
}

#[test]
fn lit_double_leading_dot() {
    check(
        expr,
        ".23",
        &expect![[r#"
            Error(
                Rule(
                    "expression",
                    Dot,
                    Span {
                        lo: 0,
                        hi: 1,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn lit_double_trailing_dot() {
    check(expr, "1.", &expect!["Expr _id_ [0-2]: Lit: Double(1)"]);
}

#[test]
fn lit_double_underscore() {
    check(
        expr,
        "123_456.78",
        &expect!["Expr _id_ [0-10]: Lit: Double(123456.78)"],
    );
}

#[test]
fn lit_double_leading_zero() {
    check(expr, "0.23", &expect!["Expr _id_ [0-4]: Lit: Double(0.23)"]);
}

#[test]
fn lit_double_trailing_exp_0() {
    check(
        expr,
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
fn lit_double_trailing_exp_1() {
    check(
        expr,
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
fn lit_double_trailing_dot_trailing_exp() {
    check(
        expr,
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
fn lit_double_dot_trailing_exp() {
    check(
        expr,
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
fn lit_double_trailing_exp_dot() {
    check(
        expr,
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
    check(
        expr,
        "0x123.45",
        &expect![[r#"
            Expr _id_ [0-6]: Field:
                Expr _id_ [0-5]: Lit: Int(291)
                Err

            [
                Error(
                    Rule(
                        "identifier",
                        Int(
                            Decimal,
                        ),
                        Span {
                            lo: 6,
                            hi: 8,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn lit_string() {
    check(
        expr,
        r#""foo""#,
        &expect![[r#"Expr _id_ [0-5]: Lit: String("foo")"#]],
    );
}

#[test]
fn lit_string_escape_quote() {
    check(
        expr,
        r#""foo\"bar""#,
        &expect![[r#"Expr _id_ [0-10]: Lit: String("foo\"bar")"#]],
    );
}

#[test]
fn lit_string_escape_backslash() {
    check(
        expr,
        r#""\\""#,
        &expect![[r#"Expr _id_ [0-4]: Lit: String("\\")"#]],
    );
}

#[test]
fn lit_string_escape_newline() {
    check(
        expr,
        r#""\n""#,
        &expect![[r#"Expr _id_ [0-4]: Lit: String("\n")"#]],
    );
}

#[test]
fn lit_string_escape_carriage_return() {
    check(
        expr,
        r#""\r""#,
        &expect![[r#"Expr _id_ [0-4]: Lit: String("\r")"#]],
    );
}

#[test]
fn lit_string_escape_tab() {
    check(
        expr,
        r#""\t""#,
        &expect![[r#"Expr _id_ [0-4]: Lit: String("\t")"#]],
    );
}

#[test]
fn lit_string_unknown_escape() {
    check(
        expr,
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
    check(
        expr,
        r#""""#,
        &expect![[r#"Expr _id_ [0-2]: Lit: String("")"#]],
    );
}

#[test]
fn lit_false() {
    check(expr, "false", &expect!["Expr _id_ [0-5]: Lit: Bool(false)"]);
}

#[test]
fn lit_true() {
    check(expr, "true", &expect!["Expr _id_ [0-4]: Lit: Bool(true)"]);
}

#[test]
fn lit_zero() {
    check(expr, "Zero", &expect!["Expr _id_ [0-4]: Lit: Result(Zero)"]);
}

#[test]
fn lit_one() {
    check(expr, "One", &expect!["Expr _id_ [0-3]: Lit: Result(One)"]);
}

#[test]
fn lit_pauli_i() {
    check(expr, "PauliI", &expect!["Expr _id_ [0-6]: Lit: Pauli(I)"]);
}

#[test]
fn lit_pauli_x() {
    check(expr, "PauliX", &expect!["Expr _id_ [0-6]: Lit: Pauli(X)"]);
}

#[test]
fn lit_pauli_y() {
    check(expr, "PauliY", &expect!["Expr _id_ [0-6]: Lit: Pauli(Y)"]);
}

#[test]
fn lit_pauli_z() {
    check(expr, "PauliZ", &expect!["Expr _id_ [0-6]: Lit: Pauli(Z)"]);
}

#[test]
fn hole() {
    check(expr, "_", &expect!["Expr _id_ [0-1]: Hole"]);
}

#[test]
fn single_path() {
    check(
        expr,
        "foo",
        &expect![[r#"Expr _id_ [0-3]: Path: Path _id_ [0-3] (Ident _id_ [0-3] "foo")"#]],
    );
}

#[test]
fn double_path() {
    check(
        expr,
        "foo.bar",
        &expect![[r#"
            Expr _id_ [0-7]: Path: Path _id_ [0-7]:
                Ident _id_ [0-3] "foo"
                Ident _id_ [4-7] "bar""#]],
    );
}

#[test]
fn leading_expr_path() {
    check(
        expr,
        "foo().bar",
        &expect![[r#"
            Expr _id_ [0-9]: Field:
                Expr _id_ [0-5]: Call:
                    Expr _id_ [0-3]: Path: Path _id_ [0-3] (Ident _id_ [0-3] "foo")
                    Expr _id_ [3-5]: Unit
                Ident _id_ [6-9] "bar""#]],
    );
}

#[test]
fn fail() {
    check(
        expr,
        r#"fail "message""#,
        &expect![[r#"Expr _id_ [0-14]: Fail: Expr _id_ [5-14]: Lit: String("message")"#]],
    );
}

#[test]
fn for_in() {
    check(
        expr,
        "for x in xs { x }",
        &expect![[r#"
            Expr _id_ [0-17]: For:
                Pat _id_ [4-5]: Bind:
                    Ident _id_ [4-5] "x"
                Expr _id_ [9-11]: Path: Path _id_ [9-11] (Ident _id_ [9-11] "xs")
                Block _id_ [12-17]:
                    Stmt _id_ [14-15]: Expr: Expr _id_ [14-15]: Path: Path _id_ [14-15] (Ident _id_ [14-15] "x")"#]],
    );
}

#[test]
fn if_then() {
    check(
        expr,
        "if c { e }",
        &expect![[r#"
            Expr _id_ [0-10]: If:
                Expr _id_ [3-4]: Path: Path _id_ [3-4] (Ident _id_ [3-4] "c")
                Block _id_ [5-10]:
                    Stmt _id_ [7-8]: Expr: Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "e")"#]],
    );
}

#[test]
fn if_else() {
    check(
        expr,
        "if c { x } else { y }",
        &expect![[r#"
            Expr _id_ [0-21]: If:
                Expr _id_ [3-4]: Path: Path _id_ [3-4] (Ident _id_ [3-4] "c")
                Block _id_ [5-10]:
                    Stmt _id_ [7-8]: Expr: Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "x")
                Expr _id_ [11-21]: Expr Block: Block _id_ [16-21]:
                    Stmt _id_ [18-19]: Expr: Expr _id_ [18-19]: Path: Path _id_ [18-19] (Ident _id_ [18-19] "y")"#]],
    );
}

#[test]
fn if_elif() {
    check(
        expr,
        "if c1 { x } elif c2 { y }",
        &expect![[r#"
            Expr _id_ [0-25]: If:
                Expr _id_ [3-5]: Path: Path _id_ [3-5] (Ident _id_ [3-5] "c1")
                Block _id_ [6-11]:
                    Stmt _id_ [8-9]: Expr: Expr _id_ [8-9]: Path: Path _id_ [8-9] (Ident _id_ [8-9] "x")
                Expr _id_ [12-25]: If:
                    Expr _id_ [17-19]: Path: Path _id_ [17-19] (Ident _id_ [17-19] "c2")
                    Block _id_ [20-25]:
                        Stmt _id_ [22-23]: Expr: Expr _id_ [22-23]: Path: Path _id_ [22-23] (Ident _id_ [22-23] "y")"#]],
    );
}

#[test]
fn if_elif_else() {
    check(
        expr,
        "if c1 { x } elif c2 { y } else { z }",
        &expect![[r#"
            Expr _id_ [0-36]: If:
                Expr _id_ [3-5]: Path: Path _id_ [3-5] (Ident _id_ [3-5] "c1")
                Block _id_ [6-11]:
                    Stmt _id_ [8-9]: Expr: Expr _id_ [8-9]: Path: Path _id_ [8-9] (Ident _id_ [8-9] "x")
                Expr _id_ [12-36]: If:
                    Expr _id_ [17-19]: Path: Path _id_ [17-19] (Ident _id_ [17-19] "c2")
                    Block _id_ [20-25]:
                        Stmt _id_ [22-23]: Expr: Expr _id_ [22-23]: Path: Path _id_ [22-23] (Ident _id_ [22-23] "y")
                    Expr _id_ [26-36]: Expr Block: Block _id_ [31-36]:
                        Stmt _id_ [33-34]: Expr: Expr _id_ [33-34]: Path: Path _id_ [33-34] (Ident _id_ [33-34] "z")"#]],
    );
}

#[test]
fn repeat_until() {
    check(
        expr,
        "repeat { x } until c",
        &expect![[r#"
            Expr _id_ [0-20]: Repeat:
                Block _id_ [7-12]:
                    Stmt _id_ [9-10]: Expr: Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "x")
                Expr _id_ [19-20]: Path: Path _id_ [19-20] (Ident _id_ [19-20] "c")
                <no fixup>"#]],
    );
}

#[test]
fn repeat_until_fixup() {
    check(
        expr,
        "repeat { x } until c fixup { y }",
        &expect![[r#"
            Expr _id_ [0-32]: Repeat:
                Block _id_ [7-12]:
                    Stmt _id_ [9-10]: Expr: Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "x")
                Expr _id_ [19-20]: Path: Path _id_ [19-20] (Ident _id_ [19-20] "c")
                Block _id_ [27-32]:
                    Stmt _id_ [29-30]: Expr: Expr _id_ [29-30]: Path: Path _id_ [29-30] (Ident _id_ [29-30] "y")"#]],
    );
}

#[test]
fn return_expr() {
    check(
        expr,
        "return x",
        &expect![[
            r#"Expr _id_ [0-8]: Return: Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "x")"#
        ]],
    );
}

#[test]
fn set() {
    check(
        expr,
        "set x = y",
        &expect![[r#"
            Expr _id_ [0-9]: Assign:
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [8-9]: Path: Path _id_ [8-9] (Ident _id_ [8-9] "y")"#]],
    );
    check(
        expr,
        "x = y",
        &expect![[r#"
            Expr _id_ [0-5]: Assign:
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")"#]],
    );
}

#[test]
fn set_chained() {
    check(
        expr,
        "set x = set y = z",
        &expect![[r#"
            Expr _id_ [0-17]: Assign:
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [8-17]: Assign:
                    Expr _id_ [12-13]: Path: Path _id_ [12-13] (Ident _id_ [12-13] "y")
                    Expr _id_ [16-17]: Path: Path _id_ [16-17] (Ident _id_ [16-17] "z")"#]],
    );
    check(
        expr,
        "x = y = z",
        &expect![[r#"
            Expr _id_ [0-9]: Assign:
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [4-9]: Assign:
                    Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")
                    Expr _id_ [8-9]: Path: Path _id_ [8-9] (Ident _id_ [8-9] "z")"#]],
    );
}

#[test]
fn set_hole() {
    check(
        expr,
        "set _ = 1",
        &expect![[r#"
            Expr _id_ [0-9]: Assign:
                Expr _id_ [4-5]: Hole
                Expr _id_ [8-9]: Lit: Int(1)"#]],
    );
    check(
        expr,
        "_ = 1",
        &expect![[r#"
            Expr _id_ [0-5]: Assign:
                Expr _id_ [0-1]: Hole
                Expr _id_ [4-5]: Lit: Int(1)"#]],
    );
}

#[test]
fn set_hole_tuple() {
    check(
        expr,
        "set (x, _) = (1, 2)",
        &expect![[r#"
            Expr _id_ [0-19]: Assign:
                Expr _id_ [4-10]: Tuple:
                    Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "x")
                    Expr _id_ [8-9]: Hole
                Expr _id_ [13-19]: Tuple:
                    Expr _id_ [14-15]: Lit: Int(1)
                    Expr _id_ [17-18]: Lit: Int(2)"#]],
    );
    check(
        expr,
        "(x, _) = (1, 2)",
        &expect![[r#"
            Expr _id_ [0-15]: Assign:
                Expr _id_ [0-6]: Tuple:
                    Expr _id_ [1-2]: Path: Path _id_ [1-2] (Ident _id_ [1-2] "x")
                    Expr _id_ [4-5]: Hole
                Expr _id_ [9-15]: Tuple:
                    Expr _id_ [10-11]: Lit: Int(1)
                    Expr _id_ [13-14]: Lit: Int(2)"#]],
    );
}

#[test]
fn set_hole_tuple_nested() {
    check(
        expr,
        "set (_, (x, _)) = (1, (2, 3))",
        &expect![[r#"
            Expr _id_ [0-29]: Assign:
                Expr _id_ [4-15]: Tuple:
                    Expr _id_ [5-6]: Hole
                    Expr _id_ [8-14]: Tuple:
                        Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "x")
                        Expr _id_ [12-13]: Hole
                Expr _id_ [18-29]: Tuple:
                    Expr _id_ [19-20]: Lit: Int(1)
                    Expr _id_ [22-28]: Tuple:
                        Expr _id_ [23-24]: Lit: Int(2)
                        Expr _id_ [26-27]: Lit: Int(3)"#]],
    );
    check(
        expr,
        "(_, (x, _)) = (1, (2, 3))",
        &expect![[r#"
            Expr _id_ [0-25]: Assign:
                Expr _id_ [0-11]: Tuple:
                    Expr _id_ [1-2]: Hole
                    Expr _id_ [4-10]: Tuple:
                        Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "x")
                        Expr _id_ [8-9]: Hole
                Expr _id_ [14-25]: Tuple:
                    Expr _id_ [15-16]: Lit: Int(1)
                    Expr _id_ [18-24]: Tuple:
                        Expr _id_ [19-20]: Lit: Int(2)
                        Expr _id_ [22-23]: Lit: Int(3)"#]],
    );
}

#[test]
fn set_bitwise_and() {
    check(
        expr,
        "set x &&&= y",
        &expect![[r#"
            Expr _id_ [0-12]: AssignOp (AndB):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [11-12]: Path: Path _id_ [11-12] (Ident _id_ [11-12] "y")"#]],
    );
    check(
        expr,
        "x &&&= y",
        &expect![[r#"
            Expr _id_ [0-8]: AssignOp (AndB):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "y")"#]],
    );
}

#[test]
fn set_logical_and() {
    check(
        expr,
        "set x and= y",
        &expect![[r#"
            Expr _id_ [0-12]: AssignOp (AndL):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [11-12]: Path: Path _id_ [11-12] (Ident _id_ [11-12] "y")"#]],
    );
    check(
        expr,
        "x and= y",
        &expect![[r#"
            Expr _id_ [0-8]: AssignOp (AndL):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "y")"#]],
    );
}

#[test]
fn set_bitwise_or() {
    check(
        expr,
        "set x |||= y",
        &expect![[r#"
            Expr _id_ [0-12]: AssignOp (OrB):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [11-12]: Path: Path _id_ [11-12] (Ident _id_ [11-12] "y")"#]],
    );
    check(
        expr,
        "x |||= y",
        &expect![[r#"
            Expr _id_ [0-8]: AssignOp (OrB):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "y")"#]],
    );
}

#[test]
fn set_exp() {
    check(
        expr,
        "set x ^= y",
        &expect![[r#"
            Expr _id_ [0-10]: AssignOp (Exp):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "y")"#]],
    );
    check(
        expr,
        "x ^= y",
        &expect![[r#"
            Expr _id_ [0-6]: AssignOp (Exp):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")"#]],
    );
}

#[test]
fn set_bitwise_xor() {
    check(
        expr,
        "set x ^^^= y",
        &expect![[r#"
            Expr _id_ [0-12]: AssignOp (XorB):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [11-12]: Path: Path _id_ [11-12] (Ident _id_ [11-12] "y")"#]],
    );
    check(
        expr,
        "x ^^^= y",
        &expect![[r#"
            Expr _id_ [0-8]: AssignOp (XorB):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "y")"#]],
    );
}

#[test]
fn set_shr() {
    check(
        expr,
        "set x >>>= y",
        &expect![[r#"
            Expr _id_ [0-12]: AssignOp (Shr):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [11-12]: Path: Path _id_ [11-12] (Ident _id_ [11-12] "y")"#]],
    );
    check(
        expr,
        "x >>>= y",
        &expect![[r#"
            Expr _id_ [0-8]: AssignOp (Shr):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "y")"#]],
    );
}

#[test]
fn set_shl() {
    check(
        expr,
        "set x <<<= y",
        &expect![[r#"
            Expr _id_ [0-12]: AssignOp (Shl):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [11-12]: Path: Path _id_ [11-12] (Ident _id_ [11-12] "y")"#]],
    );
    check(
        expr,
        "x <<<= y",
        &expect![[r#"
            Expr _id_ [0-8]: AssignOp (Shl):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "y")"#]],
    );
}

#[test]
fn set_sub() {
    check(
        expr,
        "set x -= y",
        &expect![[r#"
            Expr _id_ [0-10]: AssignOp (Sub):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "y")"#]],
    );
    check(
        expr,
        "x -= y",
        &expect![[r#"
            Expr _id_ [0-6]: AssignOp (Sub):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")"#]],
    );
}

#[test]
fn set_logical_or() {
    check(
        expr,
        "set x or= y",
        &expect![[r#"
            Expr _id_ [0-11]: AssignOp (OrL):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [10-11]: Path: Path _id_ [10-11] (Ident _id_ [10-11] "y")"#]],
    );
    check(
        expr,
        "x or= y",
        &expect![[r#"
            Expr _id_ [0-7]: AssignOp (OrL):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "y")"#]],
    );
}

#[test]
fn set_mod() {
    check(
        expr,
        "set x %= y",
        &expect![[r#"
            Expr _id_ [0-10]: AssignOp (Mod):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "y")"#]],
    );
    check(
        expr,
        "x %= y",
        &expect![[r#"
            Expr _id_ [0-6]: AssignOp (Mod):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")"#]],
    );
}

#[test]
fn set_add() {
    check(
        expr,
        "set x += y",
        &expect![[r#"
            Expr _id_ [0-10]: AssignOp (Add):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "y")"#]],
    );
    check(
        expr,
        "x += y",
        &expect![[r#"
            Expr _id_ [0-6]: AssignOp (Add):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")"#]],
    );
}

#[test]
fn set_div() {
    check(
        expr,
        "set x /= y",
        &expect![[r#"
            Expr _id_ [0-10]: AssignOp (Div):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "y")"#]],
    );
    check(
        expr,
        "x /= y",
        &expect![[r#"
            Expr _id_ [0-6]: AssignOp (Div):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")"#]],
    );
}

#[test]
fn set_mul() {
    check(
        expr,
        "set x *= y",
        &expect![[r#"
            Expr _id_ [0-10]: AssignOp (Mul):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "y")"#]],
    );
    check(
        expr,
        "x *= y",
        &expect![[r#"
            Expr _id_ [0-6]: AssignOp (Mul):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")"#]],
    );
}

#[test]
fn set_with_update() {
    check(
        expr,
        "set x w/= i <- y",
        &expect![[r#"
            Expr _id_ [0-16]: AssignUpdate:
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                Expr _id_ [10-11]: Path: Path _id_ [10-11] (Ident _id_ [10-11] "i")
                Expr _id_ [15-16]: Path: Path _id_ [15-16] (Ident _id_ [15-16] "y")"#]],
    );
    check(
        expr,
        "x w/= i <- y",
        &expect![[r#"
            Expr _id_ [0-12]: AssignUpdate:
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "i")
                Expr _id_ [11-12]: Path: Path _id_ [11-12] (Ident _id_ [11-12] "y")"#]],
    );
}

#[test]
fn while_expr() {
    check(
        expr,
        "while c { x }",
        &expect![[r#"
            Expr _id_ [0-13]: While:
                Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "c")
                Block _id_ [8-13]:
                    Stmt _id_ [10-11]: Expr: Expr _id_ [10-11]: Path: Path _id_ [10-11] (Ident _id_ [10-11] "x")"#]],
    );
}

#[test]
fn within_apply() {
    check(
        expr,
        "within { x } apply { y }",
        &expect![[r#"
            Expr _id_ [0-24]: Conjugate:
                Block _id_ [7-12]:
                    Stmt _id_ [9-10]: Expr: Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "x")
                Block _id_ [19-24]:
                    Stmt _id_ [21-22]: Expr: Expr _id_ [21-22]: Path: Path _id_ [21-22] (Ident _id_ [21-22] "y")"#]],
    );
}

#[test]
fn unit() {
    check(expr, "()", &expect!["Expr _id_ [0-2]: Unit"]);
}

#[test]
fn paren() {
    check(
        expr,
        "(x)",
        &expect![[
            r#"Expr _id_ [0-3]: Paren: Expr _id_ [1-2]: Path: Path _id_ [1-2] (Ident _id_ [1-2] "x")"#
        ]],
    );
}

#[test]
fn singleton_tuple() {
    check(
        expr,
        "(x,)",
        &expect![[r#"
            Expr _id_ [0-4]: Tuple:
                Expr _id_ [1-2]: Path: Path _id_ [1-2] (Ident _id_ [1-2] "x")"#]],
    );
}

#[test]
fn pair() {
    check(
        expr,
        "(x, y)",
        &expect![[r#"
            Expr _id_ [0-6]: Tuple:
                Expr _id_ [1-2]: Path: Path _id_ [1-2] (Ident _id_ [1-2] "x")
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")"#]],
    );
}

#[test]
fn array_empty() {
    check(expr, "[]", &expect!["Expr _id_ [0-2]: Array:"]);
}

#[test]
fn array_single() {
    check(
        expr,
        "[x]",
        &expect![[r#"
            Expr _id_ [0-3]: Array:
                Expr _id_ [1-2]: Path: Path _id_ [1-2] (Ident _id_ [1-2] "x")"#]],
    );
}

#[test]
fn array_pair() {
    check(
        expr,
        "[x, y]",
        &expect![[r#"
            Expr _id_ [0-6]: Array:
                Expr _id_ [1-2]: Path: Path _id_ [1-2] (Ident _id_ [1-2] "x")
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")"#]],
    );
}

#[test]
fn array_without_commas() {
    check(
        expr,
        "[1. 0. 0.]",
        &expect![[r#"
            Error(
                Token(
                    Comma,
                    Float,
                    Span {
                        lo: 4,
                        hi: 6,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn array_single_without_ending_bracket() {
    check(
        expr,
        "[1.;",
        &expect![[r#"
            Error(
                Token(
                    Close(
                        Bracket,
                    ),
                    Semi,
                    Span {
                        lo: 3,
                        hi: 4,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn array_repeat() {
    check(
        expr,
        "[0, size = 3]",
        &expect![[r#"
            Expr _id_ [0-13]: ArrayRepeat:
                Expr _id_ [1-2]: Lit: Int(0)
                Expr _id_ [11-12]: Lit: Int(3)"#]],
    );
}

#[test]
fn array_repeat_complex() {
    check(
        expr,
        "[Foo(), size = Count() + 1]",
        &expect![[r#"
            Expr _id_ [0-27]: ArrayRepeat:
                Expr _id_ [1-6]: Call:
                    Expr _id_ [1-4]: Path: Path _id_ [1-4] (Ident _id_ [1-4] "Foo")
                    Expr _id_ [4-6]: Unit
                Expr _id_ [15-26]: BinOp (Add):
                    Expr _id_ [15-22]: Call:
                        Expr _id_ [15-20]: Path: Path _id_ [15-20] (Ident _id_ [15-20] "Count")
                        Expr _id_ [20-22]: Unit
                    Expr _id_ [25-26]: Lit: Int(1)"#]],
    );
}

#[test]
fn array_size_last_item() {
    check(
        expr,
        "[foo, size]",
        &expect![[r#"
            Expr _id_ [0-11]: Array:
                Expr _id_ [1-4]: Path: Path _id_ [1-4] (Ident _id_ [1-4] "foo")
                Expr _id_ [6-10]: Path: Path _id_ [6-10] (Ident _id_ [6-10] "size")"#]],
    );
}

#[test]
fn array_size_middle_item() {
    check(
        expr,
        "[foo, size, bar]",
        &expect![[r#"
            Expr _id_ [0-16]: Array:
                Expr _id_ [1-4]: Path: Path _id_ [1-4] (Ident _id_ [1-4] "foo")
                Expr _id_ [6-10]: Path: Path _id_ [6-10] (Ident _id_ [6-10] "size")
                Expr _id_ [12-15]: Path: Path _id_ [12-15] (Ident _id_ [12-15] "bar")"#]],
    );
}

#[test]
fn array_repeat_no_items() {
    check(
        expr,
        "[size = 3]",
        &expect![[r#"
            Expr _id_ [0-10]: Array:
                Expr _id_ [1-9]: Assign:
                    Expr _id_ [1-5]: Path: Path _id_ [1-5] (Ident _id_ [1-5] "size")
                    Expr _id_ [8-9]: Lit: Int(3)"#]],
    );
}

#[test]
fn array_repeat_two_items() {
    check(
        expr,
        "[1, 2, size = 3]",
        &expect![[r#"
            Expr _id_ [0-16]: Array:
                Expr _id_ [1-2]: Lit: Int(1)
                Expr _id_ [4-5]: Lit: Int(2)
                Expr _id_ [7-15]: Assign:
                    Expr _id_ [7-11]: Path: Path _id_ [7-11] (Ident _id_ [7-11] "size")
                    Expr _id_ [14-15]: Lit: Int(3)"#]],
    );
}

#[test]
fn array_concat() {
    check(
        expr,
        "[1, 2] + [3, 4]",
        &expect![[r#"
            Expr _id_ [0-15]: BinOp (Add):
                Expr _id_ [0-6]: Array:
                    Expr _id_ [1-2]: Lit: Int(1)
                    Expr _id_ [4-5]: Lit: Int(2)
                Expr _id_ [9-15]: Array:
                    Expr _id_ [10-11]: Lit: Int(3)
                    Expr _id_ [13-14]: Lit: Int(4)"#]],
    );
}

#[test]
fn and_op() {
    check(
        expr,
        "x and y",
        &expect![[r#"
            Expr _id_ [0-7]: BinOp (AndL):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "y")"#]],
    );
}

#[test]
fn or_op() {
    check(
        expr,
        "x or y",
        &expect![[r#"
            Expr _id_ [0-6]: BinOp (OrL):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")"#]],
    );
}

#[test]
fn and_or_ops() {
    check(
        expr,
        "x or y and z",
        &expect![[r#"
            Expr _id_ [0-12]: BinOp (OrL):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-12]: BinOp (AndL):
                    Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")
                    Expr _id_ [11-12]: Path: Path _id_ [11-12] (Ident _id_ [11-12] "z")"#]],
    );
}

#[test]
fn eq_op() {
    check(
        expr,
        "x == y",
        &expect![[r#"
            Expr _id_ [0-6]: BinOp (Eq):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")"#]],
    );
}

#[test]
fn ne_op() {
    check(
        expr,
        "x != y",
        &expect![[r#"
            Expr _id_ [0-6]: BinOp (Neq):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")"#]],
    );
}

#[test]
fn gt_op() {
    check(
        expr,
        "x > y",
        &expect![[r#"
            Expr _id_ [0-5]: BinOp (Gt):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")"#]],
    );
}

#[test]
fn gte_op() {
    check(
        expr,
        "x >= y",
        &expect![[r#"
            Expr _id_ [0-6]: BinOp (Gte):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")"#]],
    );
}

#[test]
fn lt_op() {
    check(
        expr,
        "x < y",
        &expect![[r#"
            Expr _id_ [0-5]: BinOp (Lt):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")"#]],
    );
}

#[test]
fn lte_op() {
    check(
        expr,
        "x <= y",
        &expect![[r#"
            Expr _id_ [0-6]: BinOp (Lte):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")"#]],
    );
}

#[test]
fn bitwise_and_op() {
    check(
        expr,
        "x &&& y",
        &expect![[r#"
            Expr _id_ [0-7]: BinOp (AndB):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "y")"#]],
    );
}

#[test]
fn bitwise_or_op() {
    check(
        expr,
        "x ||| y",
        &expect![[r#"
            Expr _id_ [0-7]: BinOp (OrB):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "y")"#]],
    );
}

#[test]
fn bitwise_and_or_op() {
    check(
        expr,
        "x ||| y &&& z",
        &expect![[r#"
            Expr _id_ [0-13]: BinOp (OrB):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [6-13]: BinOp (AndB):
                    Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "y")
                    Expr _id_ [12-13]: Path: Path _id_ [12-13] (Ident _id_ [12-13] "z")"#]],
    );
}

#[test]
fn bitwise_xor_op() {
    check(
        expr,
        "x ^^^ y",
        &expect![[r#"
            Expr _id_ [0-7]: BinOp (XorB):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "y")"#]],
    );
}

#[test]
fn bitwise_or_xor_ops() {
    check(
        expr,
        "x ||| y ^^^ z",
        &expect![[r#"
            Expr _id_ [0-13]: BinOp (OrB):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [6-13]: BinOp (XorB):
                    Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "y")
                    Expr _id_ [12-13]: Path: Path _id_ [12-13] (Ident _id_ [12-13] "z")"#]],
    );
}

#[test]
fn shl_op() {
    check(
        expr,
        "x <<< y",
        &expect![[r#"
            Expr _id_ [0-7]: BinOp (Shl):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "y")"#]],
    );
}

#[test]
fn shr_op() {
    check(
        expr,
        "x >>> y",
        &expect![[r#"
            Expr _id_ [0-7]: BinOp (Shr):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "y")"#]],
    );
}

#[test]
fn add_op() {
    check(
        expr,
        "x + y",
        &expect![[r#"
            Expr _id_ [0-5]: BinOp (Add):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")"#]],
    );
}

#[test]
fn add_left_assoc() {
    check(
        expr,
        "x + y + z",
        &expect![[r#"
            Expr _id_ [0-9]: BinOp (Add):
                Expr _id_ [0-5]: BinOp (Add):
                    Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                    Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")
                Expr _id_ [8-9]: Path: Path _id_ [8-9] (Ident _id_ [8-9] "z")"#]],
    );
}

#[test]
fn sub_op() {
    check(
        expr,
        "x - y",
        &expect![[r#"
            Expr _id_ [0-5]: BinOp (Sub):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")"#]],
    );
}

#[test]
fn mul_op() {
    check(
        expr,
        "x * y",
        &expect![[r#"
            Expr _id_ [0-5]: BinOp (Mul):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")"#]],
    );
}

#[test]
fn add_mul_ops() {
    check(
        expr,
        "x + y * z",
        &expect![[r#"
            Expr _id_ [0-9]: BinOp (Add):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [4-9]: BinOp (Mul):
                    Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")
                    Expr _id_ [8-9]: Path: Path _id_ [8-9] (Ident _id_ [8-9] "z")"#]],
    );
}

#[test]
fn div_op() {
    check(
        expr,
        "x / y",
        &expect![[r#"
            Expr _id_ [0-5]: BinOp (Div):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")"#]],
    );
}

#[test]
fn mod_op() {
    check(
        expr,
        "x % y",
        &expect![[r#"
            Expr _id_ [0-5]: BinOp (Mod):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")"#]],
    );
}

#[test]
fn two_plus_two_is_four() {
    check(
        expr,
        "2 + 2 == 4",
        &expect![[r#"
            Expr _id_ [0-10]: BinOp (Eq):
                Expr _id_ [0-5]: BinOp (Add):
                    Expr _id_ [0-1]: Lit: Int(2)
                    Expr _id_ [4-5]: Lit: Int(2)
                Expr _id_ [9-10]: Lit: Int(4)"#]],
    );
}

#[test]
fn exp_op() {
    check(
        expr,
        "x ^ y",
        &expect![[r#"
            Expr _id_ [0-5]: BinOp (Exp):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "y")"#]],
    );
}

#[test]
fn exp_right_assoc() {
    check(
        expr,
        "2 ^ 3 ^ 4",
        &expect![[r#"
            Expr _id_ [0-9]: BinOp (Exp):
                Expr _id_ [0-1]: Lit: Int(2)
                Expr _id_ [4-9]: BinOp (Exp):
                    Expr _id_ [4-5]: Lit: Int(3)
                    Expr _id_ [8-9]: Lit: Int(4)"#]],
    );
}

#[test]
fn negate_exp() {
    check(
        expr,
        "-2^3",
        &expect![[r#"
            Expr _id_ [0-4]: UnOp (Neg):
                Expr _id_ [1-4]: BinOp (Exp):
                    Expr _id_ [1-2]: Lit: Int(2)
                    Expr _id_ [3-4]: Lit: Int(3)"#]],
    );
}

#[test]
fn unwrap_op() {
    check(
        expr,
        "x!",
        &expect![[r#"
            Expr _id_ [0-2]: UnOp (Unwrap):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")"#]],
    );
}

#[test]
fn logical_not_op() {
    check(
        expr,
        "not x",
        &expect![[r#"
            Expr _id_ [0-5]: UnOp (NotL):
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")"#]],
    );
}

#[test]
fn bitwise_not_op() {
    check(
        expr,
        "~~~x",
        &expect![[r#"
            Expr _id_ [0-4]: UnOp (NotB):
                Expr _id_ [3-4]: Path: Path _id_ [3-4] (Ident _id_ [3-4] "x")"#]],
    );
}

#[test]
fn pos_op() {
    check(
        expr,
        "+x",
        &expect![[r#"
            Expr _id_ [0-2]: UnOp (Pos):
                Expr _id_ [1-2]: Path: Path _id_ [1-2] (Ident _id_ [1-2] "x")"#]],
    );
}

#[test]
fn neg_op() {
    check(
        expr,
        "-x",
        &expect![[r#"
            Expr _id_ [0-2]: UnOp (Neg):
                Expr _id_ [1-2]: Path: Path _id_ [1-2] (Ident _id_ [1-2] "x")"#]],
    );
}

#[test]
fn neg_minus_ops() {
    check(
        expr,
        "-x - y",
        &expect![[r#"
            Expr _id_ [0-6]: BinOp (Sub):
                Expr _id_ [0-2]: UnOp (Neg):
                    Expr _id_ [1-2]: Path: Path _id_ [1-2] (Ident _id_ [1-2] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")"#]],
    );
}

#[test]
fn adjoint_op() {
    check(
        expr,
        "Adjoint x",
        &expect![[r#"
            Expr _id_ [0-9]: UnOp (Functor Adj):
                Expr _id_ [8-9]: Path: Path _id_ [8-9] (Ident _id_ [8-9] "x")"#]],
    );
}

#[test]
fn adjoint_call_ops() {
    check(
        expr,
        "Adjoint X(q)",
        &expect![[r#"
            Expr _id_ [0-12]: Call:
                Expr _id_ [0-9]: UnOp (Functor Adj):
                    Expr _id_ [8-9]: Path: Path _id_ [8-9] (Ident _id_ [8-9] "X")
                Expr _id_ [9-12]: Paren: Expr _id_ [10-11]: Path: Path _id_ [10-11] (Ident _id_ [10-11] "q")"#]],
    );
}

#[test]
fn adjoint_index_call_ops() {
    check(
        expr,
        "Adjoint ops[i](q)",
        &expect![[r#"
            Expr _id_ [0-17]: Call:
                Expr _id_ [0-14]: UnOp (Functor Adj):
                    Expr _id_ [8-14]: Index:
                        Expr _id_ [8-11]: Path: Path _id_ [8-11] (Ident _id_ [8-11] "ops")
                        Expr _id_ [12-13]: Path: Path _id_ [12-13] (Ident _id_ [12-13] "i")
                Expr _id_ [14-17]: Paren: Expr _id_ [15-16]: Path: Path _id_ [15-16] (Ident _id_ [15-16] "q")"#]],
    );
}

#[test]
fn controlled_op() {
    check(
        expr,
        "Controlled x",
        &expect![[r#"
            Expr _id_ [0-12]: UnOp (Functor Ctl):
                Expr _id_ [11-12]: Path: Path _id_ [11-12] (Ident _id_ [11-12] "x")"#]],
    );
}

#[test]
fn controlled_call_ops() {
    check(
        expr,
        "Controlled X([q1], q2)",
        &expect![[r#"
            Expr _id_ [0-22]: Call:
                Expr _id_ [0-12]: UnOp (Functor Ctl):
                    Expr _id_ [11-12]: Path: Path _id_ [11-12] (Ident _id_ [11-12] "X")
                Expr _id_ [12-22]: Tuple:
                    Expr _id_ [13-17]: Array:
                        Expr _id_ [14-16]: Path: Path _id_ [14-16] (Ident _id_ [14-16] "q1")
                    Expr _id_ [19-21]: Path: Path _id_ [19-21] (Ident _id_ [19-21] "q2")"#]],
    );
}

#[test]
fn controlled_index_call_ops() {
    check(
        expr,
        "Controlled ops[i]([q1], q2)",
        &expect![[r#"
            Expr _id_ [0-27]: Call:
                Expr _id_ [0-17]: UnOp (Functor Ctl):
                    Expr _id_ [11-17]: Index:
                        Expr _id_ [11-14]: Path: Path _id_ [11-14] (Ident _id_ [11-14] "ops")
                        Expr _id_ [15-16]: Path: Path _id_ [15-16] (Ident _id_ [15-16] "i")
                Expr _id_ [17-27]: Tuple:
                    Expr _id_ [18-22]: Array:
                        Expr _id_ [19-21]: Path: Path _id_ [19-21] (Ident _id_ [19-21] "q1")
                    Expr _id_ [24-26]: Path: Path _id_ [24-26] (Ident _id_ [24-26] "q2")"#]],
    );
}

#[test]
fn update_op() {
    check(
        expr,
        "x w/ i <- v",
        &expect![[r#"
            Expr _id_ [0-11]: TernOp (Update):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "i")
                Expr _id_ [10-11]: Path: Path _id_ [10-11] (Ident _id_ [10-11] "v")"#]],
    );
}

#[test]
fn update_op_left_assoc() {
    check(
        expr,
        "x w/ i1 <- v1 w/ i2 <- v2",
        &expect![[r#"
            Expr _id_ [0-25]: TernOp (Update):
                Expr _id_ [0-13]: TernOp (Update):
                    Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                    Expr _id_ [5-7]: Path: Path _id_ [5-7] (Ident _id_ [5-7] "i1")
                    Expr _id_ [11-13]: Path: Path _id_ [11-13] (Ident _id_ [11-13] "v1")
                Expr _id_ [17-19]: Path: Path _id_ [17-19] (Ident _id_ [17-19] "i2")
                Expr _id_ [23-25]: Path: Path _id_ [23-25] (Ident _id_ [23-25] "v2")"#]],
    );
}

#[test]
fn cond_op() {
    check(
        expr,
        "c ? a | b",
        &expect![[r#"
            Expr _id_ [0-9]: TernOp (Cond):
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "c")
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "a")
                Expr _id_ [8-9]: Path: Path _id_ [8-9] (Ident _id_ [8-9] "b")"#]],
    );
}

#[test]
fn cond_op_right_assoc() {
    check(
        expr,
        "c1 ? a | c2 ? b | c",
        &expect![[r#"
            Expr _id_ [0-19]: TernOp (Cond):
                Expr _id_ [0-2]: Path: Path _id_ [0-2] (Ident _id_ [0-2] "c1")
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "a")
                Expr _id_ [9-19]: TernOp (Cond):
                    Expr _id_ [9-11]: Path: Path _id_ [9-11] (Ident _id_ [9-11] "c2")
                    Expr _id_ [14-15]: Path: Path _id_ [14-15] (Ident _id_ [14-15] "b")
                    Expr _id_ [18-19]: Path: Path _id_ [18-19] (Ident _id_ [18-19] "c")"#]],
    );
}

#[test]
fn field_op() {
    check(
        expr,
        "x::foo",
        &expect![[r#"
            Expr _id_ [0-6]: Field:
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Ident _id_ [3-6] "foo""#]],
    );
}

#[test]
fn index_op() {
    check(
        expr,
        "x[i]",
        &expect![[r#"
            Expr _id_ [0-4]: Index:
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [2-3]: Path: Path _id_ [2-3] (Ident _id_ [2-3] "i")"#]],
    );
}

#[test]
fn call_op_unit() {
    check(
        expr,
        "Foo()",
        &expect![[r#"
            Expr _id_ [0-5]: Call:
                Expr _id_ [0-3]: Path: Path _id_ [0-3] (Ident _id_ [0-3] "Foo")
                Expr _id_ [3-5]: Unit"#]],
    );
}

#[test]
fn struct_cons_empty() {
    check(
        expr,
        "new Foo { }",
        &expect![[
            r#"Expr _id_ [0-11]: Struct (Path _id_ [4-7] (Ident _id_ [4-7] "Foo")): <empty>"#
        ]],
    );
}

#[test]
fn struct_cons() {
    check(
        expr,
        "new Foo { field1 = 3, field2 = 6, field3 = { 2 + 15 } }",
        &expect![[r#"
            Expr _id_ [0-55]: Struct (Path _id_ [4-7] (Ident _id_ [4-7] "Foo")):
                FieldsAssign _id_ [10-20]: (Ident _id_ [10-16] "field1") Expr _id_ [19-20]: Lit: Int(3)
                FieldsAssign _id_ [22-32]: (Ident _id_ [22-28] "field2") Expr _id_ [31-32]: Lit: Int(6)
                FieldsAssign _id_ [34-53]: (Ident _id_ [34-40] "field3") Expr _id_ [43-53]: Expr Block: Block _id_ [43-53]:
                    Stmt _id_ [45-51]: Expr: Expr _id_ [45-51]: BinOp (Add):
                        Expr _id_ [45-46]: Lit: Int(2)
                        Expr _id_ [49-51]: Lit: Int(15)"#]],
    );
}

#[test]
fn struct_copy_cons() {
    check(
        expr,
        "new Foo { ...foo, field1 = 3, field3 = { 2 + 15 } }",
        &expect![[r#"
            Expr _id_ [0-51]: Struct (Path _id_ [4-7] (Ident _id_ [4-7] "Foo")):
                Copy: Expr _id_ [13-16]: Path: Path _id_ [13-16] (Ident _id_ [13-16] "foo")
                FieldsAssign _id_ [18-28]: (Ident _id_ [18-24] "field1") Expr _id_ [27-28]: Lit: Int(3)
                FieldsAssign _id_ [30-49]: (Ident _id_ [30-36] "field3") Expr _id_ [39-49]: Expr Block: Block _id_ [39-49]:
                    Stmt _id_ [41-47]: Expr: Expr _id_ [41-47]: BinOp (Add):
                        Expr _id_ [41-42]: Lit: Int(2)
                        Expr _id_ [45-47]: Lit: Int(15)"#]],
    );
}

#[test]
fn struct_copy_cons_empty() {
    check(
        expr,
        "new Foo { ...foo }",
        &expect![[r#"
            Expr _id_ [0-18]: Struct (Path _id_ [4-7] (Ident _id_ [4-7] "Foo")):
                Copy: Expr _id_ [13-16]: Path: Path _id_ [13-16] (Ident _id_ [13-16] "foo")"#]],
    );
}

#[test]
fn call_op_one() {
    check(
        expr,
        "Foo(x)",
        &expect![[r#"
            Expr _id_ [0-6]: Call:
                Expr _id_ [0-3]: Path: Path _id_ [0-3] (Ident _id_ [0-3] "Foo")
                Expr _id_ [3-6]: Paren: Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")"#]],
    );
}

#[test]
fn call_op_singleton_tuple() {
    check(
        expr,
        "Foo(x,)",
        &expect![[r#"
            Expr _id_ [0-7]: Call:
                Expr _id_ [0-3]: Path: Path _id_ [0-3] (Ident _id_ [0-3] "Foo")
                Expr _id_ [3-7]: Tuple:
                    Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")"#]],
    );
}

#[test]
fn call_op_pair() {
    check(
        expr,
        "Foo(x, y)",
        &expect![[r#"
            Expr _id_ [0-9]: Call:
                Expr _id_ [0-3]: Path: Path _id_ [0-3] (Ident _id_ [0-3] "Foo")
                Expr _id_ [3-9]: Tuple:
                    Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "x")
                    Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "y")"#]],
    );
}

#[test]
fn call_with_array() {
    check(
        expr,
        "f([1, 2])",
        &expect![[r#"
            Expr _id_ [0-9]: Call:
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "f")
                Expr _id_ [1-9]: Paren: Expr _id_ [2-8]: Array:
                    Expr _id_ [3-4]: Lit: Int(1)
                    Expr _id_ [6-7]: Lit: Int(2)"#]],
    );
}

#[test]
fn call_partial_app() {
    check(
        expr,
        "Foo(1, _, 3)",
        &expect![[r#"
            Expr _id_ [0-12]: Call:
                Expr _id_ [0-3]: Path: Path _id_ [0-3] (Ident _id_ [0-3] "Foo")
                Expr _id_ [3-12]: Tuple:
                    Expr _id_ [4-5]: Lit: Int(1)
                    Expr _id_ [7-8]: Hole
                    Expr _id_ [10-11]: Lit: Int(3)"#]],
    );
}

#[test]
fn call_partial_app_nested() {
    check(
        expr,
        "Foo(1, _, (_, 4))",
        &expect![[r#"
            Expr _id_ [0-17]: Call:
                Expr _id_ [0-3]: Path: Path _id_ [0-3] (Ident _id_ [0-3] "Foo")
                Expr _id_ [3-17]: Tuple:
                    Expr _id_ [4-5]: Lit: Int(1)
                    Expr _id_ [7-8]: Hole
                    Expr _id_ [10-16]: Tuple:
                        Expr _id_ [11-12]: Hole
                        Expr _id_ [14-15]: Lit: Int(4)"#]],
    );
}

#[test]
fn call_index_ops() {
    check(
        expr,
        "f()[i]",
        &expect![[r#"
            Expr _id_ [0-6]: Index:
                Expr _id_ [0-3]: Call:
                    Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "f")
                    Expr _id_ [1-3]: Unit
                Expr _id_ [4-5]: Path: Path _id_ [4-5] (Ident _id_ [4-5] "i")"#]],
    );
}

#[test]
fn index_call_ops() {
    check(
        expr,
        "fs[i]()",
        &expect![[r#"
            Expr _id_ [0-7]: Call:
                Expr _id_ [0-5]: Index:
                    Expr _id_ [0-2]: Path: Path _id_ [0-2] (Ident _id_ [0-2] "fs")
                    Expr _id_ [3-4]: Path: Path _id_ [3-4] (Ident _id_ [3-4] "i")
                Expr _id_ [5-7]: Unit"#]],
    );
}

#[test]
fn range_op() {
    check(
        expr,
        "x..y",
        &expect![[r#"
            Expr _id_ [0-4]: Range:
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                <no step>
                Expr _id_ [3-4]: Path: Path _id_ [3-4] (Ident _id_ [3-4] "y")"#]],
    );
}

#[test]
fn range_op_with_step() {
    check(
        expr,
        "x..y..z",
        &expect![[r#"
            Expr _id_ [0-7]: Range:
                Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "x")
                Expr _id_ [3-4]: Path: Path _id_ [3-4] (Ident _id_ [3-4] "y")
                Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "z")"#]],
    );
}

#[test]
fn range_complex_stop() {
    check(
        expr,
        "0..Length(xs) - 1",
        &expect![[r#"
            Expr _id_ [0-17]: Range:
                Expr _id_ [0-1]: Lit: Int(0)
                <no step>
                Expr _id_ [3-17]: BinOp (Sub):
                    Expr _id_ [3-13]: Call:
                        Expr _id_ [3-9]: Path: Path _id_ [3-9] (Ident _id_ [3-9] "Length")
                        Expr _id_ [9-13]: Paren: Expr _id_ [10-12]: Path: Path _id_ [10-12] (Ident _id_ [10-12] "xs")
                    Expr _id_ [16-17]: Lit: Int(1)"#]],
    );
}

#[test]
fn range_complex_start() {
    check(
        expr,
        "i + 1..n",
        &expect![[r#"
            Expr _id_ [0-8]: Range:
                Expr _id_ [0-5]: BinOp (Add):
                    Expr _id_ [0-1]: Path: Path _id_ [0-1] (Ident _id_ [0-1] "i")
                    Expr _id_ [4-5]: Lit: Int(1)
                <no step>
                Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "n")"#]],
    );
}

#[test]
fn range_complex_step() {
    check(
        expr,
        "0..s + 1..n",
        &expect![[r#"
            Expr _id_ [0-11]: Range:
                Expr _id_ [0-1]: Lit: Int(0)
                Expr _id_ [3-8]: BinOp (Add):
                    Expr _id_ [3-4]: Path: Path _id_ [3-4] (Ident _id_ [3-4] "s")
                    Expr _id_ [7-8]: Lit: Int(1)
                Expr _id_ [10-11]: Path: Path _id_ [10-11] (Ident _id_ [10-11] "n")"#]],
    );
}

#[test]
fn range_start_open() {
    check(
        expr,
        "2...",
        &expect![[r#"
            Expr _id_ [0-4]: Range:
                Expr _id_ [0-1]: Lit: Int(2)
                <no step>
                <no end>"#]],
    );
}

#[test]
fn range_start_step_open() {
    check(
        expr,
        "3..2...",
        &expect![[r#"
            Expr _id_ [0-7]: Range:
                Expr _id_ [0-1]: Lit: Int(3)
                Expr _id_ [3-4]: Lit: Int(2)
                <no end>"#]],
    );
}

#[test]
fn range_open_stop() {
    check(
        expr,
        "...2",
        &expect![[r#"
            Expr _id_ [0-4]: Range:
                <no start>
                <no step>
                Expr _id_ [3-4]: Lit: Int(2)"#]],
    );
}

#[test]
fn range_open_step_stop() {
    check(
        expr,
        "...2..3",
        &expect![[r#"
            Expr _id_ [0-7]: Range:
                <no start>
                Expr _id_ [3-4]: Lit: Int(2)
                Expr _id_ [6-7]: Lit: Int(3)"#]],
    );
}

#[test]
fn range_open_step_open() {
    check(
        expr,
        "...2...",
        &expect![[r#"
            Expr _id_ [0-7]: Range:
                <no start>
                Expr _id_ [3-4]: Lit: Int(2)
                <no end>"#]],
    );
}

#[test]
fn function_lambda() {
    check(
        expr,
        "x -> x + 1",
        &expect![[r#"
            Expr _id_ [0-10]: Lambda (Function):
                Pat _id_ [0-1]: Bind:
                    Ident _id_ [0-1] "x"
                Expr _id_ [5-10]: BinOp (Add):
                    Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "x")
                    Expr _id_ [9-10]: Lit: Int(1)"#]],
    );
}

#[test]
fn operation_lambda() {
    check(
        expr,
        "q => X(q)",
        &expect![[r#"
            Expr _id_ [0-9]: Lambda (Operation):
                Pat _id_ [0-1]: Bind:
                    Ident _id_ [0-1] "q"
                Expr _id_ [5-9]: Call:
                    Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "X")
                    Expr _id_ [6-9]: Paren: Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "q")"#]],
    );
}

#[test]
fn lambda_tuple_input() {
    check(
        expr,
        "(x, y) -> x + y",
        &expect![[r#"
            Expr _id_ [0-15]: Lambda (Function):
                Pat _id_ [0-6]: Tuple:
                    Pat _id_ [1-2]: Bind:
                        Ident _id_ [1-2] "x"
                    Pat _id_ [4-5]: Bind:
                        Ident _id_ [4-5] "y"
                Expr _id_ [10-15]: BinOp (Add):
                    Expr _id_ [10-11]: Path: Path _id_ [10-11] (Ident _id_ [10-11] "x")
                    Expr _id_ [14-15]: Path: Path _id_ [14-15] (Ident _id_ [14-15] "y")"#]],
    );
}

#[test]
fn lambda_invalid_input() {
    check(
        expr,
        "x + 1 -> x",
        &expect![[r#"
            Error(
                Convert(
                    "pattern",
                    "expression",
                    Span {
                        lo: 0,
                        hi: 5,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn lambda_invalid_tuple_input() {
    check(
        expr,
        "(x, y + 1) -> x + y",
        &expect![[r#"
            Error(
                Convert(
                    "pattern",
                    "expression",
                    Span {
                        lo: 4,
                        hi: 9,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn interpolated_string_missing_ending() {
    check(
        expr,
        r#"$"string"#,
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
fn interpolated_string() {
    check(
        expr,
        r#"$"string""#,
        &expect![[r#"
            Expr _id_ [0-9]: Interpolate:
                Lit: "string""#]],
    );
}

#[test]
fn interpolated_string_braced() {
    check(
        expr,
        r#"$"{x}""#,
        &expect![[r#"
            Expr _id_ [0-6]: Interpolate:
                Expr: Expr _id_ [3-4]: Path: Path _id_ [3-4] (Ident _id_ [3-4] "x")"#]],
    );
}

#[test]
fn interpolated_string_escape_brace() {
    check(
        expr,
        r#"$"\{""#,
        &expect![[r#"
            Expr _id_ [0-5]: Interpolate:
                Lit: "\\{""#]],
    );
}

#[test]
fn interpolated_string_unclosed_brace() {
    check(
        expr,
        r#"$"{"#,
        &expect![[r#"
            Error(
                Rule(
                    "expression",
                    Eof,
                    Span {
                        lo: 3,
                        hi: 3,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn interpolated_string_unclosed_brace_quote() {
    check(
        expr,
        r#"$"{""#,
        &expect![[r#"
            Error(
                Rule(
                    "expression",
                    Eof,
                    Span {
                        lo: 4,
                        hi: 4,
                    },
                ),
            )

            [
                Error(
                    Lex(
                        UnterminatedString(
                            Span {
                                lo: 3,
                                hi: 3,
                            },
                        ),
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn interpolated_string_unopened_brace() {
    check(
        expr,
        r#"$"}"#,
        &expect![[r#"
            Error(
                Rule(
                    "expression",
                    Eof,
                    Span {
                        lo: 3,
                        hi: 3,
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
fn interpolated_string_unopened_brace_quote() {
    check(
        expr,
        r#"$"}""#,
        &expect![[r#"
            Expr _id_ [0-4]: Interpolate:
                Lit: "}""#]],
    );
}

#[test]
fn interpolated_string_braced_index() {
    check(
        expr,
        r#"$"{xs[0]}""#,
        &expect![[r#"
            Expr _id_ [0-10]: Interpolate:
                Expr: Expr _id_ [3-8]: Index:
                    Expr _id_ [3-5]: Path: Path _id_ [3-5] (Ident _id_ [3-5] "xs")
                    Expr _id_ [6-7]: Lit: Int(0)"#]],
    );
}

#[test]
fn interpolated_string_two_braced() {
    check(
        expr,
        r#"$"{x} {y}""#,
        &expect![[r#"
            Expr _id_ [0-10]: Interpolate:
                Expr: Expr _id_ [3-4]: Path: Path _id_ [3-4] (Ident _id_ [3-4] "x")
                Lit: " "
                Expr: Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "y")"#]],
    );
}

#[test]
fn interpolated_string_braced_normal_string() {
    check(
        expr,
        r#"$"{"{}"}""#,
        &expect![[r#"
            Expr _id_ [0-9]: Interpolate:
                Expr: Expr _id_ [3-7]: Lit: String("{}")"#]],
    );
}

#[test]
fn nested_interpolated_string() {
    check(
        expr,
        r#"$"{$"{x}"}""#,
        &expect![[r#"
            Expr _id_ [0-11]: Interpolate:
                Expr: Expr _id_ [3-9]: Interpolate:
                    Expr: Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "x")"#]],
    );
}

#[test]
fn nested_interpolated_string_with_exprs() {
    check(
        expr,
        r#"$"foo {x + $"bar {y}"} baz""#,
        &expect![[r#"
            Expr _id_ [0-27]: Interpolate:
                Lit: "foo "
                Expr: Expr _id_ [7-21]: BinOp (Add):
                    Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "x")
                    Expr _id_ [11-21]: Interpolate:
                        Lit: "bar "
                        Expr: Expr _id_ [18-19]: Path: Path _id_ [18-19] (Ident _id_ [18-19] "y")
                Lit: " baz""#]],
    );
}

#[test]
fn duplicate_commas_in_tuple() {
    check(
        expr,
        "(x,, y)",
        &expect![[r#"
            Expr _id_ [0-7]: Tuple:
                Expr _id_ [1-2]: Path: Path _id_ [1-2] (Ident _id_ [1-2] "x")
                Expr _id_ [3-3]: Err
                Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 3,
                            hi: 3,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn many_duplicate_commas_in_tuple() {
    check(
        expr,
        "(x,,,, y)",
        &expect![[r#"
            Expr _id_ [0-9]: Tuple:
                Expr _id_ [1-2]: Path: Path _id_ [1-2] (Ident _id_ [1-2] "x")
                Expr _id_ [3-3]: Err
                Expr _id_ [4-4]: Err
                Expr _id_ [5-5]: Err
                Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "y")

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 3,
                            hi: 3,
                        },
                    ),
                ),
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 4,
                            hi: 4,
                        },
                    ),
                ),
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 5,
                            hi: 5,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn invalid_initial_comma_in_tuple() {
    check(
        expr,
        "(, x)",
        &expect![[r#"
            Expr _id_ [0-5]: Tuple:
                Expr _id_ [1-1]: Err
                Expr _id_ [3-4]: Path: Path _id_ [3-4] (Ident _id_ [3-4] "x")

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 1,
                            hi: 1,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn many_invalid_initial_commas_in_tuple() {
    check(
        expr,
        "(,,,, x)",
        &expect![[r#"
            Expr _id_ [0-8]: Tuple:
                Expr _id_ [1-1]: Err
                Expr _id_ [2-2]: Err
                Expr _id_ [3-3]: Err
                Expr _id_ [4-4]: Err
                Expr _id_ [6-7]: Path: Path _id_ [6-7] (Ident _id_ [6-7] "x")

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 1,
                            hi: 1,
                        },
                    ),
                ),
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 2,
                            hi: 2,
                        },
                    ),
                ),
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 3,
                            hi: 3,
                        },
                    ),
                ),
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 4,
                            hi: 4,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn duplicate_commas_in_pattern() {
    check(
        expr,
        "set (x,, y) = (1, 2)",
        &expect![[r#"
            Expr _id_ [0-20]: Assign:
                Expr _id_ [4-11]: Tuple:
                    Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "x")
                    Expr _id_ [7-7]: Err
                    Expr _id_ [9-10]: Path: Path _id_ [9-10] (Ident _id_ [9-10] "y")
                Expr _id_ [14-20]: Tuple:
                    Expr _id_ [15-16]: Lit: Int(1)
                    Expr _id_ [18-19]: Lit: Int(2)

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 7,
                            hi: 7,
                        },
                    ),
                ),
            ]"#]],
    );
    check(
        expr,
        "(x,, y) = (1, 2)",
        &expect![[r#"
            Expr _id_ [0-16]: Assign:
                Expr _id_ [0-7]: Tuple:
                    Expr _id_ [1-2]: Path: Path _id_ [1-2] (Ident _id_ [1-2] "x")
                    Expr _id_ [3-3]: Err
                    Expr _id_ [5-6]: Path: Path _id_ [5-6] (Ident _id_ [5-6] "y")
                Expr _id_ [10-16]: Tuple:
                    Expr _id_ [11-12]: Lit: Int(1)
                    Expr _id_ [14-15]: Lit: Int(2)

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 3,
                            hi: 3,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn invalid_initial_commas_in_pattern() {
    check(
        expr,
        "set (, x) = (1, 2)",
        &expect![[r#"
            Expr _id_ [0-18]: Assign:
                Expr _id_ [4-9]: Tuple:
                    Expr _id_ [5-5]: Err
                    Expr _id_ [7-8]: Path: Path _id_ [7-8] (Ident _id_ [7-8] "x")
                Expr _id_ [12-18]: Tuple:
                    Expr _id_ [13-14]: Lit: Int(1)
                    Expr _id_ [16-17]: Lit: Int(2)

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 5,
                            hi: 5,
                        },
                    ),
                ),
            ]"#]],
    );
    check(
        expr,
        "(, x) = (1, 2)",
        &expect![[r#"
            Expr _id_ [0-14]: Assign:
                Expr _id_ [0-5]: Tuple:
                    Expr _id_ [1-1]: Err
                    Expr _id_ [3-4]: Path: Path _id_ [3-4] (Ident _id_ [3-4] "x")
                Expr _id_ [8-14]: Tuple:
                    Expr _id_ [9-10]: Lit: Int(1)
                    Expr _id_ [12-13]: Lit: Int(2)

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 1,
                            hi: 1,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn friendly_error_on_parenthesized_for() {
    check(
        expr,
        "for (x in xs) { () }",
        &expect![[r#"
            Error(
                Token(
                    Close(
                        Paren,
                    ),
                    Keyword(
                        In,
                    ),
                    Span {
                        lo: 7,
                        hi: 9,
                    },
                ),
                Some(
                    "parenthesis are not permitted around for-loop iterations",
                ),
            )
        "#]],
    );
}

#[test]
fn no_help_text_on_non_parenthesized_for() {
    check(
        expr,
        "for x in invalid syntax { () }",
        &expect![[r#"
            Error(
                Token(
                    Open(
                        Brace,
                    ),
                    Ident,
                    Span {
                        lo: 17,
                        hi: 23,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn parenthesized_pat_in_for_is_valid() {
    check(
        expr,
        "for (x) in xs { () }",
        &expect![[r#"
            Expr _id_ [0-20]: For:
                Pat _id_ [4-7]: Paren:
                    Pat _id_ [5-6]: Bind:
                        Ident _id_ [5-6] "x"
                Expr _id_ [11-13]: Path: Path _id_ [11-13] (Ident _id_ [11-13] "xs")
                Block _id_ [14-20]:
                    Stmt _id_ [16-18]: Expr: Expr _id_ [16-18]: Unit"#]],
    );
}

#[test]
fn parenthesized_expr_in_for_is_valid() {
    check(
        expr,
        "for x in (xs) { () }",
        &expect![[r#"
            Expr _id_ [0-20]: For:
                Pat _id_ [4-5]: Bind:
                    Ident _id_ [4-5] "x"
                Expr _id_ [9-13]: Paren: Expr _id_ [10-12]: Path: Path _id_ [10-12] (Ident _id_ [10-12] "xs")
                Block _id_ [14-20]:
                    Stmt _id_ [16-18]: Expr: Expr _id_ [16-18]: Unit"#]],
    );
}
