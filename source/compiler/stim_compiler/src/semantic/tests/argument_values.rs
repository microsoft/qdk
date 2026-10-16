// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn correlated_errors_reject_probabilities_below_zero() {
    check(
        "CORRELATED_ERROR(-0.1) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for CORRELATED_ERROR must be between 0 and 1; found -0.1
           ,----
         1 | CORRELATED_ERROR(-0.1) X0
           :                  ^^^^
           `----
    "#]],
    );
}

#[test]
fn correlated_errors_reject_probabilities_above_one() {
    check(
        "CORRELATED_ERROR(1.1) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for CORRELATED_ERROR must be between 0 and 1; found 1.1
           ,----
         1 | CORRELATED_ERROR(1.1) X0
           :                  ^^^
           `----
    "#]],
    );
}

#[test]
fn correlated_errors_reject_radians() {
    check(
        "CORRELATED_ERROR(0.1rad) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for CORRELATED_ERROR cannot be specified in radians
           ,----
         1 | CORRELATED_ERROR(0.1rad) X0
           :                  ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_rejects_probabilities_below_zero() {
    check(
        "X_ERROR(-0.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for X_ERROR must be between 0 and 1; found -0.1
           ,----
         1 | X_ERROR(-0.1) 0
           :         ^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_rejects_probabilities_above_one() {
    check(
        "X_ERROR(1.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for X_ERROR must be between 0 and 1; found 1.1
           ,----
         1 | X_ERROR(1.1) 0
           :         ^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_rejects_radians() {
    check(
        "X_ERROR(0.1rad) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for X_ERROR cannot be specified in radians
           ,----
         1 | X_ERROR(0.1rad) 0
           :         ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_probability_lists_report_each_invalid_probability() {
    check(
        "PAULI_CHANNEL_1(-0.1,1.1,0) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for PAULI_CHANNEL_1 must be between 0 and 1; found -0.1
           ,----
         1 | PAULI_CHANNEL_1(-0.1,1.1,0) 0
           :                 ^^^^
           `----

        Qdk.Stim.Semantic.InvalidProbability

          x probability for PAULI_CHANNEL_1 must be between 0 and 1; found 1.1
           ,----
         1 | PAULI_CHANNEL_1(-0.1,1.1,0) 0
           :                      ^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_probability_lists_report_each_argument_in_radians() {
    check(
        "PAULI_CHANNEL_1(0.1rad,0.2rad,0) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for PAULI_CHANNEL_1 cannot be specified in radians
           ,----
         1 | PAULI_CHANNEL_1(0.1rad,0.2rad,0) 0
           :                 ^^^^^^
           `----

        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for PAULI_CHANNEL_1 cannot be specified in radians
           ,----
         1 | PAULI_CHANNEL_1(0.1rad,0.2rad,0) 0
           :                        ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_noise_probability_lists_reject_sums_above_one() {
    check(
        "PAULI_CHANNEL_1(0.6,0.6,0) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbabilitySum

          x probabilities for PAULI_CHANNEL_1 must sum to at most 1.0, but they sum to
          | 1.2
           ,----
         1 | PAULI_CHANNEL_1(0.6,0.6,0) 0
           :                 ^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_rejects_probabilities_below_zero() {
    check(
        "DEPOLARIZE2(-0.1) 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for DEPOLARIZE2 must be between 0 and 1; found -0.1
           ,----
         1 | DEPOLARIZE2(-0.1) 0 1
           :             ^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_rejects_probabilities_above_one() {
    check(
        "DEPOLARIZE2(1.1) 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for DEPOLARIZE2 must be between 0 and 1; found 1.1
           ,----
         1 | DEPOLARIZE2(1.1) 0 1
           :             ^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_rejects_radians() {
    check(
        "DEPOLARIZE2(0.1rad) 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for DEPOLARIZE2 cannot be specified in radians
           ,----
         1 | DEPOLARIZE2(0.1rad) 0 1
           :             ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_noise_probability_lists_report_each_invalid_probability() {
    check(
        "PAULI_CHANNEL_2(-0.1,1.1,0,0,0,0,0,0,0,0,0,0,0,0,0) 0 1",
        &expect![[r#"
            Qdk.Stim.Semantic.InvalidProbability

              x probability for PAULI_CHANNEL_2 must be between 0 and 1; found -0.1
               ,----
             1 | PAULI_CHANNEL_2(-0.1,1.1,0,0,0,0,0,0,0,0,0,0,0,0,0) 0 1
               :                 ^^^^
               `----

            Qdk.Stim.Semantic.InvalidProbability

              x probability for PAULI_CHANNEL_2 must be between 0 and 1; found 1.1
               ,----
             1 | PAULI_CHANNEL_2(-0.1,1.1,0,0,0,0,0,0,0,0,0,0,0,0,0) 0 1
               :                      ^^^
               `----
        "#]],
    );
}

#[test]
fn two_qubit_noise_probability_lists_report_each_argument_in_radians() {
    check(
        "PAULI_CHANNEL_2(0.1rad,0.2rad,0,0,0,0,0,0,0,0,0,0,0,0,0) 0 1",
        &expect![[r#"
            Qdk.Stim.Semantic.UnexpectedRadians

              x argument for PAULI_CHANNEL_2 cannot be specified in radians
               ,----
             1 | PAULI_CHANNEL_2(0.1rad,0.2rad,0,0,0,0,0,0,0,0,0,0,0,0,0) 0 1
               :                 ^^^^^^
               `----

            Qdk.Stim.Semantic.UnexpectedRadians

              x argument for PAULI_CHANNEL_2 cannot be specified in radians
               ,----
             1 | PAULI_CHANNEL_2(0.1rad,0.2rad,0,0,0,0,0,0,0,0,0,0,0,0,0) 0 1
               :                        ^^^^^^
               `----
        "#]],
    );
}

#[test]
fn two_qubit_noise_probability_lists_reject_sums_above_one() {
    check(
        "PAULI_CHANNEL_2(0.6,0.6,0,0,0,0,0,0,0,0,0,0,0,0,0) 0 1",
        &expect![[r#"
            Qdk.Stim.Semantic.InvalidProbabilitySum

              x probabilities for PAULI_CHANNEL_2 must sum to at most 1.0, but they sum to
              | 1.2
               ,----
             1 | PAULI_CHANNEL_2(0.6,0.6,0,0,0,0,0,0,0,0,0,0,0,0,0) 0 1
               :                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn identity_errors_report_each_invalid_probability() {
    check(
        "I_ERROR(-0.1,1.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for I_ERROR must be between 0 and 1; found -0.1
           ,----
         1 | I_ERROR(-0.1,1.1) 0
           :         ^^^^
           `----

        Qdk.Stim.Semantic.InvalidProbability

          x probability for I_ERROR must be between 0 and 1; found 1.1
           ,----
         1 | I_ERROR(-0.1,1.1) 0
           :              ^^^
           `----
    "#]],
    );
}

#[test]
fn identity_errors_report_each_argument_in_radians() {
    check(
        "I_ERROR(0.1rad,0.2rad) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for I_ERROR cannot be specified in radians
           ,----
         1 | I_ERROR(0.1rad,0.2rad) 0
           :         ^^^^^^
           `----

        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for I_ERROR cannot be specified in radians
           ,----
         1 | I_ERROR(0.1rad,0.2rad) 0
           :                ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn identity_errors_reject_probability_sums_above_one() {
    check(
        "I_ERROR(0.6,0.6) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbabilitySum

          x probabilities for I_ERROR must sum to at most 1.0, but they sum to 1.2
           ,----
         1 | I_ERROR(0.6,0.6) 0
           :         ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn measurements_reject_readout_noise_below_zero() {
    check(
        "M(-0.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for M must be between 0 and 1; found -0.1
           ,----
         1 | M(-0.1) 0
           :   ^^^^
           `----
    "#]],
    );
}

#[test]
fn measurements_reject_readout_noise_above_one() {
    check(
        "M(1.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for M must be between 0 and 1; found 1.1
           ,----
         1 | M(1.1) 0
           :   ^^^
           `----
    "#]],
    );
}

#[test]
fn measurements_reject_readout_noise_in_radians() {
    check(
        "M(0.1rad) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for M cannot be specified in radians
           ,----
         1 | M(0.1rad) 0
           :   ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn pair_measurements_reject_readout_noise_below_zero() {
    check(
        "MXX(-0.1) 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for MXX must be between 0 and 1; found -0.1
           ,----
         1 | MXX(-0.1) 0 1
           :     ^^^^
           `----
    "#]],
    );
}

#[test]
fn pair_measurements_reject_readout_noise_above_one() {
    check(
        "MXX(1.1) 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for MXX must be between 0 and 1; found 1.1
           ,----
         1 | MXX(1.1) 0 1
           :     ^^^
           `----
    "#]],
    );
}

#[test]
fn pair_measurements_reject_readout_noise_in_radians() {
    check(
        "MXX(0.1rad) 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for MXX cannot be specified in radians
           ,----
         1 | MXX(0.1rad) 0 1
           :     ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_measurements_reject_readout_noise_below_zero() {
    check(
        "MPP(-0.1) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for MPP must be between 0 and 1; found -0.1
           ,----
         1 | MPP(-0.1) X0
           :     ^^^^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_measurements_reject_readout_noise_above_one() {
    check(
        "MPP(1.1) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for MPP must be between 0 and 1; found 1.1
           ,----
         1 | MPP(1.1) X0
           :     ^^^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_measurements_reject_readout_noise_in_radians() {
    check(
        "MPP(0.1rad) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for MPP cannot be specified in radians
           ,----
         1 | MPP(0.1rad) X0
           :     ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn peek_loss_rejects_readout_noise_below_zero() {
    check(
        "PEEK_LOSS(-0.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for PEEK_LOSS must be between 0 and 1; found -0.1
           ,----
         1 | PEEK_LOSS(-0.1) 0
           :           ^^^^
           `----
    "#]],
    );
}

#[test]
fn peek_loss_rejects_readout_noise_above_one() {
    check(
        "PEEK_LOSS(1.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for PEEK_LOSS must be between 0 and 1; found 1.1
           ,----
         1 | PEEK_LOSS(1.1) 0
           :           ^^^
           `----
    "#]],
    );
}

#[test]
fn peek_loss_rejects_readout_noise_in_radians() {
    check(
        "PEEK_LOSS(0.1rad) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for PEEK_LOSS cannot be specified in radians
           ,----
         1 | PEEK_LOSS(0.1rad) 0
           :           ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn detector_rejects_coordinates_in_radians() {
    check(
        "DETECTOR(0.1rad)",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for DETECTOR cannot be specified in radians
           ,----
         1 | DETECTOR(0.1rad)
           :          ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn measurement_padding_rejects_readout_noise_below_zero() {
    check(
        "MPAD(-0.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for MPAD must be between 0 and 1; found -0.1
           ,----
         1 | MPAD(-0.1) 0
           :      ^^^^
           `----
    "#]],
    );
}

#[test]
fn measurement_padding_rejects_readout_noise_above_one() {
    check(
        "MPAD(1.1) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidProbability

          x probability for MPAD must be between 0 and 1; found 1.1
           ,----
         1 | MPAD(1.1) 0
           :      ^^^
           `----
    "#]],
    );
}

#[test]
fn measurement_padding_rejects_readout_noise_in_radians() {
    check(
        "MPAD(0.1rad) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for MPAD cannot be specified in radians
           ,----
         1 | MPAD(0.1rad) 0
           :      ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_index_rejects_radians() {
    check(
        "OBSERVABLE_INCLUDE(0rad) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for OBSERVABLE_INCLUDE cannot be specified in radians
           ,----
         1 | OBSERVABLE_INCLUDE(0rad) X0
           :                    ^^^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_index_rejects_negative_values() {
    check(
        "OBSERVABLE_INCLUDE(-1) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidLogicalObservableIndex

          x logical observable index must be a non-negative 32-bit integer
           ,----
         1 | OBSERVABLE_INCLUDE(-1) X0
           :                    ^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_index_rejects_fractional_values() {
    check(
        "OBSERVABLE_INCLUDE(0.5) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidLogicalObservableIndex

          x logical observable index must be a non-negative 32-bit integer
           ,----
         1 | OBSERVABLE_INCLUDE(0.5) X0
           :                    ^^^
           `----
    "#]],
    );
}

#[test]
fn observable_include_index_rejects_values_above_u32_max() {
    check(
        "OBSERVABLE_INCLUDE(4294967296) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidLogicalObservableIndex

          x logical observable index must be a non-negative 32-bit integer
           ,----
         1 | OBSERVABLE_INCLUDE(4294967296) X0
           :                    ^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn qubit_coordinates_reject_radians() {
    check(
        "QUBIT_COORDS(0.1rad) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for QUBIT_COORDS cannot be specified in radians
           ,----
         1 | QUBIT_COORDS(0.1rad) 0
           :              ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn shift_coordinates_reject_radians() {
    check(
        "SHIFT_COORDS(0.1rad)",
        &expect![[r#"
        Qdk.Stim.Semantic.UnexpectedRadians

          x argument for SHIFT_COORDS cannot be specified in radians
           ,----
         1 | SHIFT_COORDS(0.1rad)
           :              ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn single_qubit_rotations_require_finite_angles() {
    check(
        "R_X(1e308) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidAngle

          x angle for R_X must be finite and representable in radians
           ,----
         1 | R_X(1e308) 0
           :     ^^^^^
           `----
    "#]],
    );
}

#[test]
fn u3_gates_report_each_non_finite_angle() {
    check(
        "U3(1e308,0.25,-1e308) 0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidAngle

          x angle for U3 must be finite and representable in radians
           ,----
         1 | U3(1e308,0.25,-1e308) 0
           :    ^^^^^
           `----

        Qdk.Stim.Semantic.InvalidAngle

          x angle for U3 must be finite and representable in radians
           ,----
         1 | U3(1e308,0.25,-1e308) 0
           :               ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn two_qubit_rotations_require_finite_angles() {
    check(
        "R_XX(1e308) 0 1",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidAngle

          x angle for R_XX must be finite and representable in radians
           ,----
         1 | R_XX(1e308) 0 1
           :      ^^^^^
           `----
    "#]],
    );
}

#[test]
fn pauli_product_rotations_require_finite_angles() {
    check(
        "R_PAULI(1e308) X0",
        &expect![[r#"
        Qdk.Stim.Semantic.InvalidAngle

          x angle for R_PAULI must be finite and representable in radians
           ,----
         1 | R_PAULI(1e308) X0
           :         ^^^^^
           `----
    "#]],
    );
}
