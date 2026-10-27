// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

// Single-qubit gates

#[test]
fn single_qubit_gates_reject_pauli_target() {
    check(
        "H X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: H
           ,----
         1 | H X0
           :   ^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_gates_reject_pauli_product_target() {
    check(
        "H X1*Y2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: H
           ,----
         1 | H X1*Y2
           :   ^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_gates_reject_loss_target() {
    check(
        "H L3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: H
           ,----
         1 | H L3
           :   ^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_gates_reject_sweep_target() {
    check(
        "H sweep[4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: H
           ,----
         1 | H sweep[4]
           :   ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_gates_reject_record_target() {
    check(
        "H rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: H
           ,----
         1 | H rec[-5]
           :   ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_gates_reject_negated_qubit_targets() {
    check(
        "H !0",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: H
           ,----
         1 | H !0
           :   ^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_gates_report_every_invalid_target() {
    check(
        "H X0 1 L3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: H
           ,----
         1 | H X0 1 L3
           :   ^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: H
           ,----
         1 | H X0 1 L3
           :        ^^
           `----
    "#]],
    );
}

// Two-qubit gates

#[test]
fn two_qubit_gates_reject_pauli_target() {
    check(
        "SWAP 0 X1",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: SWAP
           ,----
         1 | SWAP 0 X1
           :        ^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_gates_reject_pauli_product_target() {
    check(
        "SWAP 0 X2*Y3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: SWAP
           ,----
         1 | SWAP 0 X2*Y3
           :        ^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_gates_reject_loss_target() {
    check(
        "SWAP 0 L4",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: SWAP
           ,----
         1 | SWAP 0 L4
           :        ^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_gates_reject_sweep_target() {
    check(
        "SWAP 0 sweep[5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: SWAP
           ,----
         1 | SWAP 0 sweep[5]
           :        ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_gates_reject_record_target() {
    check(
        "SWAP 0 rec[-6]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: SWAP
           ,----
         1 | SWAP 0 rec[-6]
           :        ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_gates_reject_negated_qubit_targets() {
    check(
        "SWAP !0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: SWAP
           ,----
         1 | SWAP !0 1
           :      ^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_gates_report_every_invalid_target_pair() {
    check(
        "SWAP 0 X1 2 3 L4 0",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: SWAP
               ,----
             1 | SWAP 0 X1 2 3 L4 0
               :        ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: SWAP
               ,----
             1 | SWAP 0 X1 2 3 L4 0
               :               ^^
               `----
        "#]],
    );
}

// Classically controllable gates

#[test]
fn classically_controllable_gates_with_records_in_first_position_reject_pauli_target() {
    check(
        indoc! {"
            M 0 1
            CX rec[-1] X0
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CX
               ,-[2:12]
             1 | M 0 1
             2 | CX rec[-1] X0
               :            ^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_first_position_reject_pauli_product_target() {
    check(
        indoc! {"
            M 0 1
            CX rec[-1] X1*Y2
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CX
               ,-[2:12]
             1 | M 0 1
             2 | CX rec[-1] X1*Y2
               :            ^^^^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_first_position_reject_loss_target() {
    check(
        indoc! {"
            M 0 1
            CX rec[-1] L3
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CX
               ,-[2:12]
             1 | M 0 1
             2 | CX rec[-1] L3
               :            ^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_first_position_reject_sweep_target() {
    check(
        indoc! {"
            M 0 1
            CX rec[-1] sweep[4]
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CX
               ,-[2:12]
             1 | M 0 1
             2 | CX rec[-1] sweep[4]
               :            ^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_first_position_reject_record_target() {
    check(
        indoc! {"
            M 0 1
            CX rec[-1] rec[-2]
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.BothTargetsAreMeasurementRecords

              x controlled instruction CX requires a qubit target, but both targets are
              | measurement records
               ,-[2:4]
             1 | M 0 1
             2 | CX rec[-1] rec[-2]
               :    ^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_second_position_reject_pauli_target() {
    check(
        indoc! {"
            M 0 1
            XCZ X0 rec[-1]
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: XCZ
               ,-[2:5]
             1 | M 0 1
             2 | XCZ X0 rec[-1]
               :     ^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_second_position_reject_pauli_product_target() {
    check(
        indoc! {"
            M 0 1
            XCZ X1*Y2 rec[-1]
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: XCZ
               ,-[2:5]
             1 | M 0 1
             2 | XCZ X1*Y2 rec[-1]
               :     ^^^^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_second_position_reject_loss_target() {
    check(
        indoc! {"
            M 0 1
            XCZ L3 rec[-1]
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: XCZ
               ,-[2:5]
             1 | M 0 1
             2 | XCZ L3 rec[-1]
               :     ^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_second_position_reject_sweep_target() {
    check(
        indoc! {"
            M 0 1
            XCZ sweep[4] rec[-1]
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: XCZ
               ,-[2:5]
             1 | M 0 1
             2 | XCZ sweep[4] rec[-1]
               :     ^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_second_position_reject_record_target() {
    check(
        indoc! {"
            M 0 1
            XCZ rec[-2] rec[-1]
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.BothTargetsAreMeasurementRecords

              x controlled instruction XCZ requires a qubit target, but both targets are
              | measurement records
               ,-[2:5]
             1 | M 0 1
             2 | XCZ rec[-2] rec[-1]
               :     ^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_first_position_reject_negated_record() {
    check(
        indoc! {"
            M 0
            CX !rec[-1] 1
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.NegatedTarget

              x target cannot be negated in instruction: CX
               ,-[2:4]
             1 | M 0
             2 | CX !rec[-1] 1
               :    ^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_first_position_reject_negated_qubit() {
    check(
        indoc! {"
            M 0
            CX rec[-1] !1
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.NegatedTarget

              x target cannot be negated in instruction: CX
               ,-[2:12]
             1 | M 0
             2 | CX rec[-1] !1
               :            ^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_second_position_reject_negated_record() {
    check(
        indoc! {"
            M 0
            XCZ 1 !rec[-1]
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.NegatedTarget

              x target cannot be negated in instruction: XCZ
               ,-[2:7]
             1 | M 0
             2 | XCZ 1 !rec[-1]
               :       ^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_second_position_reject_negated_qubit() {
    check(
        indoc! {"
            M 0
            XCZ !0 rec[-1]
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.NegatedTarget

              x target cannot be negated in instruction: XCZ
               ,-[2:5]
             1 | M 0
             2 | XCZ !0 rec[-1]
               :     ^^
               `----
        "#]],
    );
}

#[test]
fn gates_allowing_records_in_first_position_reject_records_in_second_position() {
    check(
        indoc! {"
            M 0
            CX 1 rec[-1]
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.MisplacedMeasurementRecord

              x measurement record target in an unsupported position in instruction: CX
               ,-[2:6]
             1 | M 0
             2 | CX 1 rec[-1]
               :      ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn gates_allowing_records_in_second_position_reject_records_in_first_position() {
    check(
        indoc! {"
            M 0
            XCZ rec[-1] 1
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.MisplacedMeasurementRecord

              x measurement record target in an unsupported position in instruction: XCZ
               ,-[2:5]
             1 | M 0
             2 | XCZ rec[-1] 1
               :     ^^^^^^^
               `----
        "#]],
    );
}

// Three-qubit gates

#[test]
fn three_qubit_gates_reject_pauli_target() {
    check(
        "CCX 0 1 X2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: CCX
           ,----
         1 | CCX 0 1 X2
           :         ^^
           `----
    "#]],
    );
}

#[test]
fn three_qubit_gates_reject_pauli_product_target() {
    check(
        "CCX 0 1 X3*Y4",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: CCX
           ,----
         1 | CCX 0 1 X3*Y4
           :         ^^^^^
           `----
    "#]],
    );
}

#[test]
fn three_qubit_gates_reject_loss_target() {
    check(
        "CCX 0 1 L5",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: CCX
           ,----
         1 | CCX 0 1 L5
           :         ^^
           `----
    "#]],
    );
}

#[test]
fn three_qubit_gates_reject_sweep_target() {
    check(
        "CCX 0 1 sweep[6]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: CCX
           ,----
         1 | CCX 0 1 sweep[6]
           :         ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn three_qubit_gates_reject_record_target() {
    check(
        "CCX 0 1 rec[-7]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: CCX
           ,----
         1 | CCX 0 1 rec[-7]
           :         ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn three_qubit_gates_reject_negated_qubit_targets() {
    check(
        "CCX !0 1 2",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: CCX
           ,----
         1 | CCX !0 1 2
           :     ^^
           `----
    "#]],
    );
}

// Correlated errors

#[test]
fn correlated_errors_reject_qubit_target() {
    check(
        "E(0.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: E
           ,----
         1 | E(0.1) 0
           :        ^
           `----
    "#]],
    );
}

#[test]
fn correlated_errors_reject_pauli_product_target() {
    check(
        "E(0.1) X1*Y2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: E
           ,----
         1 | E(0.1) X1*Y2
           :        ^^^^^
           `----
    "#]],
    );
}

#[test]
fn correlated_errors_reject_sweep_target() {
    check(
        "E(0.1) sweep[3]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: E
           ,----
         1 | E(0.1) sweep[3]
           :        ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn correlated_errors_reject_record_target() {
    check(
        "E(0.1) rec[-4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: E
           ,----
         1 | E(0.1) rec[-4]
           :        ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn correlated_errors_reject_negated_pauli_targets() {
    check(
        "E(0.1) !X0",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: E
           ,----
         1 | E(0.1) !X0
           :        ^^^
           `----
    "#]],
    );
}

// Single-qubit noise

#[test]
fn single_qubit_noise_rejects_pauli_target() {
    check(
        "DEPOLARIZE1(0.1) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DEPOLARIZE1
           ,----
         1 | DEPOLARIZE1(0.1) X0
           :                  ^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_rejects_pauli_product_target() {
    check(
        "DEPOLARIZE1(0.1) X1*Y2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DEPOLARIZE1
           ,----
         1 | DEPOLARIZE1(0.1) X1*Y2
           :                  ^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_rejects_loss_target() {
    check(
        "DEPOLARIZE1(0.1) L3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DEPOLARIZE1
           ,----
         1 | DEPOLARIZE1(0.1) L3
           :                  ^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_rejects_sweep_target() {
    check(
        "DEPOLARIZE1(0.1) sweep[4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DEPOLARIZE1
           ,----
         1 | DEPOLARIZE1(0.1) sweep[4]
           :                  ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_rejects_record_target() {
    check(
        "DEPOLARIZE1(0.1) rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DEPOLARIZE1
           ,----
         1 | DEPOLARIZE1(0.1) rec[-5]
           :                  ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_rejects_negated_qubit_targets() {
    check(
        "DEPOLARIZE1(0.1) !0",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: DEPOLARIZE1
           ,----
         1 | DEPOLARIZE1(0.1) !0
           :                  ^^
           `----
    "#]],
    );
}

// Two-qubit noise

#[test]
fn two_qubit_noise_rejects_pauli_target() {
    check(
        "DEPOLARIZE2(0.1) 0 X1",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DEPOLARIZE2
           ,----
         1 | DEPOLARIZE2(0.1) 0 X1
           :                    ^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_rejects_pauli_product_target() {
    check(
        "DEPOLARIZE2(0.1) 0 X2*Y3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DEPOLARIZE2
           ,----
         1 | DEPOLARIZE2(0.1) 0 X2*Y3
           :                    ^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_rejects_loss_target() {
    check(
        "DEPOLARIZE2(0.1) 0 L4",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DEPOLARIZE2
           ,----
         1 | DEPOLARIZE2(0.1) 0 L4
           :                    ^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_rejects_sweep_target() {
    check(
        "DEPOLARIZE2(0.1) 0 sweep[5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DEPOLARIZE2
           ,----
         1 | DEPOLARIZE2(0.1) 0 sweep[5]
           :                    ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_rejects_record_target() {
    check(
        "DEPOLARIZE2(0.1) 0 rec[-6]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DEPOLARIZE2
           ,----
         1 | DEPOLARIZE2(0.1) 0 rec[-6]
           :                    ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_rejects_negated_qubit_targets() {
    check(
        "DEPOLARIZE2(0.1) !0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: DEPOLARIZE2
           ,----
         1 | DEPOLARIZE2(0.1) !0 1
           :                  ^^
           `----
    "#]],
    );
}

// Identity errors

#[test]
fn identity_errors_reject_pauli_target() {
    check(
        "I_ERROR X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: I_ERROR
           ,----
         1 | I_ERROR X0
           :         ^^
           `----
    "#]],
    );
}

#[test]
fn identity_errors_reject_pauli_product_target() {
    check(
        "I_ERROR X1*Y2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: I_ERROR
           ,----
         1 | I_ERROR X1*Y2
           :         ^^^^^
           `----
    "#]],
    );
}

#[test]
fn identity_errors_reject_loss_target() {
    check(
        "I_ERROR L3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: I_ERROR
           ,----
         1 | I_ERROR L3
           :         ^^
           `----
    "#]],
    );
}

#[test]
fn identity_errors_reject_sweep_target() {
    check(
        "I_ERROR sweep[4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: I_ERROR
           ,----
         1 | I_ERROR sweep[4]
           :         ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn identity_errors_reject_record_target() {
    check(
        "I_ERROR rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: I_ERROR
           ,----
         1 | I_ERROR rec[-5]
           :         ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn identity_errors_reject_negated_qubit_targets() {
    check(
        "I_ERROR !0",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: I_ERROR
           ,----
         1 | I_ERROR !0
           :         ^^
           `----
    "#]],
    );
}

// Measurements

#[test]
fn measurements_reject_pauli_target() {
    check(
        "M X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: M
           ,----
         1 | M X0
           :   ^^
           `----
    "#]],
    );
}

#[test]
fn measurements_reject_pauli_product_target() {
    check(
        "M X1*Y2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: M
           ,----
         1 | M X1*Y2
           :   ^^^^^
           `----
    "#]],
    );
}

#[test]
fn measurements_reject_loss_target() {
    check(
        "M L3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: M
           ,----
         1 | M L3
           :   ^^
           `----
    "#]],
    );
}

#[test]
fn measurements_reject_sweep_target() {
    check(
        "M sweep[4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: M
           ,----
         1 | M sweep[4]
           :   ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn measurements_reject_record_target() {
    check(
        "M rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: M
           ,----
         1 | M rec[-5]
           :   ^^^^^^^
           `----
    "#]],
    );
}

// Resets

#[test]
fn resets_reject_pauli_target() {
    check(
        "R X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R
           ,----
         1 | R X0
           :   ^^
           `----
    "#]],
    );
}

#[test]
fn resets_reject_pauli_product_target() {
    check(
        "R X1*Y2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R
           ,----
         1 | R X1*Y2
           :   ^^^^^
           `----
    "#]],
    );
}

#[test]
fn resets_reject_loss_target() {
    check(
        "R L3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R
           ,----
         1 | R L3
           :   ^^
           `----
    "#]],
    );
}

#[test]
fn resets_reject_sweep_target() {
    check(
        "R sweep[4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R
           ,----
         1 | R sweep[4]
           :   ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn resets_reject_record_target() {
    check(
        "R rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R
           ,----
         1 | R rec[-5]
           :   ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn resets_reject_negated_qubit_targets() {
    check(
        "R !0",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: R
           ,----
         1 | R !0
           :   ^^
           `----
    "#]],
    );
}

// Pair measurements

#[test]
fn pair_measurements_reject_pauli_target() {
    check(
        "MXX 0 X1",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MXX
           ,----
         1 | MXX 0 X1
           :       ^^
           `----
    "#]],
    );
}

#[test]
fn pair_measurements_reject_pauli_product_target() {
    check(
        "MXX 0 X2*Y3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MXX
           ,----
         1 | MXX 0 X2*Y3
           :       ^^^^^
           `----
    "#]],
    );
}

#[test]
fn pair_measurements_reject_loss_target() {
    check(
        "MXX 0 L4",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MXX
           ,----
         1 | MXX 0 L4
           :       ^^
           `----
    "#]],
    );
}

#[test]
fn pair_measurements_reject_sweep_target() {
    check(
        "MXX 0 sweep[5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MXX
           ,----
         1 | MXX 0 sweep[5]
           :       ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn pair_measurements_reject_record_target() {
    check(
        "MXX 0 rec[-6]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MXX
           ,----
         1 | MXX 0 rec[-6]
           :       ^^^^^^^
           `----
    "#]],
    );
}

// Pauli product measurements and gates

#[test]
fn pauli_product_instructions_reject_qubit_target() {
    check(
        "MPP 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPP
           ,----
         1 | MPP 0
           :     ^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_instructions_reject_loss_target() {
    check(
        "MPP L1",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPP
           ,----
         1 | MPP L1
           :     ^^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_instructions_reject_sweep_target() {
    check(
        "MPP sweep[2]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPP
           ,----
         1 | MPP sweep[2]
           :     ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_instructions_reject_record_target() {
    check(
        "MPP rec[-3]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPP
           ,----
         1 | MPP rec[-3]
           :     ^^^^^^^
           `----
    "#]],
    );
}

// REPEAT

#[test]
fn repeat_rejects_pauli_count() {
    check(
        indoc! {"
            REPEAT X0 {
              X 0
            }
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REPEAT
               ,-[1:8]
             1 | REPEAT X0 {
               :        ^^
             2 |   X 0
               `----
        "#]],
    );
}

#[test]
fn repeat_rejects_pauli_product_count() {
    check(
        indoc! {"
            REPEAT X0*Y1 {
              X 0
            }
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REPEAT
               ,-[1:8]
             1 | REPEAT X0*Y1 {
               :        ^^^^^
             2 |   X 0
               `----
        "#]],
    );
}

#[test]
fn repeat_rejects_loss_count() {
    check(
        indoc! {"
            REPEAT L2 {
              X 0
            }
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REPEAT
               ,-[1:8]
             1 | REPEAT L2 {
               :        ^^
             2 |   X 0
               `----
        "#]],
    );
}

#[test]
fn repeat_rejects_sweep_count() {
    check(
        indoc! {"
            REPEAT sweep[3] {
              X 0
            }
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REPEAT
               ,-[1:8]
             1 | REPEAT sweep[3] {
               :        ^^^^^^^^
             2 |   X 0
               `----
        "#]],
    );
}

#[test]
fn repeat_rejects_record_count() {
    check(
        indoc! {"
            REPEAT rec[-4] {
              X 0
            }
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REPEAT
               ,-[1:8]
             1 | REPEAT rec[-4] {
               :        ^^^^^^^
             2 |   X 0
               `----
        "#]],
    );
}

#[test]
fn repeat_rejects_negated_count() {
    check(
        indoc! {"
            REPEAT !3 {
              X 0
            }
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REPEAT
               ,-[1:8]
             1 | REPEAT !3 {
               :        ^^
             2 |   X 0
               `----
        "#]],
    );
}

// SELECT

#[test]
fn select_conditions_reject_qubit_target() {
    check(
        indoc! {"
            SELECT {
              REQUIRE 0
            }
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[2:11]
             1 | SELECT {
             2 |   REQUIRE 0
               :           ^
             3 | }
               `----
        "#]],
    );
}

#[test]
fn select_conditions_reject_pauli_target() {
    check(
        indoc! {"
            SELECT {
              REQUIRE X1
            }
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[2:11]
             1 | SELECT {
             2 |   REQUIRE X1
               :           ^^
             3 | }
               `----
        "#]],
    );
}

#[test]
fn select_conditions_reject_pauli_product_target() {
    check(
        indoc! {"
            SELECT {
              REQUIRE X2*Y3
            }
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[2:11]
             1 | SELECT {
             2 |   REQUIRE X2*Y3
               :           ^^^^^
             3 | }
               `----
        "#]],
    );
}

#[test]
fn select_conditions_reject_loss_target() {
    check(
        indoc! {"
            SELECT {
              REQUIRE L4
            }
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[2:11]
             1 | SELECT {
             2 |   REQUIRE L4
               :           ^^
             3 | }
               `----
        "#]],
    );
}

#[test]
fn select_conditions_reject_sweep_target() {
    check(
        indoc! {"
            SELECT {
              REQUIRE sweep[5]
            }
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[2:11]
             1 | SELECT {
             2 |   REQUIRE sweep[5]
               :           ^^^^^^^^
             3 | }
               `----
        "#]],
    );
}

#[test]
fn notleaked_rejects_negated_record_targets() {
    check(
        indoc! {"
            SELECT {
              M 0
              NOTLEAKED !rec[-1]
            }
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.NegatedTarget

              x target cannot be negated in instruction: NOTLEAKED
               ,-[3:13]
             2 |   M 0
             3 |   NOTLEAKED !rec[-1]
               :             ^^^^^^^^
             4 | }
               `----
        "#]],
    );
}

// PEEK_LOSS

#[test]
fn peek_loss_rejects_pauli_target() {
    check(
        "PEEK_LOSS X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: PEEK_LOSS
           ,----
         1 | PEEK_LOSS X0
           :           ^^
           `----
    "#]],
    );
}

#[test]
fn peek_loss_rejects_pauli_product_target() {
    check(
        "PEEK_LOSS X1*Y2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: PEEK_LOSS
           ,----
         1 | PEEK_LOSS X1*Y2
           :           ^^^^^
           `----
    "#]],
    );
}

#[test]
fn peek_loss_rejects_loss_target() {
    check(
        "PEEK_LOSS L3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: PEEK_LOSS
           ,----
         1 | PEEK_LOSS L3
           :           ^^
           `----
    "#]],
    );
}

#[test]
fn peek_loss_rejects_sweep_target() {
    check(
        "PEEK_LOSS sweep[4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: PEEK_LOSS
           ,----
         1 | PEEK_LOSS sweep[4]
           :           ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn peek_loss_rejects_record_target() {
    check(
        "PEEK_LOSS rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: PEEK_LOSS
           ,----
         1 | PEEK_LOSS rec[-5]
           :           ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn peek_loss_rejects_negated_qubit_targets() {
    check(
        "PEEK_LOSS !0",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: PEEK_LOSS
           ,----
         1 | PEEK_LOSS !0
           :           ^^
           `----
    "#]],
    );
}

// DETECTOR

#[test]
fn detector_rejects_qubit_target() {
    check(
        "DETECTOR 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DETECTOR
           ,----
         1 | DETECTOR 0
           :          ^
           `----
    "#]],
    );
}

#[test]
fn detector_rejects_pauli_target() {
    check(
        "DETECTOR X1",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DETECTOR
           ,----
         1 | DETECTOR X1
           :          ^^
           `----
    "#]],
    );
}

#[test]
fn detector_rejects_pauli_product_target() {
    check(
        "DETECTOR X2*Y3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DETECTOR
           ,----
         1 | DETECTOR X2*Y3
           :          ^^^^^
           `----
    "#]],
    );
}

#[test]
fn detector_rejects_loss_target() {
    check(
        "DETECTOR L4",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DETECTOR
           ,----
         1 | DETECTOR L4
           :          ^^
           `----
    "#]],
    );
}

#[test]
fn detector_rejects_sweep_target() {
    check(
        "DETECTOR sweep[5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DETECTOR
           ,----
         1 | DETECTOR sweep[5]
           :          ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn detector_rejects_negated_record_targets() {
    check(
        indoc! {"
            M 0
            DETECTOR !rec[-1]
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.NegatedTarget

              x target cannot be negated in instruction: DETECTOR
               ,-[2:10]
             1 | M 0
             2 | DETECTOR !rec[-1]
               :          ^^^^^^^^
               `----
        "#]],
    );
}

// MPAD

#[test]
fn measurement_padding_rejects_pauli_target() {
    check(
        "MPAD X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPAD
           ,----
         1 | MPAD X0
           :      ^^
           `----
    "#]],
    );
}

#[test]
fn measurement_padding_rejects_pauli_product_target() {
    check(
        "MPAD X1*Y2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPAD
           ,----
         1 | MPAD X1*Y2
           :      ^^^^^
           `----
    "#]],
    );
}

#[test]
fn measurement_padding_rejects_loss_target() {
    check(
        "MPAD L3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPAD
           ,----
         1 | MPAD L3
           :      ^^
           `----
    "#]],
    );
}

#[test]
fn measurement_padding_rejects_sweep_target() {
    check(
        "MPAD sweep[4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPAD
           ,----
         1 | MPAD sweep[4]
           :      ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn measurement_padding_rejects_record_target() {
    check(
        "MPAD rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPAD
           ,----
         1 | MPAD rec[-5]
           :      ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn measurement_padding_rejects_negated_qubit_targets() {
    check(
        "MPAD !0",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: MPAD
           ,----
         1 | MPAD !0
           :      ^^
           `----
    "#]],
    );
}

#[test]
fn measurement_padding_reports_every_invalid_target() {
    check(
        "MPAD X0 0 2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPAD
           ,----
         1 | MPAD X0 0 2
           :      ^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPAD
           ,----
         1 | MPAD X0 0 2
           :           ^
           `----
    "#]],
    );
}

// OBSERVABLE_INCLUDE

#[test]
fn observable_include_rejects_qubit_target() {
    check(
        "OBSERVABLE_INCLUDE(0) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: OBSERVABLE_INCLUDE
           ,----
         1 | OBSERVABLE_INCLUDE(0) 0
           :                       ^
           `----
    "#]],
    );
}

#[test]
fn observable_include_rejects_pauli_product_target() {
    check(
        "OBSERVABLE_INCLUDE(0) X1*Y2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: OBSERVABLE_INCLUDE
           ,----
         1 | OBSERVABLE_INCLUDE(0) X1*Y2
           :                       ^^^^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_rejects_loss_target() {
    check(
        "OBSERVABLE_INCLUDE(0) L3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: OBSERVABLE_INCLUDE
           ,----
         1 | OBSERVABLE_INCLUDE(0) L3
           :                       ^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_rejects_sweep_target() {
    check(
        "OBSERVABLE_INCLUDE(0) sweep[4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: OBSERVABLE_INCLUDE
           ,----
         1 | OBSERVABLE_INCLUDE(0) sweep[4]
           :                       ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_rejects_negated_pauli_target() {
    check(
        "OBSERVABLE_INCLUDE(0) !X1",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: OBSERVABLE_INCLUDE
           ,----
         1 | OBSERVABLE_INCLUDE(0) !X1
           :                       ^^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_rejects_negated_record_target() {
    check(
        indoc! {"
            M 0
            OBSERVABLE_INCLUDE(0) !rec[-1]
        "},
        &expect![[r#"
            Qdk.Stim.Semantic.NegatedTarget

              x target cannot be negated in instruction: OBSERVABLE_INCLUDE
               ,-[2:23]
             1 | M 0
             2 | OBSERVABLE_INCLUDE(0) !rec[-1]
               :                       ^^^^^^^^
               `----
        "#]],
    );
}

// QUBIT_COORDS

#[test]
fn qubit_coordinates_reject_pauli_target() {
    check(
        "QUBIT_COORDS(1,2) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: QUBIT_COORDS
           ,----
         1 | QUBIT_COORDS(1,2) X0
           :                   ^^
           `----
    "#]],
    );
}

#[test]
fn qubit_coordinates_reject_pauli_product_target() {
    check(
        "QUBIT_COORDS(1,2) X1*Y2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: QUBIT_COORDS
           ,----
         1 | QUBIT_COORDS(1,2) X1*Y2
           :                   ^^^^^
           `----
    "#]],
    );
}

#[test]
fn qubit_coordinates_reject_loss_target() {
    check(
        "QUBIT_COORDS(1,2) L3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: QUBIT_COORDS
           ,----
         1 | QUBIT_COORDS(1,2) L3
           :                   ^^
           `----
    "#]],
    );
}

#[test]
fn qubit_coordinates_reject_sweep_target() {
    check(
        "QUBIT_COORDS(1,2) sweep[4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: QUBIT_COORDS
           ,----
         1 | QUBIT_COORDS(1,2) sweep[4]
           :                   ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn qubit_coordinates_reject_record_target() {
    check(
        "QUBIT_COORDS(1,2) rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: QUBIT_COORDS
           ,----
         1 | QUBIT_COORDS(1,2) rec[-5]
           :                   ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn qubit_coordinates_reject_negated_qubit_targets() {
    check(
        "QUBIT_COORDS(1,2) !0",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: QUBIT_COORDS
           ,----
         1 | QUBIT_COORDS(1,2) !0
           :                   ^^
           `----
    "#]],
    );
}

// Single-qubit rotations

#[test]
fn single_qubit_rotations_reject_pauli_target() {
    check(
        "R_X(0.25) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_X
           ,----
         1 | R_X(0.25) X0
           :           ^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_rotations_reject_pauli_product_target() {
    check(
        "R_X(0.25) X1*Y2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_X
           ,----
         1 | R_X(0.25) X1*Y2
           :           ^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_rotations_reject_loss_target() {
    check(
        "R_X(0.25) L3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_X
           ,----
         1 | R_X(0.25) L3
           :           ^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_rotations_reject_sweep_target() {
    check(
        "R_X(0.25) sweep[4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_X
           ,----
         1 | R_X(0.25) sweep[4]
           :           ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_rotations_reject_record_target() {
    check(
        "R_X(0.25) rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_X
           ,----
         1 | R_X(0.25) rec[-5]
           :           ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_rotations_reject_negated_qubit_targets() {
    check(
        "R_X(0.25) !0",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: R_X
           ,----
         1 | R_X(0.25) !0
           :           ^^
           `----
    "#]],
    );
}

// U3

#[test]
fn u3_gates_reject_pauli_target() {
    check(
        "U3(0.1,0.2,0.3) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: U3
           ,----
         1 | U3(0.1,0.2,0.3) X0
           :                 ^^
           `----
    "#]],
    );
}

#[test]
fn u3_gates_reject_pauli_product_target() {
    check(
        "U3(0.1,0.2,0.3) X1*Y2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: U3
           ,----
         1 | U3(0.1,0.2,0.3) X1*Y2
           :                 ^^^^^
           `----
    "#]],
    );
}

#[test]
fn u3_gates_reject_loss_target() {
    check(
        "U3(0.1,0.2,0.3) L3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: U3
           ,----
         1 | U3(0.1,0.2,0.3) L3
           :                 ^^
           `----
    "#]],
    );
}

#[test]
fn u3_gates_reject_sweep_target() {
    check(
        "U3(0.1,0.2,0.3) sweep[4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: U3
           ,----
         1 | U3(0.1,0.2,0.3) sweep[4]
           :                 ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn u3_gates_reject_record_target() {
    check(
        "U3(0.1,0.2,0.3) rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: U3
           ,----
         1 | U3(0.1,0.2,0.3) rec[-5]
           :                 ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn u3_gates_reject_negated_qubit_targets() {
    check(
        "U3(0.1,0.2,0.3) !0",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: U3
           ,----
         1 | U3(0.1,0.2,0.3) !0
           :                 ^^
           `----
    "#]],
    );
}

// Two-qubit rotations

#[test]
fn two_qubit_rotations_reject_pauli_target() {
    check(
        "R_XX(0.25) 0 X1",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_XX
           ,----
         1 | R_XX(0.25) 0 X1
           :              ^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_rotations_reject_pauli_product_target() {
    check(
        "R_XX(0.25) 0 X2*Y3",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_XX
           ,----
         1 | R_XX(0.25) 0 X2*Y3
           :              ^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_rotations_reject_loss_target() {
    check(
        "R_XX(0.25) 0 L4",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_XX
           ,----
         1 | R_XX(0.25) 0 L4
           :              ^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_rotations_reject_sweep_target() {
    check(
        "R_XX(0.25) 0 sweep[5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_XX
           ,----
         1 | R_XX(0.25) 0 sweep[5]
           :              ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_rotations_reject_record_target() {
    check(
        "R_XX(0.25) 0 rec[-6]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_XX
           ,----
         1 | R_XX(0.25) 0 rec[-6]
           :              ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_rotations_reject_negated_qubit_targets() {
    check(
        "R_XX(0.25) !0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: R_XX
           ,----
         1 | R_XX(0.25) !0 1
           :            ^^
           `----
    "#]],
    );
}

// Pauli product rotations

#[test]
fn pauli_product_rotations_reject_qubit_target() {
    check(
        "R_PAULI(0.25) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_PAULI
           ,----
         1 | R_PAULI(0.25) 0
           :               ^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_rotations_reject_loss_target() {
    check(
        "R_PAULI(0.25) L1",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_PAULI
           ,----
         1 | R_PAULI(0.25) L1
           :               ^^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_rotations_reject_sweep_target() {
    check(
        "R_PAULI(0.25) sweep[2]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_PAULI
           ,----
         1 | R_PAULI(0.25) sweep[2]
           :               ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_rotations_reject_record_target() {
    check(
        "R_PAULI(0.25) rec[-3]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_PAULI
           ,----
         1 | R_PAULI(0.25) rec[-3]
           :               ^^^^^^^
           `----
    "#]],
    );
}
