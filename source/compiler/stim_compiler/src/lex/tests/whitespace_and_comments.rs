// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn horizontal_whitespace_between_tokens_is_skipped() {
    // Spaces, tabs, and carriage returns separate tokens but produce no tokens
    // of their own. A lone '\r' (not followed by '\n') is horizontal whitespace.
    check(
        "H 0",
        &expect![[r#"
        instruction_name(H) [0-1]
        uint(0) [2-3]"#]],
    );
    check(
        "H\t0",
        &expect![[r#"
        instruction_name(H) [0-1]
        uint(0) [2-3]"#]],
    );
    check(
        "H \t 0",
        &expect![[r#"
        instruction_name(H) [0-1]
        uint(0) [4-5]"#]],
    );
    check(
        "H\r0",
        &expect![[r#"
        instruction_name(H) [0-1]
        uint(0) [2-3]"#]],
    );
}

#[test]
fn leading_horizontal_whitespace_is_skipped() {
    check("   H", &expect!["instruction_name(H) [3-4]"]);
    check("\t\tH", &expect!["instruction_name(H) [2-3]"]);
    check("\rH", &expect!["instruction_name(H) [1-2]"]);
}

#[test]
fn trailing_horizontal_whitespace_is_skipped() {
    check("H   ", &expect!["instruction_name(H) [0-1]"]);
    check("H\t", &expect!["instruction_name(H) [0-1]"]);
    check("H\r", &expect!["instruction_name(H) [0-1]"]);
}

#[test]
fn leading_newlines_at_beginning_of_file() {
    // A run of newlines (and surrounding whitespace) collapses into a single
    // newline token. Windows line endings ("\r\n") behave just like "\n".
    check(
        "\nH",
        &expect![[r#"
        newline(\n) [0-1]
        instruction_name(H) [1-2]"#]],
    );
    check(
        "\n\nH",
        &expect![[r#"
        newline(\n\n) [0-2]
        instruction_name(H) [2-3]"#]],
    );
    check(
        "\n  \nH",
        &expect![[r#"
        newline(\n  \n) [0-4]
        instruction_name(H) [4-5]"#]],
    );
    check(
        "\r\nH",
        &expect![[r#"
        newline(\n) [1-2]
        instruction_name(H) [2-3]"#]],
    );
    check(
        "\r\n\r\nH",
        &expect![[r#"
        newline(\n\r\n) [1-4]
        instruction_name(H) [4-5]"#]],
    );
}

#[test]
fn trailing_newlines_at_end_of_file() {
    check(
        "H\n",
        &expect![[r#"
        instruction_name(H) [0-1]
        newline(\n) [1-2]"#]],
    );
    check(
        "H\n\n",
        &expect![[r#"
        instruction_name(H) [0-1]
        newline(\n\n) [1-3]"#]],
    );
    check(
        "H\n  \n",
        &expect![[r#"
        instruction_name(H) [0-1]
        newline(\n  \n) [1-5]"#]],
    );
    check(
        "H\r\n",
        &expect![[r#"
        instruction_name(H) [0-1]
        newline(\n) [2-3]"#]],
    );
    check(
        "H\r\n\r\n",
        &expect![[r#"
        instruction_name(H) [0-1]
        newline(\n\r\n) [2-5]"#]],
    );
}

#[test]
fn newlines_between_instructions() {
    check(
        "H\nX",
        &expect![[r#"
        instruction_name(H) [0-1]
        newline(\n) [1-2]
        instruction_name(X) [2-3]"#]],
    );
    check(
        "X 0\nH 1",
        &expect![[r#"
        instruction_name(X) [0-1]
        uint(0) [2-3]
        newline(\n) [3-4]
        instruction_name(H) [4-5]
        uint(1) [6-7]"#]],
    );
    check(
        "H\n\n\nX",
        &expect![[r#"
        instruction_name(H) [0-1]
        newline(\n\n\n) [1-4]
        instruction_name(X) [4-5]"#]],
    );
    check(
        "H\r\nX",
        &expect![[r#"
        instruction_name(H) [0-1]
        newline(\n) [2-3]
        instruction_name(X) [3-4]"#]],
    );
    check(
        "X 0\r\nH 1",
        &expect![[r#"
        instruction_name(X) [0-1]
        uint(0) [2-3]
        newline(\n) [4-5]
        instruction_name(H) [5-6]
        uint(1) [7-8]"#]],
    );
    check(
        "H\r\n\r\nX",
        &expect![[r#"
        instruction_name(H) [0-1]
        newline(\n\r\n) [2-5]
        instruction_name(X) [5-6]"#]],
    );
}

#[test]
fn whole_line_comment_is_skipped() {
    check("# a comment", &expect![]);
    check(
        "# a comment\nH",
        &expect![[r#"
        newline(\n) [11-12]
        instruction_name(H) [12-13]"#]],
    );
    check(
        "# a comment\r\nH",
        &expect![[r#"
        newline(\n) [12-13]
        instruction_name(H) [13-14]"#]],
    );
}

#[test]
fn trailing_comment_after_instruction_is_skipped() {
    check(
        "H 0 # apply hadamard",
        &expect![[r#"
        instruction_name(H) [0-1]
        uint(0) [2-3]"#]],
    );
}

#[test]
fn comment_between_instructions_is_skipped() {
    check(
        "H\n# a comment\nX",
        &expect![[r#"
        instruction_name(H) [0-1]
        newline(\n) [1-2]
        newline(\n) [13-14]
        instruction_name(X) [14-15]"#]],
    );
    check(
        "H\r\n# a comment\r\nX",
        &expect![[r#"
        instruction_name(H) [0-1]
        newline(\n) [2-3]
        newline(\n) [15-16]
        instruction_name(X) [16-17]"#]],
    );
}

#[test]
fn blank_lines_and_comments_only() {
    check(
        "\n# comment\n\n",
        &expect![[r#"
        newline(\n) [0-1]
        newline(\n\n) [10-12]"#]],
    );
}
