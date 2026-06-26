// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn instruction_with_tag() {
    check(
        "H[tag] 0",
        &expect![[r#"
        Circuit [0-8]:
            items:
                Instruction [0-8]:
                    name: H
                    tag: tag
                    args: <empty>
                    targets:
                        Target [7-8]:
                            kind: Qubit(0)"#]],
    );
}

#[test]
fn empty_tag() {
    check(
        "H[] 0",
        &expect![[r#"
            Circuit [0-5]:
                items:
                    Instruction [0-5]:
                        name: H
                        tag: 
                        args: <empty>
                        targets:
                            Target [4-5]:
                                kind: Qubit(0)"#]],
    );
}

#[test]
fn tag_with_complicated_name() {
    check(
        "H[   _my.name_Is_TAG1_ ]",
        &expect![[r#"
            Circuit [0-24]:
                items:
                    Instruction [0-24]:
                        name: H
                        tag:    _my.name_Is_TAG1_ 
                        args: <empty>
                        targets: <empty>"#]],
    );
}

#[test]
fn tag_with_args_and_targets() {
    check(
        "X_ERROR[t](0.1) 0",
        &expect![[r#"
        Circuit [0-17]:
            items:
                Instruction [0-17]:
                    name: X_ERROR
                    tag: t
                    args:
                        0.1
                    targets:
                        Target [16-17]:
                            kind: Qubit(0)"#]],
    );
}

#[test]
fn tag_on_block_instruction() {
    check(
        "REPEAT[t] 5 {\n    H 0\n}",
        &expect![[r#"
        Circuit [0-23]:
            items:
                Block [0-23]:
                    block_instruction: Instruction [0-11]:
                        name: REPEAT
                        tag: t
                        args: <empty>
                        targets:
                            Target [10-11]:
                                kind: Qubit(5)
                    items:
                        Instruction [18-21]:
                            name: H
                            tag: <none>
                            args: <empty>
                            targets:
                                Target [20-21]:
                                    kind: Qubit(0)"#]],
    );
}

#[test]
fn tag_in_target_position_is_error() {
    // A tag is only valid immediately after the instruction name, not later in
    // the target list.
    check("H 0 [t]", &expect![[r#"
        Stim.Parser.ExpectedToken

          x expected newline, found tag
           ,----
         1 | H 0 [t]
           :     ^^^
           `----
    "#]]);
}
