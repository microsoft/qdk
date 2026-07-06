// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn line_not_starting_with_instruction_name_is_error() {
    check(
        "0 1",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected instruction_name, found uint
               ,----
             1 | 0 1
               : ^
               `----
        "#]],
    );
}

#[test]
fn lexer_error_is_surfaced() {
    check(
        "H @0",
        &expect![[r#"
            Qdk.Stim.UnrecognizedCharacter

              x unrecognized character
               ,----
             1 | H @0
               :   ^
               `----

            Circuit [0-4]:
                items:
                    Instruction [0-4]:
                        name: H
                        tag: <none>
                        args: <empty>
                        targets:
                            Target [3-4]:
                                kind: Qubit(0)"#]],
    );
}

#[test]
fn parser_recovers_to_next_line_after_error() {
    // The first line is invalid, but the parser recovers and the second line
    // parses cleanly, so only one error is reported.
    check(
        "0 1\nH 2",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected instruction_name, found uint
               ,-[1:1]
             1 | 0 1
               : ^
             2 | H 2
               `----

            Circuit [0-7]:
                items:
                    Instruction [4-7]:
                        name: H
                        tag: <none>
                        args: <empty>
                        targets:
                            Target [6-7]:
                                kind: Qubit(2)"#]],
    );
}

#[test]
fn multiple_errors_are_collected() {
    check(
        "0\n1\n2",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected instruction_name, found uint
               ,-[1:1]
             1 | 0
               : ^
             2 | 1
               `----

            Qdk.Stim.Parser.ExpectedToken

              x expected instruction_name, found uint
               ,-[2:1]
             1 | 0
             2 | 1
               : ^
             3 | 2
               `----

            Qdk.Stim.Parser.ExpectedToken

              x expected instruction_name, found uint
               ,-[3:1]
             2 | 1
             3 | 2
               : ^
               `----
        "#]],
    );
}

#[test]
fn line_starting_with_close_paren_is_error() {
    check(
        ")",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected instruction_name, found close(paren)
               ,----
             1 | )
               : ^
               `----
        "#]],
    );
}

#[test]
fn line_starting_with_star_is_error() {
    check(
        "*",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected instruction_name, found star
               ,----
             1 | *
               : ^
               `----
        "#]],
    );
}

#[test]
fn recovery_preserves_surrounding_instructions() {
    // The bad middle line is dropped; the valid lines before and after it
    // are both kept in the AST.
    check(
        "H 0\n0 1\nX 2",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected instruction_name, found uint
               ,-[2:1]
             1 | H 0
             2 | 0 1
               : ^
             3 | X 2
               `----

            Circuit [0-11]:
                items:
                    Instruction [0-3]:
                        name: H
                        tag: <none>
                        args: <empty>
                        targets:
                            Target [2-3]:
                                kind: Qubit(0)
                    Instruction [8-11]:
                        name: X
                        tag: <none>
                        args: <empty>
                        targets:
                            Target [10-11]:
                                kind: Qubit(2)"#]],
    );
}

#[test]
fn recovery_inside_repeat_block() {
    // Recovery also happens within a block: the bad line is skipped and the
    // following instruction is still added to the block.
    check(
        "REPEAT 2 {\n0 1\nH 0\n}",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected instruction_name, found uint
               ,-[2:1]
             1 | REPEAT 2 {
             2 | 0 1
               : ^
             3 | H 0
               `----

            Circuit [0-20]:
                items:
                    Block [0-20]:
                        block_instruction: Instruction [0-8]:
                            name: REPEAT
                            tag: <none>
                            args: <empty>
                            targets:
                                Target [7-8]:
                                    kind: Qubit(2)
                        items:
                            Instruction [15-18]:
                                name: H
                                tag: <none>
                                args: <empty>
                                targets:
                                    Target [17-18]:
                                        kind: Qubit(0)"#]],
    );
}

#[test]
fn malformed_line_is_discarded_during_recovery() {
    // A line that starts validly but has trailing garbage is discarded whole
    // (the "H 0" prefix is not kept); parsing resumes on the next line.
    check(
        "H 0 )\nX 1",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected newline, found close(paren)
               ,-[1:5]
             1 | H 0 )
               :     ^
             2 | X 1
               `----

            Circuit [0-9]:
                items:
                    Instruction [6-9]:
                        name: X
                        tag: <none>
                        args: <empty>
                        targets:
                            Target [8-9]:
                                kind: Qubit(1)"#]],
    );
}
