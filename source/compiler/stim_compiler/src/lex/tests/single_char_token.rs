// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn parentheses() {
    check("(", &expect!["open(paren)(() [0-1]"]);
    check(")", &expect!["close(paren)()) [0-1]"]);
    check(
        "()",
        &expect![[r#"
        open(paren)(() [0-1]
        close(paren)()) [1-2]"#]],
    );
}

#[test]
fn braces() {
    check("{", &expect!["open(brace)({) [0-1]"]);
    check("}", &expect!["close(brace)(}) [0-1]"]);
    check(
        "{}",
        &expect![[r#"
        open(brace)({) [0-1]
        close(brace)(}) [1-2]"#]],
    );
}

#[test]
fn star() {
    check("*", &expect!["star(*) [0-1]"]);
}

#[test]
fn bang() {
    check("!", &expect!["bang(!) [0-1]"]);
}

#[test]
fn comma() {
    check(",", &expect!["comma(,) [0-1]"]);
}

#[test]
fn adjacent_single_char_tokens_need_no_separator() {
    check(
        "(){}*!,",
        &expect![[r#"
        open(paren)(() [0-1]
        close(paren)()) [1-2]
        open(brace)({) [2-3]
        close(brace)(}) [3-4]
        star(*) [4-5]
        bang(!) [5-6]
        comma(,) [6-7]"#]],
    );
}
