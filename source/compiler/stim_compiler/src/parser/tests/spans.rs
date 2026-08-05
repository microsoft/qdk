// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn instruction_span_with_no_targets() {
    // With no targets, the instruction span ends at the name.
    check(
        "S",
        &expect![[r#"
        Circuit [0-1]:
            items:
                Instruction [0-1]:
                    name: S
                    tag: <none>
                    args: <empty>
                    targets: <empty>"#]],
    );
}

#[test]
fn instruction_span_extends_to_last_target() {
    check(
        "CZ 7 8 9",
        &expect![[r#"
        Circuit [0-8]:
            items:
                Instruction [0-8]:
                    name: CZ
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [3-4]:
                            kind: Qubit(7)
                        Target [5-6]:
                            kind: Qubit(8)
                        Target [7-8]:
                            kind: Qubit(9)"#]],
    );
}

#[test]
fn negated_target_span_includes_the_bang() {
    check(
        "MX !3",
        &expect![[r#"
        Circuit [0-5]:
            items:
                Instruction [0-5]:
                    name: MX
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [3-5]:
                            kind: Qubit(-3)"#]],
    );
}

#[test]
fn block_span_covers_through_closing_brace() {
    check(
        indoc! {"
            REPEAT 2 {
              TICK
            }
        "},
        &expect![[r#"
            Circuit [0-20]:
                items:
                    Block [0-19]:
                        block_instruction: Instruction [0-8]:
                            name: REPEAT
                            tag: <none>
                            args: <empty>
                            targets:
                                Target [7-8]:
                                    kind: Qubit(2)
                        items:
                            Instruction [13-17]:
                                name: TICK
                                tag: <none>
                                args: <empty>
                                targets: <empty>"#]],
    );
}

#[test]
fn span_includes_args_when_no_targets() {
    // Paren args extend the instruction span even when there are no targets.
    check(
        "X_ERROR(0.1)\n",
        &expect![[r#"
            Circuit [0-13]:
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
fn span_includes_tag_when_no_targets() {
    // A tag extends the instruction span even when there are no targets.
    check(
        "H[tag]\n",
        &expect![[r#"
        Circuit [0-7]:
            items:
                Instruction [0-6]:
                    name: H
                    tag: tag
                    args: <empty>
                    targets: <empty>"#]],
    );
}

#[test]
fn span_extends_past_tag_and_args_to_target() {
    // Once there is a target, the span reaches it, covering the tag and args
    // that precede it.
    check(
        "X_ERROR[t](0.1) 5\n",
        &expect![[r#"
        Circuit [0-18]:
            items:
                Instruction [0-17]:
                    name: X_ERROR
                    tag: t
                    args:
                        0.1
                    targets:
                        Target [16-17]:
                            kind: Qubit(5)"#]],
    );
}

#[test]
fn rec_target_span_covers_full_token() {
    // The target span is the whole "rec[-1]" token, not just the index.
    check(
        "DETECTOR rec[-1]",
        &expect![[r#"
        Circuit [0-16]:
            items:
                Instruction [0-16]:
                    name: DETECTOR
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [9-16]:
                            kind: MeasurementRecord(1)"#]],
    );
}
