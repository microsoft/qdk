// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn no_args_no_targets() {
    check(
        "TICK",
        &expect![[r#"
        Circuit [0-4]:
            items:
                Instruction [0-4]:
                    name: TICK
                    tag: <none>
                    args: <empty>
                    targets: <empty>"#]],
    );
}

#[test]
fn no_args_with_targets() {
    check(
        "H 0 1 2",
        &expect![[r#"
        Circuit [0-7]:
            items:
                Instruction [0-7]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-3]:
                            kind: Qubit(0)
                        Target [4-5]:
                            kind: Qubit(1)
                        Target [6-7]:
                            kind: Qubit(2)"#]],
    );
}

#[test]
fn args_no_targets() {
    check(
        "X_ERROR(0.1)",
        &expect![[r#"
            Circuit [0-12]:
                items:
                    Instruction [0-12]:
                        name: X_ERROR
                        tag: <none>
                        args:
                            0.1
                        targets: <empty>"#]],
    );
}

#[test]
fn args_with_targets() {
    check(
        "X_ERROR(0.1) 0 1",
        &expect![[r#"
        Circuit [0-16]:
            items:
                Instruction [0-16]:
                    name: X_ERROR
                    tag: <none>
                    args:
                        0.1
                    targets:
                        Target [13-14]:
                            kind: Qubit(0)
                        Target [15-16]:
                            kind: Qubit(1)"#]],
    );
}

#[test]
fn block_instruction() {
    check(
        "REPEAT 5 {\n    H 0\n}",
        &expect![[r#"
        Circuit [0-20]:
            items:
                Block [0-20]:
                    block_instruction: Instruction [0-8]:
                        name: REPEAT
                        tag: <none>
                        args: <empty>
                        targets:
                            Target [7-8]:
                                kind: Qubit(5)
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
fn empty_parens_have_no_args() {
    check(
        "H() 0",
        &expect![[r#"
        Circuit [0-5]:
            items:
                Instruction [0-5]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [4-5]:
                            kind: Qubit(0)"#]],
    );
}
