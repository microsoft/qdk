// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn only_newlines() {
    check(
        "\n\n\n",
        &expect![[r#"
        Circuit [0-3]:
            items: <empty>"#]],
    );
}

#[test]
fn only_a_comment() {
    check(
        "# just a comment",
        &expect![[r#"
        Circuit [0-16]:
            items: <empty>"#]],
    );
}

#[test]
fn only_whitespace() {
    check(
        "   ",
        &expect![[r#"
        Circuit [0-3]:
            items: <empty>"#]],
    );
}

#[test]
fn only_tabs_and_spaces() {
    check(
        "\t  \t",
        &expect![[r#"
        Circuit [0-4]:
            items: <empty>"#]],
    );
}

#[test]
fn only_carriage_returns_and_newlines() {
    check(
        "\r\n\r\n",
        &expect![[r#"
        Circuit [0-4]:
            items: <empty>"#]],
    );
}

#[test]
fn blank_lines_and_comments_only() {
    check(
        "\n# a\n\n# b\n",
        &expect![[r#"
        Circuit [0-10]:
            items: <empty>"#]],
    );
}

#[test]
fn comment_inside_block() {
    check(
        indoc! {"
            REPEAT 2 {
              # inner
              H 0
            }
        "},
        &expect![[r#"
            Circuit [0-29]:
                items:
                    Block [0-28]:
                        block_instruction: Instruction [0-8]:
                            name: REPEAT
                            tag: <none>
                            args: <empty>
                            targets:
                                Target [7-8]:
                                    kind: Qubit(2)
                        items:
                            Instruction [23-26]:
                                name: H
                                tag: <none>
                                args: <empty>
                                targets:
                                    Target [25-26]:
                                        kind: Qubit(0)"#]],
    );
}
