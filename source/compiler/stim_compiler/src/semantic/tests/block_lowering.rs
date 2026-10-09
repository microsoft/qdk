// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn repeat_block_lowers() {
    let source = indoc! {"
        REPEAT 3 {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
            Circuit [0-19]:
                items:
                    RepeatBlock:
                        count: 3
                        body:
                            [13-16] SingleQubitGate {
                                qubit: 0,
                                gate: X,
                            }"#]],
    );
}

#[test]
fn select_block_lowers() {
    let source = indoc! {"
        SELECT {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-17]:
            items:
                SelectBlock:
                    body:
                        [11-14] SingleQubitGate {
                            qubit: 0,
                            gate: X,
                        }"#]],
    );
}

#[test]
fn empty_blocks_lower() {
    let source = indoc! {"
                REPEAT 2 {
                }
                SELECT {
                }
        "};
    check(
        source,
        &expect![[r#"
        Circuit [0-24]:
            items:
                RepeatBlock:
                    count: 2
                    body: <empty>
                SelectBlock:
                    body: <empty>"#]],
    );
}

#[test]
fn sibling_blocks_lower() {
    let source = indoc! {"
                REPEAT 2 {
                    X 0
                }
                SELECT {
                    Z 1
                }
        "};
    check(
        source,
        &expect![[r#"
        Circuit [0-40]:
            items:
                RepeatBlock:
                    count: 2
                    body:
                        [15-18] SingleQubitGate {
                            qubit: 0,
                            gate: X,
                        }
                SelectBlock:
                    body:
                        [34-37] SingleQubitGate {
                            qubit: 1,
                            gate: Z,
                        }"#]],
    );
}

#[test]
fn repeat_inside_repeat_lowers() {
    let source = indoc! {"
                REPEAT 2 {
                    REPEAT 3 {
                        X 0
                    }
                }
        "};
    check(
        source,
        &expect![[r#"
        Circuit [0-46]:
            items:
                RepeatBlock:
                    count: 2
                    body:
                        RepeatBlock:
                            count: 3
                            body:
                                [34-37] SingleQubitGate {
                                    qubit: 0,
                                    gate: X,
                                }"#]],
    );
}

#[test]
fn select_inside_repeat_lowers() {
    let source = indoc! {"
                REPEAT 2 {
                    SELECT {
                        X 0
                    }
                }
        "};
    check(
        source,
        &expect![[r#"
        Circuit [0-44]:
            items:
                RepeatBlock:
                    count: 2
                    body:
                        SelectBlock:
                            body:
                                [32-35] SingleQubitGate {
                                    qubit: 0,
                                    gate: X,
                                }"#]],
    );
}

#[test]
fn repeat_inside_select_lowers() {
    let source = indoc! {"
                SELECT {
                    REPEAT 2 {
                        X 0
                    }
                }
        "};
    check(
        source,
        &expect![[r#"
        Circuit [0-44]:
            items:
                SelectBlock:
                    body:
                        RepeatBlock:
                            count: 2
                            body:
                                [32-35] SingleQubitGate {
                                    qubit: 0,
                                    gate: X,
                                }"#]],
    );
}

#[test]
fn select_inside_select_lowers() {
    let source = indoc! {"
                SELECT {
                    SELECT {
                        X 0
                    }
                }
        "};
    check(
        source,
        &expect![[r#"
        Circuit [0-42]:
            items:
                SelectBlock:
                    body:
                        SelectBlock:
                            body:
                                [30-33] SingleQubitGate {
                                    qubit: 0,
                                    gate: X,
                                }"#]],
    );
}

#[test]
fn alternating_nested_blocks_lower() {
    let source = indoc! {"
        REPEAT 2 {
          SELECT {
            REPEAT 3 {
              M 0
            }
            REQUIRE rec[-1]
          }
        }
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-79]:
            items:
                RepeatBlock:
                    count: 2
                    body:
                        SelectBlock:
                            body:
                                RepeatBlock:
                                    count: 3
                                    body:
                                        [43-46] SingleQubitMeasurement {
                                            reset: false,
                                            observable: Z,
                                            readout_noise: 0.0,
                                            negated: false,
                                            qubit: 0,
                                        }
                                [57-72] Require {
                                    records: [
                                        NegatableMeasurementRecord {
                                            record: MeasurementRecord {
                                                offset: 1,
                                                span: Span {
                                                    lo: 65,
                                                    hi: 72,
                                                },
                                            },
                                            negated: false,
                                        },
                                    ],
                                }"#]],
    );
}
