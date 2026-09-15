// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn mpp_single_x_yields_expected_qir() {
    // same as MX 0
    check(
        "MPP X0",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_single_y_yields_expected_qir() {
    // same as MY 0
    check(
        "MPP Y0",
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_single_z_yields_expected_qir() {
    // same as MZ 0
    check(
        "MPP Z0",
        &expect![[r#"
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__m__body(ptr, ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_negated_single_pauli_yields_expected_qir() {
    // same as MZ !5
    check(
        "MPP !Z5",
        &expect![[r#"
            body:
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_two_factor_product_yields_expected_qir() {
    check(
        "MPP X1*Y2",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_three_factor_product_yields_expected_qir() {
    // order of qubit indices in the output is not guaranteed to match the order of qubit indices in the input
    check(
        "MPP Z3*Z4*Z5",
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)

            required_num_qubits: 3
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_product_of_all_three_bases_yields_expected_qir() {
    check(
        "MPP X0*Y1*Z2",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 3
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_negated_product_yields_expected_qir() {
    // order of qubit indices in the output is not guaranteed to match the order of qubit indices in the input
    check(
        "MPP !Z3*Z4*Z5",
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 3
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_negation_on_later_factor_negates_whole_product() {
    check(
        "MPP X0*!Y1",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_double_negation_cancels() {
    check(
        "MPP !X0*!Y1",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_repeated_qubit_folds_to_single_pauli() {
    // X0*X0*X0 = X0
    check(
        "MPP X0*X0*X0",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_repeated_qubit_folding_to_minus_one_negates_result() {
    // X0*Y0 = iZ0 and X1*Y1 = iZ1, so the product is i^2 Z0*Z1 = -Z0*Z1.
    check(
        "MPP X0*Y0*X1*Y1",
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_explicit_negation_cancels_folded_minus_one() {
    // The '!' contributes -1 and the folding contributes -1, so the result is not negated.
    check(
        "MPP !X0*Y0*X1*Y1",
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_non_adjacent_repeated_qubits_are_folded_together() {
    // X0*Z0 = -iY0 and Z1*X1 = iY1, so the product is +Y0*Y1.
    check(
        "MPP X0*Z1*Z0*X1",
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_qubits_folding_to_identity_are_dropped_from_the_product() {
    // Y0*Y0 = I and Z1*Z1 = I, so only X2 is measured.
    check(
        "MPP Y0*Y0*Z1*Z1*X2",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mpp_multiple_products_in_one_instruction_yields_expected_qir() {
    check(
        "MPP X1*Y2 !Z3*Z4*Z5",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 5
            required_num_results: 2"#]],
    );
}

#[test]
fn mpp_mixed_single_and_product_targets_yields_expected_qir() {
    check(
        "MPP X0 Y1*Z2",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 3
            required_num_results: 2"#]],
    );
}

#[test]
fn mpp_product_folding_to_identity_yields_error() {
    // this is temporary
    check(
        "MPP X0*X0",
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedTarget

          x unsupported target in instruction: MPP
           ,----
         1 | MPP X0*X0
           :     ^^^^^
           `----
    "#]],
    );
}

#[test]
fn mpp_product_with_imaginary_phase_yields_anti_hermitian_error() {
    // X0*Y0 = iZ0, which is not hermitian and cannot be measured.
    check(
        "MPP X0*Y0",
        &expect![[r#"
            Qdk.Stim.Compiler.AntiHermitianPauliProduct

              x Pauli product must be Hermitian
               ,----
             1 | MPP X0*Y0
               :     ^^^^^
               `----
        "#]],
    );
}

#[test]
fn mpp_with_qubit_target_yields_unsupported_target_error() {
    check(
        "MPP 0",
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedTarget

          x unsupported target in instruction: MPP
           ,----
         1 | MPP 0
           :     ^
           `----
    "#]],
    );
}

#[test]
fn mpp_with_measurement_record_target_yields_unsupported_target_error() {
    let source = indoc! {"
        M 0
        MPP rec[-1]
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedTarget

          x unsupported target in instruction: MPP
           ,-[2:5]
         1 | M 0
         2 | MPP rec[-1]
           :     ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn mpp_with_loss_target_yields_unsupported_target_error() {
    check(
        "MPP L0",
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedTarget

          x unsupported target in instruction: MPP
           ,----
         1 | MPP L0
           :     ^^
           `----
    "#]],
    );
}

#[test]
fn mpp_with_readout_noise_yields_expected_qir() {
    check(
        "MPP(0.01) Z1*Z2",
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__rt__readout_noise(double 0.01, double 0.01, ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 2
            required_num_results: 1
            uses_noise: true"#]],
    );
}

#[test]
fn mpp_with_invalid_readout_noise_yields_error() {
    check(
        "MPP(1.1) Z1*Z2",
        &expect![[r#"
            Qdk.Stim.Compiler.InvalidProbability

              x probability for MPP must be between 0 and 1; found 1.1
               ,----
             1 | MPP(1.1) Z1*Z2
               :     ^^^
               `----
        "#]],
    );
    check(
        "MPP(-0.1) Z1*Z2",
        &expect![[r#"
            Qdk.Stim.Compiler.InvalidProbability

              x probability for MPP must be between 0 and 1; found -0.1
               ,----
             1 | MPP(-0.1) Z1*Z2
               :     ^^^^
               `----
        "#]],
    );
}

#[test]
fn mpp_with_readout_noise_in_radians_yields_error() {
    check(
        "MPP(0.01rad) Z1*Z2",
        &expect![[r#"
        Qdk.Stim.Compiler.UnexpectedRadians

          x argument for MPP cannot be specified in radians
           ,----
         1 | MPP(0.01rad) Z1*Z2
           :     ^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn spp_single_z_yields_expected_qir() {
    check(
        "SPP Z1",
        &expect![[r#"
            body:
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 1
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_single_x_yields_expected_qir() {
    check(
        "SPP X1",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 1
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_negated_single_x_yields_expected_qir() {
    check(
        "SPP !X1",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)

            required_num_qubits: 1
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_two_factor_product_yields_expected_qir() {
    check(
        "SPP X1*X2",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_multiple_products_in_one_instruction_yield_expected_qir() {
    check(
        "SPP Y1*Y2 !Z1*Z2",
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_negated_three_factor_product_yields_expected_qir() {
    check(
        "SPP !X1*Y2*Z3",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 3
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_mixed_basis_product_yields_expected_qir() {
    check(
        "SPP X0*Y1*Z2",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 3
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_folded_minus_one_negates_correctly() {
    check(
        "SPP X0*Y0*X1*Y1",
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_identity_products_are_noops() {
    let source = indoc! {"
    SPP X0*X0 !Y1*Y1
    SPP_DAG Z2*Z2 !X3*X3
  "};
    check(
        source,
        &expect![[r#"
            required_num_qubits: 0
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_anti_hermitian_product_yields_error() {
    check(
        "SPP X0*Y0",
        &expect![[r#"
            Qdk.Stim.Compiler.AntiHermitianPauliProduct

              x Pauli product must be Hermitian
               ,----
             1 | SPP X0*Y0
               :     ^^^^^
               `----
        "#]],
    );
}

#[test]
fn spp_with_argument_yields_error() {
    check(
        "SPP(0.001) Z0",
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedArgument

              x unsupported argument in instruction: SPP
               ,----
             1 | SPP(0.001) Z0
               :     ^^^^^
               `----
        "#]],
    );
}

#[test]
fn spp_dag_single_z_yields_expected_qir() {
    check(
        "SPP_DAG Z1",
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__s__adj(ptr)

            required_num_qubits: 1
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_dag_single_x_yields_expected_qir() {
    check(
        "SPP_DAG X1",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)

            required_num_qubits: 1
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_dag_negated_single_x_yields_expected_qir() {
    check(
        "SPP_DAG !X1",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 1
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_dag_two_factor_product_yields_expected_qir() {
    check(
        "SPP_DAG X1*X2",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_dag_multiple_products_in_one_instruction_yield_expected_qir() {
    check(
        "SPP_DAG Y1*Y2 !Z1*Z2",
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn spp_dag_negated_three_factor_product_yields_expected_qir() {
    check(
        "SPP_DAG !X1*Y2*Z3",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 3
            required_num_results: 0"#]],
    );
}
