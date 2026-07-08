// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

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
        "REPEAT 2 {\n    # inner\n    H 0\n}",
        &expect![[r#"
        Circuit [0-32]:
            items:
                Block [0-32]:
                    block_instruction: Instruction [0-8]:
                        name: REPEAT
                        tag: <none>
                        args: <empty>
                        targets:
                            Target [7-8]:
                                kind: Qubit(2)
                    items:
                        Instruction [27-30]:
                            name: H
                            tag: <none>
                            args: <empty>
                            targets:
                                Target [29-30]:
                                    kind: Qubit(0)"#]],
    );
}
