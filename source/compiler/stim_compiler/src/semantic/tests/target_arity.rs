// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

// Two-qubit gates

#[test]
fn ii_with_odd_target_count_yields_error() {
    check(
        "II 0",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction II requires an even number of targets
               ,----
             1 | II 0
               :    ^
               `----
        "#]],
    );

    check(
        "II 0 1 2",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction II requires an even number of targets
               ,----
             1 | II 0 1 2
               :    ^^^^^
               `----
        "#]],
    );
}

// Classically controllable gates

#[test]
fn cx_with_odd_target_count_yields_error() {
    check(
        "CX 0",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction CX requires an even number of targets
               ,----
             1 | CX 0
               :    ^
               `----
        "#]],
    );

    check(
        "CX 0 1 2",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction CX requires an even number of targets
               ,----
             1 | CX 0 1 2
               :    ^^^^^
               `----
        "#]],
    );
}

#[test]
fn cx_with_record_and_odd_target_count_yields_error() {
    let source = indoc! {"
    M 0
    CX rec[-1]
  "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction CX requires an even number of targets
               ,-[2:4]
             1 | M 0
             2 | CX rec[-1]
               :    ^^^^^^^
               `----
        "#]],
    );

    let source = indoc! {"
        M 0
        CX rec[-1] 1 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction CX requires an even number of targets
               ,-[2:4]
             1 | M 0
             2 | CX rec[-1] 1 rec[-1]
               :    ^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

// Three-qubit gates

#[test]
fn ccx_with_one_target_yields_error() {
    check(
        "CCX 0",
        &expect![[r#"
            Qdk.Stim.Semantic.TargetCountNotMultipleOfThree

              x instruction CCX requires a multiple of three targets
               ,----
             1 | CCX 0
               :     ^
               `----
        "#]],
    );
}

#[test]
fn ccx_with_two_targets_yields_error() {
    check(
        "CCX 0 1",
        &expect![[r#"
            Qdk.Stim.Semantic.TargetCountNotMultipleOfThree

              x instruction CCX requires a multiple of three targets
               ,----
             1 | CCX 0 1
               :     ^^^
               `----
        "#]],
    );
}

#[test]
fn ccx_with_four_targets_yields_error() {
    check(
        "CCX 0 1 2 3",
        &expect![[r#"
            Qdk.Stim.Semantic.TargetCountNotMultipleOfThree

              x instruction CCX requires a multiple of three targets
               ,----
             1 | CCX 0 1 2 3
               :     ^^^^^^^
               `----
        "#]],
    );
}

// Noise channels

#[test]
fn depolarize2_with_odd_target_count_yields_error() {
    check(
        "DEPOLARIZE2(0.01) 0",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction DEPOLARIZE2 requires an even number of targets
               ,----
             1 | DEPOLARIZE2(0.01) 0
               :                   ^
               `----
        "#]],
    );

    check(
        "DEPOLARIZE2(0.01) 0 1 2",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction DEPOLARIZE2 requires an even number of targets
               ,----
             1 | DEPOLARIZE2(0.01) 0 1 2
               :                   ^^^^^
               `----
        "#]],
    );
}

#[test]
fn ii_error_with_odd_target_count_yields_error() {
    check(
        "II_ERROR 0",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction II_ERROR requires an even number of targets
               ,----
             1 | II_ERROR 0
               :          ^
               `----
        "#]],
    );

    check(
        "II_ERROR 0 1 2",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction II_ERROR requires an even number of targets
               ,----
             1 | II_ERROR 0 1 2
               :          ^^^^^
               `----
        "#]],
    );
}

#[test]
fn pauli_channel_2_with_odd_target_count_yields_error() {
    check(
        "PAULI_CHANNEL_2(0,0,0, 0,0.1,0,0, 0,0,0,0.2, 0,0,0,0) 0",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction PAULI_CHANNEL_2 requires an even number of targets
               ,----
             1 | PAULI_CHANNEL_2(0,0,0, 0,0.1,0,0, 0,0,0,0.2, 0,0,0,0) 0
               :                                                       ^
               `----
        "#]],
    );

    check(
        "PAULI_CHANNEL_2(0,0,0, 0,0.1,0,0, 0,0,0,0.2, 0,0,0,0) 0 1 2",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction PAULI_CHANNEL_2 requires an even number of targets
               ,----
             1 | PAULI_CHANNEL_2(0,0,0, 0,0.1,0,0, 0,0,0,0.2, 0,0,0,0) 0 1 2
               :                                                       ^^^^^
               `----
        "#]],
    );
}

// Pair measurements

#[test]
fn mzz_with_odd_target_count_yields_error() {
    check(
        "MZZ 0",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction MZZ requires an even number of targets
               ,----
             1 | MZZ 0
               :     ^
               `----
        "#]],
    );

    check(
        "MZZ 0 1 2",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction MZZ requires an even number of targets
               ,----
             1 | MZZ 0 1 2
               :     ^^^^^
               `----
        "#]],
    );
}

// Annotations

#[test]
fn shift_coords_with_targets_yields_error() {
    check(
        "SHIFT_COORDS(1,2) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTargets

          x unsupported targets in instruction: SHIFT_COORDS
           ,----
         1 | SHIFT_COORDS(1,2) 0
           :                   ^
           `----
    "#]],
    );
}

#[test]
fn tick_with_targets_yields_error() {
    check(
        "TICK 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTargets

          x unsupported targets in instruction: TICK
           ,----
         1 | TICK 0
           :      ^
           `----
    "#]],
    );
}

// Two-qubit rotations

#[test]
fn r_xx_with_odd_target_count_yields_error() {
    check(
        "R_XX(0.25) 0",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction R_XX requires an even number of targets
               ,----
             1 | R_XX(0.25) 0
               :            ^
               `----
        "#]],
    );

    check(
        "R_XX(0.25) 0 1 2",
        &expect![[r#"
            Qdk.Stim.Semantic.OddTargetCount

              x instruction R_XX requires an even number of targets
               ,----
             1 | R_XX(0.25) 0 1 2
               :            ^^^^^
               `----
        "#]],
    );
}

// Block instructions

#[test]
fn repeat_with_multiple_targets_yields_error() {
    let source = indoc! {"
        REPEAT 3 2 1 {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: REPEAT
               ,-[1:10]
             1 | REPEAT 3 2 1 {
               :          ^
             2 |   X 0
               `----
        "#]],
    );
}

#[test]
fn repeat_without_targets_yields_error() {
    let source = indoc! {"
        REPEAT {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.MissingTarget

              x missing target in instruction: REPEAT
               ,-[1:1]
             1 | REPEAT {
               : ^^^^^^
             2 |   X 0
               `----
        "#]],
    );
}

#[test]
fn select_with_targets_yields_error() {
    let source = indoc! {"
        SELECT 0 1 {
          M 0
          REQUIRE rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedTarget

              x unsupported target in instruction: SELECT
               ,-[1:8]
             1 | SELECT 0 1 {
               :        ^
             2 |   M 0
               `----
        "#]],
    );
}

#[test]
fn require_without_targets_yields_error() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.MissingTarget

              x missing target in instruction: REQUIRE
               ,-[3:3]
             2 |   M 0
             3 |   REQUIRE
               :   ^^^^^^^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn notleaked_without_targets_yields_error() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Semantic.MissingTarget

              x missing target in instruction: NOTLEAKED
               ,-[3:3]
             2 |   M 0
             3 |   NOTLEAKED
               :   ^^^^^^^^^
             4 | }
               `----
        "#]],
    );
}
