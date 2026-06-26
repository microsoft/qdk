// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn multiple_instructions_on_separate_lines() {
    check("H 0\nX 1", &expect![[r#"
        Circuit [0-7]:
            items:
                Instruction [0-3]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-3]:
                            kind: Qubit(0)
                Instruction [4-7]:
                    name: X
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [6-7]:
                            kind: Qubit(1)"#]]);
}

#[test]
fn blank_lines_between_instructions_are_skipped() {
    check("H 0\n\n\nX 1", &expect![[r#"
        Circuit [0-9]:
            items:
                Instruction [0-3]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-3]:
                            kind: Qubit(0)
                Instruction [6-9]:
                    name: X
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [8-9]:
                            kind: Qubit(1)"#]]);
}

#[test]
fn leading_newlines_are_skipped() {
    check("\n\nH 0", &expect![[r#"
        Circuit [0-5]:
            items:
                Instruction [2-5]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [4-5]:
                            kind: Qubit(0)"#]]);
}

#[test]
fn trailing_newline_is_accepted() {
    check("H 0\n", &expect![[r#"
        Circuit [0-4]:
            items:
                Instruction [0-3]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-3]:
                            kind: Qubit(0)"#]]);
}

#[test]
fn comments_are_skipped() {
    check("H 0 # comment\nX 1", &expect![[r#"
        Circuit [0-17]:
            items:
                Instruction [0-3]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-3]:
                            kind: Qubit(0)
                Instruction [14-17]:
                    name: X
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [16-17]:
                            kind: Qubit(1)"#]]);
}

#[test]
fn comment_only_line_between_instructions() {
    check("H 0\n# a comment\nX 1", &expect![[r#"
        Circuit [0-19]:
            items:
                Instruction [0-3]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-3]:
                            kind: Qubit(0)
                Instruction [16-19]:
                    name: X
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [18-19]:
                            kind: Qubit(1)"#]]);
}

#[test]
fn leading_comment_is_skipped() {
    check("# header\nH 0", &expect![[r#"
        Circuit [0-12]:
            items:
                Instruction [9-12]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [11-12]:
                            kind: Qubit(0)"#]]);
}

#[test]
fn trailing_comment_without_newline() {
    check("H 0 # comment", &expect![[r#"
        Circuit [0-13]:
            items:
                Instruction [0-3]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-3]:
                            kind: Qubit(0)"#]]);
}

#[test]
fn comments_and_blank_lines_mixed() {
    check("\n# c1\n\n# c2\nH 0", &expect![[r#"
        Circuit [0-15]:
            items:
                Instruction [12-15]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [14-15]:
                            kind: Qubit(0)"#]]);
}

#[test]
fn horizontal_whitespace_around_tokens_is_ignored() {
    check("   H   0   \n   X   1   ", &expect![[r#"
        Circuit [0-23]:
            items:
                Instruction [3-8]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [7-8]:
                            kind: Qubit(0)
                Instruction [15-20]:
                    name: X
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [19-20]:
                            kind: Qubit(1)"#]]);
}

#[test]
fn tabs_separate_tokens() {
    check("H\t0\nX\t1", &expect![[r#"
        Circuit [0-7]:
            items:
                Instruction [0-3]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-3]:
                            kind: Qubit(0)
                Instruction [4-7]:
                    name: X
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [6-7]:
                            kind: Qubit(1)"#]]);
}

#[test]
fn crlf_line_endings_separate_instructions() {
    check("H 0\r\nX 1", &expect![[r#"
        Circuit [0-8]:
            items:
                Instruction [0-3]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-3]:
                            kind: Qubit(0)
                Instruction [5-8]:
                    name: X
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [7-8]:
                            kind: Qubit(1)"#]]);
}
