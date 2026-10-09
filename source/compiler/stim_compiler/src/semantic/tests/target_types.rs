// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn single_qubit_gates_reject_non_qubit_targets() {
    check(
        "H X0 X1*Y2 L3 sweep[4] rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: H
           ,----
         1 | H X0 X1*Y2 L3 sweep[4] rec[-5]
           :   ^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: H
           ,----
         1 | H X0 X1*Y2 L3 sweep[4] rec[-5]
           :      ^^^^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: H
           ,----
         1 | H X0 X1*Y2 L3 sweep[4] rec[-5]
           :            ^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: H
           ,----
         1 | H X0 X1*Y2 L3 sweep[4] rec[-5]
           :               ^^^^^^^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: H
           ,----
         1 | H X0 X1*Y2 L3 sweep[4] rec[-5]
           :                        ^^^^^^^
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
fn two_qubit_gates_reject_non_qubit_targets() {
    check(
        "SWAP 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: SWAP
               ,----
             1 | SWAP 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :        ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: SWAP
               ,----
             1 | SWAP 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :             ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: SWAP
               ,----
             1 | SWAP 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                     ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: SWAP
               ,----
             1 | SWAP 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                          ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: SWAP
               ,----
             1 | SWAP 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                                     ^^^^^^^
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
fn classically_controllable_gates_with_records_in_first_position_reject_invalid_target_types() {
    let source = indoc! {"
        M 0 1 2 3 4
        CX rec[-1] X0 rec[-1] X1*Y2 rec[-1] L3 rec[-1] sweep[4] rec[-1] rec[-5]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CX
               ,-[2:12]
             1 | M 0 1 2 3 4
             2 | CX rec[-1] X0 rec[-1] X1*Y2 rec[-1] L3 rec[-1] sweep[4] rec[-1] rec[-5]
               :            ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CX
               ,-[2:23]
             1 | M 0 1 2 3 4
             2 | CX rec[-1] X0 rec[-1] X1*Y2 rec[-1] L3 rec[-1] sweep[4] rec[-1] rec[-5]
               :                       ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CX
               ,-[2:37]
             1 | M 0 1 2 3 4
             2 | CX rec[-1] X0 rec[-1] X1*Y2 rec[-1] L3 rec[-1] sweep[4] rec[-1] rec[-5]
               :                                     ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CX
               ,-[2:48]
             1 | M 0 1 2 3 4
             2 | CX rec[-1] X0 rec[-1] X1*Y2 rec[-1] L3 rec[-1] sweep[4] rec[-1] rec[-5]
               :                                                ^^^^^^^^
               `----

            Qdk.Stim.Semantic.BothTargetsAreMeasurementRecords

              x controlled instruction CX requires a qubit target, but both targets are
              | measurement records
               ,-[2:57]
             1 | M 0 1 2 3 4
             2 | CX rec[-1] X0 rec[-1] X1*Y2 rec[-1] L3 rec[-1] sweep[4] rec[-1] rec[-5]
               :                                                         ^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_second_position_reject_invalid_target_types() {
    let source = indoc! {"
        M 0 1 2 3 4
        XCZ X0 rec[-1] X1*Y2 rec[-1] L3 rec[-1] sweep[4] rec[-1] rec[-5] rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: XCZ
               ,-[2:5]
             1 | M 0 1 2 3 4
             2 | XCZ X0 rec[-1] X1*Y2 rec[-1] L3 rec[-1] sweep[4] rec[-1] rec[-5] rec[-1]
               :     ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: XCZ
               ,-[2:16]
             1 | M 0 1 2 3 4
             2 | XCZ X0 rec[-1] X1*Y2 rec[-1] L3 rec[-1] sweep[4] rec[-1] rec[-5] rec[-1]
               :                ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: XCZ
               ,-[2:30]
             1 | M 0 1 2 3 4
             2 | XCZ X0 rec[-1] X1*Y2 rec[-1] L3 rec[-1] sweep[4] rec[-1] rec[-5] rec[-1]
               :                              ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: XCZ
               ,-[2:41]
             1 | M 0 1 2 3 4
             2 | XCZ X0 rec[-1] X1*Y2 rec[-1] L3 rec[-1] sweep[4] rec[-1] rec[-5] rec[-1]
               :                                         ^^^^^^^^
               `----

            Qdk.Stim.Semantic.BothTargetsAreMeasurementRecords

              x controlled instruction XCZ requires a qubit target, but both targets are
              | measurement records
               ,-[2:58]
             1 | M 0 1 2 3 4
             2 | XCZ X0 rec[-1] X1*Y2 rec[-1] L3 rec[-1] sweep[4] rec[-1] rec[-5] rec[-1]
               :                                                          ^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_first_position_reject_negated_targets() {
    let source = indoc! {"
        M 0
        CX !rec[-1] 1 rec[-1] !1
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.NegatedTarget

              x target cannot be negated in instruction: CX
               ,-[2:4]
             1 | M 0
             2 | CX !rec[-1] 1 rec[-1] !1
               :    ^^^^^^^^
               `----

            Qdk.Stim.Semantic.NegatedTarget

              x target cannot be negated in instruction: CX
               ,-[2:23]
             1 | M 0
             2 | CX !rec[-1] 1 rec[-1] !1
               :                       ^^
               `----
        "#]],
    );
}

#[test]
fn classically_controllable_gates_with_records_in_second_position_reject_negated_targets() {
    let source = indoc! {"
        M 0
        XCZ 1 !rec[-1] !0 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.NegatedTarget

              x target cannot be negated in instruction: XCZ
               ,-[2:7]
             1 | M 0
             2 | XCZ 1 !rec[-1] !0 rec[-1]
               :       ^^^^^^^^
               `----

            Qdk.Stim.Semantic.NegatedTarget

              x target cannot be negated in instruction: XCZ
               ,-[2:16]
             1 | M 0
             2 | XCZ 1 !rec[-1] !0 rec[-1]
               :                ^^
               `----
        "#]],
    );
}

#[test]
fn gates_allowing_records_in_first_position_reject_records_in_second_position() {
    let source = indoc! {"
      M 0
      CX 1 rec[-1]
    "};
    check(
        source,
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
    let source = indoc! {"
      M 0
      XCZ rec[-1] 1
    "};
    check(
        source,
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

#[test]
fn three_qubit_gates_reject_non_qubit_targets() {
    check(
        "CCX 0 1 X2 0 1 X3*Y4 0 1 L5 0 1 sweep[6] 0 1 rec[-7]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CCX
               ,----
             1 | CCX 0 1 X2 0 1 X3*Y4 0 1 L5 0 1 sweep[6] 0 1 rec[-7]
               :         ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CCX
               ,----
             1 | CCX 0 1 X2 0 1 X3*Y4 0 1 L5 0 1 sweep[6] 0 1 rec[-7]
               :                ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CCX
               ,----
             1 | CCX 0 1 X2 0 1 X3*Y4 0 1 L5 0 1 sweep[6] 0 1 rec[-7]
               :                          ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CCX
               ,----
             1 | CCX 0 1 X2 0 1 X3*Y4 0 1 L5 0 1 sweep[6] 0 1 rec[-7]
               :                                 ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: CCX
               ,----
             1 | CCX 0 1 X2 0 1 X3*Y4 0 1 L5 0 1 sweep[6] 0 1 rec[-7]
               :                                              ^^^^^^^
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

#[test]
fn correlated_errors_reject_non_fault_targets() {
    check(
        "E(0.1) 0 X1*Y2 sweep[3] rec[-4]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: E
               ,----
             1 | E(0.1) 0 X1*Y2 sweep[3] rec[-4]
               :        ^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: E
               ,----
             1 | E(0.1) 0 X1*Y2 sweep[3] rec[-4]
               :          ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: E
               ,----
             1 | E(0.1) 0 X1*Y2 sweep[3] rec[-4]
               :                ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: E
               ,----
             1 | E(0.1) 0 X1*Y2 sweep[3] rec[-4]
               :                         ^^^^^^^
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

#[test]
fn single_qubit_noise_rejects_non_qubit_targets() {
    check(
        "DEPOLARIZE1(0.1) X0 X1*Y2 L3 sweep[4] rec[-5]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: DEPOLARIZE1
               ,----
             1 | DEPOLARIZE1(0.1) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                  ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: DEPOLARIZE1
               ,----
             1 | DEPOLARIZE1(0.1) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                     ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: DEPOLARIZE1
               ,----
             1 | DEPOLARIZE1(0.1) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                           ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: DEPOLARIZE1
               ,----
             1 | DEPOLARIZE1(0.1) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                              ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: DEPOLARIZE1
               ,----
             1 | DEPOLARIZE1(0.1) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                                       ^^^^^^^
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

#[test]
fn two_qubit_noise_rejects_non_qubit_targets() {
    check(
        "DEPOLARIZE2(0.1) 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: DEPOLARIZE2
               ,----
             1 | DEPOLARIZE2(0.1) 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                    ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: DEPOLARIZE2
               ,----
             1 | DEPOLARIZE2(0.1) 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                         ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: DEPOLARIZE2
               ,----
             1 | DEPOLARIZE2(0.1) 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                                 ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: DEPOLARIZE2
               ,----
             1 | DEPOLARIZE2(0.1) 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                                      ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: DEPOLARIZE2
               ,----
             1 | DEPOLARIZE2(0.1) 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                                                 ^^^^^^^
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

#[test]
fn identity_errors_reject_non_qubit_targets() {
    check(
        "I_ERROR X0 X1*Y2 L3 sweep[4] rec[-5]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: I_ERROR
               ,----
             1 | I_ERROR X0 X1*Y2 L3 sweep[4] rec[-5]
               :         ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: I_ERROR
               ,----
             1 | I_ERROR X0 X1*Y2 L3 sweep[4] rec[-5]
               :            ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: I_ERROR
               ,----
             1 | I_ERROR X0 X1*Y2 L3 sweep[4] rec[-5]
               :                  ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: I_ERROR
               ,----
             1 | I_ERROR X0 X1*Y2 L3 sweep[4] rec[-5]
               :                     ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: I_ERROR
               ,----
             1 | I_ERROR X0 X1*Y2 L3 sweep[4] rec[-5]
               :                              ^^^^^^^
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

#[test]
fn measurements_reject_non_qubit_targets() {
    check(
        "M X0 X1*Y2 L3 sweep[4] rec[-5]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: M
               ,----
             1 | M X0 X1*Y2 L3 sweep[4] rec[-5]
               :   ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: M
               ,----
             1 | M X0 X1*Y2 L3 sweep[4] rec[-5]
               :      ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: M
               ,----
             1 | M X0 X1*Y2 L3 sweep[4] rec[-5]
               :            ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: M
               ,----
             1 | M X0 X1*Y2 L3 sweep[4] rec[-5]
               :               ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: M
               ,----
             1 | M X0 X1*Y2 L3 sweep[4] rec[-5]
               :                        ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn resets_reject_non_qubit_targets() {
    check(
        "R X0 X1*Y2 L3 sweep[4] rec[-5]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R
               ,----
             1 | R X0 X1*Y2 L3 sweep[4] rec[-5]
               :   ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R
               ,----
             1 | R X0 X1*Y2 L3 sweep[4] rec[-5]
               :      ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R
               ,----
             1 | R X0 X1*Y2 L3 sweep[4] rec[-5]
               :            ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R
               ,----
             1 | R X0 X1*Y2 L3 sweep[4] rec[-5]
               :               ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R
               ,----
             1 | R X0 X1*Y2 L3 sweep[4] rec[-5]
               :                        ^^^^^^^
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

#[test]
fn pair_measurements_reject_non_qubit_targets() {
    check(
        "MXX 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: MXX
               ,----
             1 | MXX 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :       ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: MXX
               ,----
             1 | MXX 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :            ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: MXX
               ,----
             1 | MXX 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                    ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: MXX
               ,----
             1 | MXX 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                         ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: MXX
               ,----
             1 | MXX 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                                    ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn pauli_product_instructions_reject_non_pauli_targets() {
    check(
        "MPP 0 L1 sweep[2] rec[-3]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: MPP
               ,----
             1 | MPP 0 L1 sweep[2] rec[-3]
               :     ^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: MPP
               ,----
             1 | MPP 0 L1 sweep[2] rec[-3]
               :       ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: MPP
               ,----
             1 | MPP 0 L1 sweep[2] rec[-3]
               :          ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: MPP
               ,----
             1 | MPP 0 L1 sweep[2] rec[-3]
               :                   ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn repeat_rejects_non_qubit_count_targets() {
    let source = indoc! {"
        REPEAT X0 {
          X 0
        }
        REPEAT X0*Y1 {
          X 0
        }
        REPEAT L2 {
          X 0
        }
        REPEAT sweep[3] {
          X 0
        }
        REPEAT rec[-4] {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: REPEAT
           ,-[1:8]
         1 | REPEAT X0 {
           :        ^^
         2 |   X 0
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: REPEAT
           ,-[4:8]
         3 | }
         4 | REPEAT X0*Y1 {
           :        ^^^^^
         5 |   X 0
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: REPEAT
           ,-[7:8]
         6 | }
         7 | REPEAT L2 {
           :        ^^
         8 |   X 0
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: REPEAT
            ,-[10:8]
          9 | }
         10 | REPEAT sweep[3] {
            :        ^^^^^^^^
         11 |   X 0
            `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: REPEAT
            ,-[13:8]
         12 | }
         13 | REPEAT rec[-4] {
            :        ^^^^^^^
         14 |   X 0
            `----
    "#]],
    );
}

#[test]
fn repeat_rejects_negated_count() {
    let source = indoc! {"
        REPEAT !3 {
          X 0
        }
    "};
    check(
        source,
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

#[test]
fn select_conditions_reject_non_record_targets() {
    let source = indoc! {"
        SELECT {
          REQUIRE 0 X1 X2*Y3 L4 sweep[5]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[2:11]
             1 | SELECT {
             2 |   REQUIRE 0 X1 X2*Y3 L4 sweep[5]
               :           ^
             3 | }
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[2:13]
             1 | SELECT {
             2 |   REQUIRE 0 X1 X2*Y3 L4 sweep[5]
               :             ^^
             3 | }
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[2:16]
             1 | SELECT {
             2 |   REQUIRE 0 X1 X2*Y3 L4 sweep[5]
               :                ^^^^^
             3 | }
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[2:22]
             1 | SELECT {
             2 |   REQUIRE 0 X1 X2*Y3 L4 sweep[5]
               :                      ^^
             3 | }
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[2:25]
             1 | SELECT {
             2 |   REQUIRE 0 X1 X2*Y3 L4 sweep[5]
               :                         ^^^^^^^^
             3 | }
               `----
        "#]],
    );
}

#[test]
fn notleaked_rejects_negated_record_targets() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED !rec[-1]
        }
    "};
    check(
        source,
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

#[test]
fn peek_loss_rejects_non_qubit_targets() {
    check(
        "PEEK_LOSS X0 X1*Y2 L3 sweep[4] rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: PEEK_LOSS
           ,----
         1 | PEEK_LOSS X0 X1*Y2 L3 sweep[4] rec[-5]
           :           ^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: PEEK_LOSS
           ,----
         1 | PEEK_LOSS X0 X1*Y2 L3 sweep[4] rec[-5]
           :              ^^^^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: PEEK_LOSS
           ,----
         1 | PEEK_LOSS X0 X1*Y2 L3 sweep[4] rec[-5]
           :                    ^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: PEEK_LOSS
           ,----
         1 | PEEK_LOSS X0 X1*Y2 L3 sweep[4] rec[-5]
           :                       ^^^^^^^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: PEEK_LOSS
           ,----
         1 | PEEK_LOSS X0 X1*Y2 L3 sweep[4] rec[-5]
           :                                ^^^^^^^
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

#[test]
fn detector_rejects_non_record_targets() {
    check(
        "DETECTOR 0 X1 X2*Y3 L4 sweep[5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DETECTOR
           ,----
         1 | DETECTOR 0 X1 X2*Y3 L4 sweep[5]
           :          ^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DETECTOR
           ,----
         1 | DETECTOR 0 X1 X2*Y3 L4 sweep[5]
           :            ^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DETECTOR
           ,----
         1 | DETECTOR 0 X1 X2*Y3 L4 sweep[5]
           :               ^^^^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DETECTOR
           ,----
         1 | DETECTOR 0 X1 X2*Y3 L4 sweep[5]
           :                     ^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: DETECTOR
           ,----
         1 | DETECTOR 0 X1 X2*Y3 L4 sweep[5]
           :                        ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn detector_rejects_negated_record_targets() {
    let source = indoc! {"
        M 0
        DETECTOR !rec[-1]
    "};
    check(
        source,
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

#[test]
fn measurement_padding_rejects_non_qubit_targets() {
    check(
        "MPAD X0 X1*Y2 L3 sweep[4] rec[-5]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPAD
           ,----
         1 | MPAD X0 X1*Y2 L3 sweep[4] rec[-5]
           :      ^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPAD
           ,----
         1 | MPAD X0 X1*Y2 L3 sweep[4] rec[-5]
           :         ^^^^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPAD
           ,----
         1 | MPAD X0 X1*Y2 L3 sweep[4] rec[-5]
           :               ^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPAD
           ,----
         1 | MPAD X0 X1*Y2 L3 sweep[4] rec[-5]
           :                  ^^^^^^^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPAD
           ,----
         1 | MPAD X0 X1*Y2 L3 sweep[4] rec[-5]
           :                           ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn measurement_padding_rejects_negated_qubit_targets() {
    check(
        "MPAD !0 !1",
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: MPAD
           ,----
         1 | MPAD !0 !1
           :      ^^
           `----

        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: MPAD
           ,----
         1 | MPAD !0 !1
           :         ^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_rejects_unsupported_target_types() {
    check(
        "OBSERVABLE_INCLUDE(0) 0 X1*Y2 L3 sweep[4]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: OBSERVABLE_INCLUDE
           ,----
         1 | OBSERVABLE_INCLUDE(0) 0 X1*Y2 L3 sweep[4]
           :                       ^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: OBSERVABLE_INCLUDE
           ,----
         1 | OBSERVABLE_INCLUDE(0) 0 X1*Y2 L3 sweep[4]
           :                         ^^^^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: OBSERVABLE_INCLUDE
           ,----
         1 | OBSERVABLE_INCLUDE(0) 0 X1*Y2 L3 sweep[4]
           :                               ^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: OBSERVABLE_INCLUDE
           ,----
         1 | OBSERVABLE_INCLUDE(0) 0 X1*Y2 L3 sweep[4]
           :                                  ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_rejects_negated_targets() {
    let source = indoc! {"
        M 0
        OBSERVABLE_INCLUDE(0) !X1 !rec[-1]
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: OBSERVABLE_INCLUDE
           ,-[2:23]
         1 | M 0
         2 | OBSERVABLE_INCLUDE(0) !X1 !rec[-1]
           :                       ^^^
           `----

        Qdk.Stim.Semantic.NegatedTarget

          x target cannot be negated in instruction: OBSERVABLE_INCLUDE
           ,-[2:27]
         1 | M 0
         2 | OBSERVABLE_INCLUDE(0) !X1 !rec[-1]
           :                           ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn qubit_coordinates_reject_non_qubit_targets() {
    check(
        "QUBIT_COORDS(1,2) X0 X1*Y2 L3 sweep[4] rec[-5]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: QUBIT_COORDS
               ,----
             1 | QUBIT_COORDS(1,2) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                   ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: QUBIT_COORDS
               ,----
             1 | QUBIT_COORDS(1,2) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                      ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: QUBIT_COORDS
               ,----
             1 | QUBIT_COORDS(1,2) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                            ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: QUBIT_COORDS
               ,----
             1 | QUBIT_COORDS(1,2) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                               ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: QUBIT_COORDS
               ,----
             1 | QUBIT_COORDS(1,2) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                                        ^^^^^^^
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

#[test]
fn single_qubit_rotations_reject_non_qubit_targets() {
    check(
        "R_X(0.25) X0 X1*Y2 L3 sweep[4] rec[-5]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R_X
               ,----
             1 | R_X(0.25) X0 X1*Y2 L3 sweep[4] rec[-5]
               :           ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R_X
               ,----
             1 | R_X(0.25) X0 X1*Y2 L3 sweep[4] rec[-5]
               :              ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R_X
               ,----
             1 | R_X(0.25) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                    ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R_X
               ,----
             1 | R_X(0.25) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                       ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R_X
               ,----
             1 | R_X(0.25) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                                ^^^^^^^
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

#[test]
fn u3_gates_reject_non_qubit_targets() {
    check(
        "U3(0.1,0.2,0.3) X0 X1*Y2 L3 sweep[4] rec[-5]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: U3
               ,----
             1 | U3(0.1,0.2,0.3) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                 ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: U3
               ,----
             1 | U3(0.1,0.2,0.3) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                    ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: U3
               ,----
             1 | U3(0.1,0.2,0.3) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                          ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: U3
               ,----
             1 | U3(0.1,0.2,0.3) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                             ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: U3
               ,----
             1 | U3(0.1,0.2,0.3) X0 X1*Y2 L3 sweep[4] rec[-5]
               :                                      ^^^^^^^
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

#[test]
fn two_qubit_rotations_reject_non_qubit_targets() {
    check(
        "R_XX(0.25) 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]",
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R_XX
               ,----
             1 | R_XX(0.25) 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :              ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R_XX
               ,----
             1 | R_XX(0.25) 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                   ^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R_XX
               ,----
             1 | R_XX(0.25) 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                           ^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R_XX
               ,----
             1 | R_XX(0.25) 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                                ^^^^^^^^
               `----

            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: R_XX
               ,----
             1 | R_XX(0.25) 0 X1 0 X2*Y3 0 L4 0 sweep[5] 0 rec[-6]
               :                                           ^^^^^^^
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

#[test]
fn pauli_product_rotations_reject_non_pauli_targets() {
    check(
        "R_PAULI(0.25) 0 L1 sweep[2] rec[-3]",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_PAULI
           ,----
         1 | R_PAULI(0.25) 0 L1 sweep[2] rec[-3]
           :               ^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_PAULI
           ,----
         1 | R_PAULI(0.25) 0 L1 sweep[2] rec[-3]
           :                 ^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_PAULI
           ,----
         1 | R_PAULI(0.25) 0 L1 sweep[2] rec[-3]
           :                    ^^^^^^^^
           `----

        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: R_PAULI
           ,----
         1 | R_PAULI(0.25) 0 L1 sweep[2] rec[-3]
           :                             ^^^^^^^
           `----
    "#]],
    );
}
