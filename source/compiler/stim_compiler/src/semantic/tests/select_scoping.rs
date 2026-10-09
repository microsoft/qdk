// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn require_inside_repeat_without_select_yields_error() {
    let source = indoc! {"
                REPEAT 2 {
                    M 0
                    REQUIRE rec[-1]
                }
        "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.InstructionOutsideSelectBlock

              x REQUIRE must appear inside a SELECT block
               ,-[3:5]
             2 |     M 0
             3 |     REQUIRE rec[-1]
               :     ^^^^^^^^^^^^^^^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn require_outside_select_yields_error() {
    let source = indoc! {"
        M 0
        REQUIRE rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.InstructionOutsideSelectBlock

              x REQUIRE must appear inside a SELECT block
               ,-[2:1]
             1 | M 0
             2 | REQUIRE rec[-1]
               : ^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn require_before_measurement_in_select_yields_error() {
    let source = indoc! {"
                M 0
                SELECT {
                    REQUIRE rec[-1]
                    M 1
                }
        "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.AllMeasurementRecordsOutOfScope

              x all measurement records referenced by REQUIRE are out of scope
               ,-[3:5]
             2 | SELECT {
             3 |     REQUIRE rec[-1]
               :     ^^^^^^^^^^^^^^^
             4 |     M 1
               `----
        "#]],
    );
}

#[test]
fn require_with_all_measurement_records_out_of_scope_yields_error() {
    let source = indoc! {"
                M 0
                SELECT {
                    M 1
                    REQUIRE rec[-2]
                }
        "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.AllMeasurementRecordsOutOfScope

              x all measurement records referenced by REQUIRE are out of scope
               ,-[4:5]
             3 |     M 1
             4 |     REQUIRE rec[-2]
               :     ^^^^^^^^^^^^^^^
             5 | }
               `----
        "#]],
    );
}

#[test]
fn require_with_at_least_one_record_in_scope_lowers() {
    let source = indoc! {"
                M 0
                SELECT {
                    M 1
                    REQUIRE rec[-1] rec[-2]
                }
        "};
    check(
        source,
        &expect![[r#"
            Circuit [0-51]:
                items:
                    [0-3] SingleQubitMeasurement {
                        reset: false,
                        observable: Z,
                        readout_noise: 0.0,
                        negated: false,
                        qubit: 0,
                    }
                    SelectBlock:
                        body:
                            [17-20] SingleQubitMeasurement {
                                reset: false,
                                observable: Z,
                                readout_noise: 0.0,
                                negated: false,
                                qubit: 1,
                            }
                            [25-48] Require {
                                records: [
                                    NegatableMeasurementRecord {
                                        record: MeasurementRecord {
                                            offset: 1,
                                            span: Span {
                                                lo: 33,
                                                hi: 40,
                                            },
                                        },
                                        negated: false,
                                    },
                                    NegatableMeasurementRecord {
                                        record: MeasurementRecord {
                                            offset: 2,
                                            span: Span {
                                                lo: 41,
                                                hi: 48,
                                            },
                                        },
                                        negated: false,
                                    },
                                ],
                            }"#]],
    );
}

#[test]
fn measure_reset_produces_record_in_select() {
    // MR produces a measurement record.
    let source = indoc! {"
                SELECT {
                    MR 0
                    REQUIRE rec[-1]
                }
        "};
    check(
        source,
        &expect![[r#"
            Circuit [0-40]:
                items:
                    SelectBlock:
                        body:
                            [13-17] SingleQubitMeasurement {
                                reset: true,
                                observable: Z,
                                readout_noise: 0.0,
                                negated: false,
                                qubit: 0,
                            }
                            [22-37] Require {
                                records: [
                                    NegatableMeasurementRecord {
                                        record: MeasurementRecord {
                                            offset: 1,
                                            span: Span {
                                                lo: 30,
                                                hi: 37,
                                            },
                                        },
                                        negated: false,
                                    },
                                ],
                            }"#]],
    );
}

#[test]
fn pair_measurement_produces_record_in_select() {
    let source = indoc! {"
                SELECT {
                    MZZ 0 1
                    REQUIRE rec[-1]
                }
        "};
    check(
        source,
        &expect![[r#"
            Circuit [0-43]:
                items:
                    SelectBlock:
                        body:
                            [13-20] TwoQubitMeasurement {
                                readout_noise: 0.0,
                                observable: ZZ,
                                negated: false,
                                q0: 0,
                                q1: 1,
                            }
                            [25-40] Require {
                                records: [
                                    NegatableMeasurementRecord {
                                        record: MeasurementRecord {
                                            offset: 1,
                                            span: Span {
                                                lo: 33,
                                                hi: 40,
                                            },
                                        },
                                        negated: false,
                                    },
                                ],
                            }"#]],
    );
}

#[test]
fn require_with_record_from_outer_select_yields_error() {
    let source = indoc! {"
                SELECT {
                    M 0
                    SELECT {
                        REQUIRE rec[-1]
                    }
                }
        "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.AllMeasurementRecordsOutOfScope

              x all measurement records referenced by REQUIRE are out of scope
               ,-[4:9]
             3 |     SELECT {
             4 |         REQUIRE rec[-1]
               :         ^^^^^^^^^^^^^^^
             5 |     }
               `----
        "#]],
    );
}

#[test]
fn require_with_record_from_sibling_select_yields_error() {
    let source = indoc! {"
                SELECT {
                    M 0
                }
                SELECT {
                    M 1
                    REQUIRE rec[-2]
                }
        "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.AllMeasurementRecordsOutOfScope

              x all measurement records referenced by REQUIRE are out of scope
               ,-[6:5]
             5 |     M 1
             6 |     REQUIRE rec[-2]
               :     ^^^^^^^^^^^^^^^
             7 | }
               `----
        "#]],
    );
}

#[test]
fn require_with_multiple_records_out_of_scope_yields_error() {
    let source = indoc! {"
                SELECT {
                    M 0
                    M 1
                }
                SELECT {
                    M 2
                    REQUIRE rec[-2] rec[-3]
                }
        "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.AllMeasurementRecordsOutOfScope

              x all measurement records referenced by REQUIRE are out of scope
               ,-[7:5]
             6 |     M 2
             7 |     REQUIRE rec[-2] rec[-3]
               :     ^^^^^^^^^^^^^^^^^^^^^^^
             8 | }
               `----
        "#]],
    );
}

#[test]
fn notleaked_outside_select_yields_error() {
    let source = indoc! {"
    M 0
    NOTLEAKED rec[-1]
  "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.InstructionOutsideSelectBlock

              x NOTLEAKED must appear inside a SELECT block
               ,-[2:1]
             1 | M 0
             2 | NOTLEAKED rec[-1]
               : ^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn notleaked_with_all_measurement_records_out_of_scope_yields_error() {
    let source = indoc! {"
                M 0
                SELECT {
                    M 1
                    NOTLEAKED rec[-2]
                }
        "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.AllMeasurementRecordsOutOfScope

              x all measurement records referenced by NOTLEAKED are out of scope
               ,-[4:5]
             3 |     M 1
             4 |     NOTLEAKED rec[-2]
               :     ^^^^^^^^^^^^^^^^^
             5 | }
               `----
        "#]],
    );
}

#[test]
fn notleaked_with_at_least_one_record_in_scope_lowers() {
    let source = indoc! {"
                M 0
                SELECT {
                    M 1
                    NOTLEAKED rec[-1] rec[-2]
                }
        "};
    check(
        source,
        &expect![[r#"
            Circuit [0-53]:
                items:
                    [0-3] SingleQubitMeasurement {
                        reset: false,
                        observable: Z,
                        readout_noise: 0.0,
                        negated: false,
                        qubit: 0,
                    }
                    SelectBlock:
                        body:
                            [17-20] SingleQubitMeasurement {
                                reset: false,
                                observable: Z,
                                readout_noise: 0.0,
                                negated: false,
                                qubit: 1,
                            }
                            [25-50] NotLeaked {
                                records: [
                                    MeasurementRecord {
                                        offset: 1,
                                        span: Span {
                                            lo: 35,
                                            hi: 42,
                                        },
                                    },
                                    MeasurementRecord {
                                        offset: 2,
                                        span: Span {
                                            lo: 43,
                                            hi: 50,
                                        },
                                    },
                                ],
                            }"#]],
    );
}

#[test]
fn notleaked_with_multiple_records_out_of_scope_yields_error() {
    let source = indoc! {"
                M 0
                M 1
                SELECT {
                    M 2
                    NOTLEAKED rec[-2] rec[-3]
                }
        "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.AllMeasurementRecordsOutOfScope

              x all measurement records referenced by NOTLEAKED are out of scope
               ,-[5:5]
             4 |     M 2
             5 |     NOTLEAKED rec[-2] rec[-3]
               :     ^^^^^^^^^^^^^^^^^^^^^^^^^
             6 | }
               `----
        "#]],
    );
}
