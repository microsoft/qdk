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
                    RepeatBlock [0-18]:
                        count: 3
                        body:
                            Instruction [13-16]:
                                name_span: [13-14]
                                kind: SingleQubitGate {
                                    q: 0 [15-16],
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
                    SelectBlock [0-16]:
                        body:
                            Instruction [11-14]:
                                name_span: [11-12]
                                kind: SingleQubitGate {
                                    q: 0 [13-14],
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
                    RepeatBlock [0-12]:
                        count: 2
                        body: <empty>
                    SelectBlock [13-23]:
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
                    RepeatBlock [0-20]:
                        count: 2
                        body:
                            Instruction [15-18]:
                                name_span: [15-16]
                                kind: SingleQubitGate {
                                    q: 0 [17-18],
                                    gate: X,
                                }
                    SelectBlock [21-39]:
                        body:
                            Instruction [34-37]:
                                name_span: [34-35]
                                kind: SingleQubitGate {
                                    q: 1 [36-37],
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
                    RepeatBlock [0-45]:
                        count: 2
                        body:
                            RepeatBlock [15-43]:
                                count: 3
                                body:
                                    Instruction [34-37]:
                                        name_span: [34-35]
                                        kind: SingleQubitGate {
                                            q: 0 [36-37],
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
                    RepeatBlock [0-43]:
                        count: 2
                        body:
                            SelectBlock [15-41]:
                                body:
                                    Instruction [32-35]:
                                        name_span: [32-33]
                                        kind: SingleQubitGate {
                                            q: 0 [34-35],
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
                    SelectBlock [0-43]:
                        body:
                            RepeatBlock [13-41]:
                                count: 2
                                body:
                                    Instruction [32-35]:
                                        name_span: [32-33]
                                        kind: SingleQubitGate {
                                            q: 0 [34-35],
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
                    SelectBlock [0-41]:
                        body:
                            SelectBlock [13-39]:
                                body:
                                    Instruction [30-33]:
                                        name_span: [30-31]
                                        kind: SingleQubitGate {
                                            q: 0 [32-33],
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
                    RepeatBlock [0-78]:
                        count: 2
                        body:
                            SelectBlock [13-76]:
                                body:
                                    RepeatBlock [26-52]:
                                        count: 3
                                        body:
                                            Instruction [43-46]:
                                                name_span: [43-44]
                                                kind: SingleQubitMeasurement {
                                                    reset: false,
                                                    observable: Z,
                                                    readout_noise: 0.0 [0-0],
                                                    negated: false,
                                                    q: 0 [45-46],
                                                }
                                    Instruction [57-72]:
                                        name_span: [57-64]
                                        kind: Require {
                                            records: [
                                                NegatableMeasurementRecord {
                                                    record: rec[-1] [65-72],
                                                    negated: false,
                                                },
                                            ],
                                        }"#]],
    );
}
