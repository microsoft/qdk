// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn classical_control_without_prior_record_is_out_of_bounds() {
    check(
        "CX rec[-1] 1",
        &expect![[r#"
            Qdk.Stim.Semantic.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,----
             1 | CX rec[-1] 1
               :    ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn classical_control_just_beyond_available_records_is_out_of_bounds() {
    let source = indoc! {"
        M 0
        CX rec[-2] 1
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[2:4]
             1 | M 0
             2 | CX rec[-2] 1
               :    ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn require_reports_each_out_of_bounds_record() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE rec[-1] rec[-2] rec[-3]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[3:19]
             2 |   M 0
             3 |   REQUIRE rec[-1] rec[-2] rec[-3]
               :                   ^^^^^^^
             4 | }
               `----

            Qdk.Stim.Semantic.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[3:27]
             2 |   M 0
             3 |   REQUIRE rec[-1] rec[-2] rec[-3]
               :                           ^^^^^^^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn notleaked_just_beyond_available_records_is_out_of_bounds() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[3:13]
             2 |   M 0
             3 |   NOTLEAKED rec[-2]
               :             ^^^^^^^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn detector_record_is_out_of_bounds() {
    check(
        "DETECTOR rec[-1]",
        &expect![[r#"
        Qdk.Stim.Semantic.MeasurementRecordOutOfBounds

          x measurement record is out of bounds
           ,----
         1 | DETECTOR rec[-1]
           :          ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_record_is_out_of_bounds() {
    check(
        "OBSERVABLE_INCLUDE(0) rec[-1]",
        &expect![[r#"
        Qdk.Stim.Semantic.MeasurementRecordOutOfBounds

          x measurement record is out of bounds
           ,----
         1 | OBSERVABLE_INCLUDE(0) rec[-1]
           :                       ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn reset_and_non_heralded_noise_do_not_append_records() {
    let source = indoc! {"
        R 0
        X_ERROR(0.1) 1
        CX rec[-1] 2
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[3:4]
             2 | X_ERROR(0.1) 1
             3 | CX rec[-1] 2
               :    ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn measurement_instructions_append_expected_records() {
    // the first record is in bounds, the second isn't
    let source = indoc! {"
        M 0 1
        MZZ 2 3 4 5
        MPP X6 Y7*Z8
        PEEK_LOSS 9 10
        CX rec[-8] 11
        CX rec[-9] 12
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[6:4]
             5 | CX rec[-8] 11
             6 | CX rec[-9] 12
               :    ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn heralded_and_mpad_append_expected_records() {
    // the first record is in bounds, the second isn't
    let source = indoc! {"
        MPAD 0 1
        HERALDED_ERASE(0.1) 2 3
        HERALDED_PAULI_CHANNEL_1(0.1, 0, 0, 0) 4 5
        DETECTOR rec[-6]
        DETECTOR rec[-7]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[5:10]
             4 | DETECTOR rec[-6]
             5 | DETECTOR rec[-7]
               :          ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn record_reference_before_first_repeat_producer_is_out_of_bounds() {
    let source = indoc! {"
        REPEAT 2 {
          CX rec[-1] 1
          M 0
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[2:6]
             1 | REPEAT 2 {
             2 |   CX rec[-1] 1
               :      ^^^^^^^
             3 |   M 0
               `----
        "#]],
    );
}

#[test]
fn repeats_append_expected_records() {
    // only the last record is out of bounds
    let source = indoc! {"
        REPEAT 2 {
          M 0
        }
        CX rec[-2] 3
        REPEAT 2 {
          REPEAT 3 {
            M 1 2
          }
        }
        CX rec[-14] 4
        CX rec[-15] 5
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
                ,-[11:4]
             10 | CX rec[-14] 4
             11 | CX rec[-15] 5
                :    ^^^^^^^^
                `----
        "#]],
    );
}

#[test]
fn record_count_overflow_yields_error() {
    let nested_repeats = indoc! {"
        REPEAT 4294967295 {
          REPEAT 4294967295 {
            M 0 1
          }
        }
    "};
    check(
        nested_repeats,
        &expect![[r#"
            Qdk.Stim.Semantic.MeasurementRecordCounterOverflow

              x the circuit exceeds the limit of 18,446,744,073,709,551,615 measurement
              | records
        "#]],
    );
}
