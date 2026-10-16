// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn single_qubit_gates_reject_arguments() {
    check(
        "H(0.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedArgument

          x unsupported argument in instruction: H
           ,----
         1 | H(0.1) 0
           :   ^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_gates_reject_arguments() {
    check(
        "SWAP(0.1) 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedArgument

          x unsupported argument in instruction: SWAP
           ,----
         1 | SWAP(0.1) 0 1
           :      ^^^
           `----
    "#]],
    );
}

#[test]
fn classically_controllable_gates_reject_arguments() {
    check(
        indoc! {"
        M 0
        CX(0.1) rec[-1] 1
    "},
        &expect![[r#"
            Qdk.Stim.Semantic.UnsupportedArgument

              x unsupported argument in instruction: CX
               ,-[2:4]
             1 | M 0
             2 | CX(0.1) rec[-1] 1
               :    ^^^
               `----
        "#]],
    );
}

#[test]
fn three_qubit_gates_reject_arguments() {
    check(
        "CCX(0.1) 0 1 2",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedArgument

          x unsupported argument in instruction: CCX
           ,----
         1 | CCX(0.1) 0 1 2
           :     ^^^
           `----
    "#]],
    );
}

#[test]
fn correlated_errors_require_an_argument() {
    check(
        "CORRELATED_ERROR X0",
        &expect![[r#"
        Qdk.Stim.Semantic.MissingArg

          x missing argument in instruction: CORRELATED_ERROR
           ,----
         1 | CORRELATED_ERROR X0
           : ^^^^^^^^^^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn correlated_errors_reject_multiple_arguments() {
    check(
        "CORRELATED_ERROR(0.1,0.2) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction CORRELATED_ERROR; expected 1, found 2
           ,----
         1 | CORRELATED_ERROR(0.1,0.2) X0
           :                      ^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_requires_an_argument() {
    check(
        "DEPOLARIZE1 0",
        &expect![[r#"
        Qdk.Stim.Semantic.MissingArg

          x missing argument in instruction: DEPOLARIZE1
           ,----
         1 | DEPOLARIZE1 0
           : ^^^^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_rejects_multiple_arguments() {
    check(
        "DEPOLARIZE1(0.1,0.2) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction DEPOLARIZE1; expected 1, found 2
           ,----
         1 | DEPOLARIZE1(0.1,0.2) 0
           :                 ^^^
           `----
    "#]],
    );
}

#[test]
fn heralded_single_qubit_noise_rejects_too_few_arguments() {
    check(
        "HERALDED_PAULI_CHANNEL_1(0.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooFewArgs

          x too few arguments for instruction HERALDED_PAULI_CHANNEL_1; expected 4,
          | found 1
           ,----
         1 | HERALDED_PAULI_CHANNEL_1(0.1) 0
           :                          ^^^
           `----
    "#]],
    );
}

#[test]
fn heralded_single_qubit_noise_rejects_too_many_arguments() {
    check(
        "HERALDED_PAULI_CHANNEL_1(0,0,0,0,0) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction HERALDED_PAULI_CHANNEL_1; expected 4,
          | found 5
           ,----
         1 | HERALDED_PAULI_CHANNEL_1(0,0,0,0,0) 0
           :                                  ^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_probability_lists_reject_too_few_arguments() {
    check(
        "PAULI_CHANNEL_1(0.1,0.2) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooFewArgs

          x too few arguments for instruction PAULI_CHANNEL_1; expected 3, found 2
           ,----
         1 | PAULI_CHANNEL_1(0.1,0.2) 0
           :                 ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_probability_lists_reject_too_many_arguments() {
    check(
        "PAULI_CHANNEL_1(0,0,0,0) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction PAULI_CHANNEL_1; expected 3, found 4
           ,----
         1 | PAULI_CHANNEL_1(0,0,0,0) 0
           :                       ^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_requires_an_argument() {
    check(
        "DEPOLARIZE2 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.MissingArg

          x missing argument in instruction: DEPOLARIZE2
           ,----
         1 | DEPOLARIZE2 0 1
           : ^^^^^^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_rejects_multiple_arguments() {
    check(
        "DEPOLARIZE2(0.1,0.2) 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction DEPOLARIZE2; expected 1, found 2
           ,----
         1 | DEPOLARIZE2(0.1,0.2) 0 1
           :                 ^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_probability_lists_reject_too_few_arguments() {
    check(
        "PAULI_CHANNEL_2(0.1) 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.TooFewArgs

          x too few arguments for instruction PAULI_CHANNEL_2; expected 15, found 1
           ,----
         1 | PAULI_CHANNEL_2(0.1) 0 1
           :                 ^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_probability_lists_reject_too_many_arguments() {
    check(
        "PAULI_CHANNEL_2(0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0) 0 1",
        &expect![[r#"
            Qdk.Stim.Semantic.TooManyArgs

              x too many arguments for instruction PAULI_CHANNEL_2; expected 15, found 16
               ,----
             1 | PAULI_CHANNEL_2(0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0) 0 1
               :                                               ^
               `----
        "#]],
    );
}

#[test]
fn measurements_reject_multiple_arguments() {
    check(
        "M(0.1,0.2) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction M; expected 1, found 2
           ,----
         1 | M(0.1,0.2) 0
           :       ^^^
           `----
    "#]],
    );
}

#[test]
fn resets_reject_arguments() {
    check(
        "R(0.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedArgument

          x unsupported argument in instruction: R
           ,----
         1 | R(0.1) 0
           :   ^^^
           `----
    "#]],
    );
}

#[test]
fn pair_measurements_reject_multiple_arguments() {
    check(
        "MXX(0.1,0.2) 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction MXX; expected 1, found 2
           ,----
         1 | MXX(0.1,0.2) 0 1
           :         ^^^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_measurements_reject_multiple_arguments() {
    check(
        "MPP(0.1,0.2) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction MPP; expected 1, found 2
           ,----
         1 | MPP(0.1,0.2) X0
           :         ^^^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_gates_reject_arguments() {
    check(
        "SPP(0.1) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedArgument

          x unsupported argument in instruction: SPP
           ,----
         1 | SPP(0.1) X0
           :     ^^^
           `----
    "#]],
    );
}

#[test]
fn repeat_rejects_arguments() {
    let source = indoc! {"
        REPEAT(0.1) 2 {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedArgument

          x unsupported argument in instruction: REPEAT
           ,-[1:8]
         1 | REPEAT(0.1) 2 {
           :        ^^^
         2 |   X 0
           `----
    "#]],
    );
}

#[test]
fn select_blocks_reject_arguments() {
    let source = indoc! {"
        SELECT(0.1) {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedArgument

          x unsupported argument in instruction: SELECT
           ,-[1:8]
         1 | SELECT(0.1) {
           :        ^^^
         2 |   X 0
           `----
    "#]],
    );
}

#[test]
fn select_conditions_reject_arguments() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE(0.1) rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedArgument

          x unsupported argument in instruction: REQUIRE
           ,-[3:11]
         2 |   M 0
         3 |   REQUIRE(0.1) rec[-1]
           :           ^^^
         4 | }
           `----
    "#]],
    );
}

#[test]
fn notleaked_rejects_arguments() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED(0.1) rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedArgument

          x unsupported argument in instruction: NOTLEAKED
           ,-[3:13]
         2 |   M 0
         3 |   NOTLEAKED(0.1) rec[-1]
           :             ^^^
         4 | }
           `----
    "#]],
    );
}

#[test]
fn peek_loss_rejects_multiple_arguments() {
    check(
        "PEEK_LOSS(0.1,0.2) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction PEEK_LOSS; expected 1, found 2
           ,----
         1 | PEEK_LOSS(0.1,0.2) 0
           :               ^^^
           `----
    "#]],
    );
}

#[test]
fn detector_rejects_more_than_sixteen_arguments() {
    check(
        "DETECTOR(0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16)",
        &expect![[r#"
            Qdk.Stim.Semantic.TooManyArgs

              x too many arguments for instruction DETECTOR; expected 16, found 17
               ,----
             1 | DETECTOR(0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16)
               :                                                ^^
               `----
        "#]],
    );
}

#[test]
fn measurement_padding_rejects_multiple_arguments() {
    check(
        "MPAD(0.1,0.2) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction MPAD; expected 1, found 2
           ,----
         1 | MPAD(0.1,0.2) 0
           :          ^^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_requires_an_argument() {
    check(
        "OBSERVABLE_INCLUDE X0",
        &expect![[r#"
        Qdk.Stim.Semantic.MissingArg

          x missing argument in instruction: OBSERVABLE_INCLUDE
           ,----
         1 | OBSERVABLE_INCLUDE X0
           : ^^^^^^^^^^^^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_rejects_multiple_arguments() {
    check(
        "OBSERVABLE_INCLUDE(0,1) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction OBSERVABLE_INCLUDE; expected 1, found 2
           ,----
         1 | OBSERVABLE_INCLUDE(0,1) X0
           :                      ^
           `----
    "#]],
    );
}

#[test]
fn qubit_coordinates_reject_more_than_sixteen_arguments() {
    check(
        "QUBIT_COORDS(0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16) 0",
        &expect![[r#"
            Qdk.Stim.Semantic.TooManyArgs

              x too many arguments for instruction QUBIT_COORDS; expected 16, found 17
               ,----
             1 | QUBIT_COORDS(0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16) 0
               :                                                    ^^
               `----
        "#]],
    );
}

#[test]
fn shift_coordinates_requires_an_argument() {
    check(
        "SHIFT_COORDS",
        &expect![[r#"
        Qdk.Stim.Semantic.MissingArg

          x missing argument in instruction: SHIFT_COORDS
           ,----
         1 | SHIFT_COORDS
           : ^^^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn shift_coordinates_rejects_more_than_sixteen_arguments() {
    check(
        "SHIFT_COORDS(0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16)",
        &expect![[r#"
            Qdk.Stim.Semantic.TooManyArgs

              x too many arguments for instruction SHIFT_COORDS; expected 16, found 17
               ,----
             1 | SHIFT_COORDS(0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16)
               :                                                    ^^
               `----
        "#]],
    );
}

#[test]
fn tick_rejects_arguments() {
    check(
        "TICK(0.1)",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedArgument

          x unsupported argument in instruction: TICK
           ,----
         1 | TICK(0.1)
           :      ^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_rotations_require_an_argument() {
    check(
        "R_X 0",
        &expect![[r#"
        Qdk.Stim.Semantic.MissingArg

          x missing argument in instruction: R_X
           ,----
         1 | R_X 0
           : ^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_rotations_reject_multiple_arguments() {
    check(
        "R_X(0.1,0.2) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction R_X; expected 1, found 2
           ,----
         1 | R_X(0.1,0.2) 0
           :         ^^^
           `----
    "#]],
    );
}

#[test]
fn u3_gates_require_arguments() {
    check(
        "U3 0",
        &expect![[r#"
        Qdk.Stim.Semantic.MissingArg

          x missing argument in instruction: U3
           ,----
         1 | U3 0
           : ^^^^
           `----
    "#]],
    );
}

#[test]
fn u3_gates_reject_too_few_arguments() {
    check(
        "U3(0.1,0.2) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooFewArgs

          x too few arguments for instruction U3; expected 3, found 2
           ,----
         1 | U3(0.1,0.2) 0
           :    ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn u3_gates_reject_too_many_arguments() {
    check(
        "U3(0.1,0.2,0.3,0.4) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction U3; expected 3, found 4
           ,----
         1 | U3(0.1,0.2,0.3,0.4) 0
           :                ^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_rotations_require_an_argument() {
    check(
        "R_XX 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.MissingArg

          x missing argument in instruction: R_XX
           ,----
         1 | R_XX 0 1
           : ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_rotations_reject_multiple_arguments() {
    check(
        "R_XX(0.1,0.2) 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction R_XX; expected 1, found 2
           ,----
         1 | R_XX(0.1,0.2) 0 1
           :          ^^^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_rotations_require_an_argument() {
    check(
        "R_PAULI X0",
        &expect![[r#"
        Qdk.Stim.Semantic.MissingArg

          x missing argument in instruction: R_PAULI
           ,----
         1 | R_PAULI X0
           : ^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_rotations_reject_multiple_arguments() {
    check(
        "R_PAULI(0.1,0.2) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.TooManyArgs

          x too many arguments for instruction R_PAULI; expected 1, found 2
           ,----
         1 | R_PAULI(0.1,0.2) X0
           :             ^^^
           `----
    "#]],
    );
}
