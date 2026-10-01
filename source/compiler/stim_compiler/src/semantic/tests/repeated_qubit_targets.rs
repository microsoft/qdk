// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn two_qubit_gates_reject_repeated_qubit_targets() {
    check(
        "ISWAP 0 0",
        &expect![[r#"
        Qdk.Stim.Semantic.RepeatedQubit

          x qubit 0 is repeated in instruction: ISWAP
           ,----
         1 | ISWAP 0 0
           :         ^
           `----
    "#]],
    );
}

#[test]
fn classically_controllable_gates_reject_repeated_qubit_targets() {
    check(
        "CX 0 0",
        &expect![[r#"
        Qdk.Stim.Semantic.RepeatedQubit

          x qubit 0 is repeated in instruction: CX
           ,----
         1 | CX 0 0
           :      ^
           `----
    "#]],
    );
}

#[test]
fn three_qubit_gates_reject_repeated_first_and_second_targets() {
    check(
        "CCX 0 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.RepeatedQubit

          x qubit 0 is repeated in instruction: CCX
           ,----
         1 | CCX 0 0 1
           :       ^
           `----
    "#]],
    );
}

#[test]
fn three_qubit_gates_reject_repeated_first_and_third_targets() {
    check(
        "CCX 0 1 0",
        &expect![[r#"
        Qdk.Stim.Semantic.RepeatedQubit

          x qubit 0 is repeated in instruction: CCX
           ,----
         1 | CCX 0 1 0
           :         ^
           `----
    "#]],
    );
}

#[test]
fn three_qubit_gates_reject_repeated_second_and_third_targets() {
    check(
        "CCX 0 1 1",
        &expect![[r#"
        Qdk.Stim.Semantic.RepeatedQubit

          x qubit 1 is repeated in instruction: CCX
           ,----
         1 | CCX 0 1 1
           :         ^
           `----
    "#]],
    );
}

#[test]
fn three_qubit_gates_reject_three_repeated_targets() {
    check(
        "CCX 0 0 0",
        &expect![[r#"
        Qdk.Stim.Semantic.RepeatedQubit

          x qubit 0 is repeated in instruction: CCX
           ,----
         1 | CCX 0 0 0
           :       ^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_rejects_repeated_qubit_targets() {
    check(
        "DEPOLARIZE2(0.1) 0 0",
        &expect![[r#"
        Qdk.Stim.Semantic.RepeatedQubit

          x qubit 0 is repeated in instruction: DEPOLARIZE2
           ,----
         1 | DEPOLARIZE2(0.1) 0 0
           :                    ^
           `----
    "#]],
    );
}

#[test]
fn identity_errors_reject_repeated_qubit_targets() {
    check(
        "II_ERROR 0 0",
        &expect![[r#"
        Qdk.Stim.Semantic.RepeatedQubit

          x qubit 0 is repeated in instruction: II_ERROR
           ,----
         1 | II_ERROR 0 0
           :            ^
           `----
    "#]],
    );
}

#[test]
fn pair_measurements_reject_repeated_qubit_targets() {
    check(
        "MXX 0 0",
        &expect![[r#"
        Qdk.Stim.Semantic.RepeatedQubit

          x qubit 0 is repeated in instruction: MXX
           ,----
         1 | MXX 0 0
           :       ^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_rotations_reject_repeated_qubit_targets() {
    check(
        "R_XX(0.25) 0 0",
        &expect![[r#"
        Qdk.Stim.Semantic.RepeatedQubit

          x qubit 0 is repeated in instruction: R_XX
           ,----
         1 | R_XX(0.25) 0 0
           :              ^
           `----
    "#]],
    );
}
