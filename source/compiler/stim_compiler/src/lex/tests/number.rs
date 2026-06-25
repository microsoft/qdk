// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn single_digit_uint() {
    check("0", &expect!["uint [0-1]"]);
    check("9", &expect!["uint [0-1]"]);
}

#[test]
fn multi_digit_uint() {
    check("42", &expect!["uint [0-2]"]);
    check("123", &expect!["uint [0-3]"]);
}

#[test]
fn signed_integer_is_a_double() {
    check("+1", &expect!["double [0-2]"]);
    check("-1", &expect!["double [0-2]"]);
    check("+42", &expect!["double [0-3]"]);
    check("-42", &expect!["double [0-3]"]);
    check("+0", &expect!["double [0-2]"]);
    check("-0", &expect!["double [0-2]"]);
}

#[test]
fn double_with_fractional_part() {
    check("0.5", &expect!["double [0-3]"]);
    check("3.14", &expect!["double [0-4]"]);
    check("12.0", &expect!["double [0-4]"]);
    check("100.001", &expect!["double [0-7]"]);
}

#[test]
fn double_with_exponent() {
    check("1e9", &expect!["double [0-3]"]);
    check("1E9", &expect!["double [0-3]"]);
    check("6e0", &expect!["double [0-3]"]);
    check("2e+5", &expect!["double [0-4]"]);
    check("2e-5", &expect!["double [0-4]"]);
    check("10E10", &expect!["double [0-5]"]);
}

#[test]
fn double_with_fraction_and_exponent() {
    check("1.0e9", &expect!["double [0-5]"]);
    check("3.14e10", &expect!["double [0-7]"]);
    check("+3.5e-2", &expect!["double [0-7]"]);
    check("-0.5E+8", &expect!["double [0-7]"]);
}
#[test]
fn lone_sign_is_error() {
    check(
        "+",
        &expect![[r#"
        Stim.MissingDigitsAfterSign

          x expected digits after sign
           ,----
         1 | +
           : ^
           `----
    "#]],
    );
    check(
        "-",
        &expect![[r#"
        Stim.MissingDigitsAfterSign

          x expected digits after sign
           ,----
         1 | -
           : ^
           `----
    "#]],
    );
}

#[test]
fn trailing_decimal_point_is_error() {
    check(
        "3.",
        &expect![[r#"
        Stim.MissingFractionalDigits

          x expected digits after decimal point
           ,----
         1 | 3.
           : ^^
           `----
    "#]],
    );
    check(
        "0.",
        &expect![[r#"
        Stim.MissingFractionalDigits

          x expected digits after decimal point
           ,----
         1 | 0.
           : ^^
           `----
    "#]],
    );
    check(
        "12.",
        &expect![[r#"
        Stim.MissingFractionalDigits

          x expected digits after decimal point
           ,----
         1 | 12.
           : ^^^
           `----
    "#]],
    );
}

#[test]
fn signed_trailing_decimal_point_is_error() {
    check(
        "+5.",
        &expect![[r#"
        Stim.MissingFractionalDigits

          x expected digits after decimal point
           ,----
         1 | +5.
           : ^^^
           `----
    "#]],
    );
    check(
        "-5.",
        &expect![[r#"
        Stim.MissingFractionalDigits

          x expected digits after decimal point
           ,----
         1 | -5.
           : ^^^
           `----
    "#]],
    );
}

#[test]
fn decimal_point_followed_by_non_digit_is_error() {
    check(
        "3.e5",
        &expect![[r#"
        Stim.MissingFractionalDigits

          x expected digits after decimal point
           ,----
         1 | 3.e5
           : ^^
           `----

        instruction_name [2-4]"#]],
    );
}

#[test]
fn bare_exponent_marker_is_error() {
    check(
        "1e",
        &expect![[r#"
        Stim.MissingExponentDigits

          x expected digits in exponent
           ,----
         1 | 1e
           : ^^
           `----
    "#]],
    );
    check(
        "1E",
        &expect![[r#"
        Stim.MissingExponentDigits

          x expected digits in exponent
           ,----
         1 | 1E
           : ^^
           `----
    "#]],
    );
}

#[test]
fn exponent_sign_without_digits_is_error() {
    check(
        "1e+",
        &expect![[r#"
        Stim.MissingExponentDigits

          x expected digits in exponent
           ,----
         1 | 1e+
           : ^^^
           `----
    "#]],
    );
    check(
        "1e-",
        &expect![[r#"
        Stim.MissingExponentDigits

          x expected digits in exponent
           ,----
         1 | 1e-
           : ^^^
           `----
    "#]],
    );
}

#[test]
fn fractional_then_bare_exponent_is_error() {
    check(
        "2.5e",
        &expect![[r#"
        Stim.MissingExponentDigits

          x expected digits in exponent
           ,----
         1 | 2.5e
           : ^^^^
           `----
    "#]],
    );
    check(
        "3.0E+",
        &expect![[r#"
        Stim.MissingExponentDigits

          x expected digits in exponent
           ,----
         1 | 3.0E+
           : ^^^^^
           `----
    "#]],
    );
}

#[test]
fn signed_then_bare_exponent_is_error() {
    check(
        "+1e",
        &expect![[r#"
        Stim.MissingExponentDigits

          x expected digits in exponent
           ,----
         1 | +1e
           : ^^^
           `----
    "#]],
    );
}

#[test]
fn leading_decimal_point_is_not_a_double() {
    // A double must start with a digit, so ".5" is an unrecognized '.'
    // followed by the integer "5".
    check(
        ".5",
        &expect![[r#"
        Stim.UnrecognizedCharacter

          x unrecognized character
           ,----
         1 | .5
           : ^
           `----

        uint [1-2]"#]],
    );
}

#[test]
fn second_decimal_point_ends_the_double() {
    // The number stops at the first complete fraction; the extra '.' is
    // an unrecognized character and "3" lexes as a separate integer.
    check(
        "1.2.3",
        &expect![[r#"
        double [0-3]
        Stim.UnrecognizedCharacter

          x unrecognized character
           ,----
         1 | 1.2.3
           :    ^
           `----

        uint [4-5]"#]],
    );
}

#[test]
fn double_sign_recovers_to_a_double() {
    // The first '+' has no digits (error); lexing resumes at "+1".
    check(
        "++1",
        &expect![[r#"
        Stim.MissingDigitsAfterSign

          x expected digits after sign
           ,----
         1 | ++1
           : ^
           `----

        double [1-3]"#]],
    );
}
