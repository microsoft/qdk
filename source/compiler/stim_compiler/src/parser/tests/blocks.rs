// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn repeat_block_with_body() {
    // The repeat count (3) is parsed as a Qubit target on the block instruction.
    check(
        indoc! {"
            REPEAT 3 {
              H 0
              X 1
            }
        "},
        &expect![[r#"
            Circuit [0-25]:
                items:
                    Block [0-24]:
                        block_instruction: Instruction [0-8]:
                            name: REPEAT
                            tag: <none>
                            args: <empty>
                            targets:
                                Target [7-8]:
                                    kind: Qubit(3)
                        items:
                            Instruction [13-16]:
                                name: H
                                tag: <none>
                                args: <empty>
                                targets:
                                    Target [15-16]:
                                        kind: Qubit(0)
                            Instruction [19-22]:
                                name: X
                                tag: <none>
                                args: <empty>
                                targets:
                                    Target [21-22]:
                                        kind: Qubit(1)"#]],
    );
}

#[test]
fn empty_repeat_block() {
    check(
        indoc! {"
            REPEAT 5 {
            }
        "},
        &expect![[r#"
            Circuit [0-13]:
                items:
                    Block [0-12]:
                        block_instruction: Instruction [0-8]:
                            name: REPEAT
                            tag: <none>
                            args: <empty>
                            targets:
                                Target [7-8]:
                                    kind: Qubit(5)
                        items: <empty>"#]],
    );
}

#[test]
fn nested_repeat_blocks() {
    check(
        indoc! {"
            REPEAT 2 {
              REPEAT 3 {
                H 0
              }
            }
        "},
        &expect![[r#"
            Circuit [0-38]:
                items:
                    Block [0-37]:
                        block_instruction: Instruction [0-8]:
                            name: REPEAT
                            tag: <none>
                            args: <empty>
                            targets:
                                Target [7-8]:
                                    kind: Qubit(2)
                        items:
                            Block [13-35]:
                                block_instruction: Instruction [13-21]:
                                    name: REPEAT
                                    tag: <none>
                                    args: <empty>
                                    targets:
                                        Target [20-21]:
                                            kind: Qubit(3)
                                items:
                                    Instruction [28-31]:
                                        name: H
                                        tag: <none>
                                        args: <empty>
                                        targets:
                                            Target [30-31]:
                                                kind: Qubit(0)"#]],
    );
}

#[test]
fn missing_newline_after_open_brace_is_error() {
    check(
        "REPEAT 5 { H 0 }",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected newline, found instruction_name
               ,----
             1 | REPEAT 5 { H 0 }
               :            ^
               `----
        "#]],
    );
}

#[test]
fn unclosed_block_is_error() {
    check(
        indoc! {"
            REPEAT 5 {
              H 0"},
        &expect![[r#"
            Qdk.Stim.Parser.UnexpectedEof

              x unexpected end of input
               ,-[2:6]
             1 | REPEAT 5 {
             2 |   H 0
               `----
        "#]],
    );
}

#[test]
fn content_after_close_brace_is_error() {
    check(
        indoc! {"
            REPEAT 5 {
            } H 0
        "},
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected newline, found instruction_name
               ,-[2:3]
             1 | REPEAT 5 {
             2 | } H 0
               :   ^
               `----
        "#]],
    );
}

#[test]
fn stray_close_brace_is_error() {
    check(
        "}",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected instruction_name, found close(brace)
               ,----
             1 | }
               : ^
               `----
        "#]],
    );
}

#[test]
fn non_repeat_instruction_with_block() {
    // This should be parsed correctly, although it doesn't generate any meaningful code
    check(
        indoc! {"
            H 0 {
            }
        "},
        &expect![[r#"
            Circuit [0-8]:
                items:
                    Block [0-7]:
                        block_instruction: Instruction [0-3]:
                            name: H
                            tag: <none>
                            args: <empty>
                            targets:
                                Target [2-3]:
                                    kind: Qubit(0)
                        items: <empty>"#]],
    );
}
