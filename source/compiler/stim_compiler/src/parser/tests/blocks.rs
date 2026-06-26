// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn repeat_block_with_body() {
    // The repeat count (3) is parsed as a Qubit target on the block instruction.
    check(
        "REPEAT 3 {\n    H 0\n    X 1\n}",
        &expect![[r#"
        Circuit [0-28]:
            items:
                Block [0-28]:
                    block_instruction: Instruction [0-8]:
                        name: REPEAT
                        tag: <none>
                        args: <empty>
                        targets:
                            Target [7-8]:
                                kind: Qubit(3)
                    items:
                        Instruction [15-18]:
                            name: H
                            tag: <none>
                            args: <empty>
                            targets:
                                Target [17-18]:
                                    kind: Qubit(0)
                        Instruction [23-26]:
                            name: X
                            tag: <none>
                            args: <empty>
                            targets:
                                Target [25-26]:
                                    kind: Qubit(1)"#]],
    );
}

#[test]
fn empty_repeat_block() {
    check(
        "REPEAT 5 {\n}",
        &expect![[r#"
        Circuit [0-12]:
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
        "REPEAT 2 {\n    REPEAT 3 {\n        H 0\n    }\n}",
        &expect![[r#"
            Circuit [0-45]:
                items:
                    Block [0-45]:
                        block_instruction: Instruction [0-8]:
                            name: REPEAT
                            tag: <none>
                            args: <empty>
                            targets:
                                Target [7-8]:
                                    kind: Qubit(2)
                        items:
                            Block [15-43]:
                                block_instruction: Instruction [15-23]:
                                    name: REPEAT
                                    tag: <none>
                                    args: <empty>
                                    targets:
                                        Target [22-23]:
                                            kind: Qubit(3)
                                items:
                                    Instruction [34-37]:
                                        name: H
                                        tag: <none>
                                        args: <empty>
                                        targets:
                                            Target [36-37]:
                                                kind: Qubit(0)"#]],
    );
}

#[test]
fn missing_newline_after_open_brace_is_error() {
    check(
        "REPEAT 5 { H 0 }",
        &expect![[r#"
        Stim.Parser.ExpectedToken

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
        "REPEAT 5 {\n    H 0",
        &expect![[r#"
        Stim.Parser.UnexpectedEof

          x unexpected end of input
           ,-[2:8]
         1 | REPEAT 5 {
         2 |     H 0
           `----
    "#]],
    );
}

#[test]
fn content_after_close_brace_is_error() {
    check(
        "REPEAT 5 {\n} H 0",
        &expect![[r#"
        Stim.Parser.ExpectedToken

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
        Stim.Parser.ExpectedToken

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
        "H 0 {\n}",
        &expect![[r#"
        Circuit [0-7]:
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
