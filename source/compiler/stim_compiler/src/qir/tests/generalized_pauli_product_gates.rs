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
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 1"#]],
    );
}

#[test]
fn mpp_single_y_yields_expected_qir() {
    // same as MY 0
    check(
        "MPP Y0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 1"#]],
    );
}

#[test]
fn mpp_single_z_yields_expected_qir() {
    // same as MZ 0
    check(
        "MPP Z0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__m__body(ptr, ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 1"#]],
    );
}

#[test]
fn mpp_negated_single_pauli_yields_expected_qir() {
    // same as MZ !5
    check(
        "MPP !Z5",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 1"#]],
    );
}

#[test]
fn mpp_two_factor_product_yields_expected_qir() {
    check(
        "MPP X1*Y2",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 1"#]],
    );
}

#[test]
fn mpp_three_factor_product_yields_expected_qir() {
    // order of qubit indices in the output is not guaranteed to match the order of qubit indices in the input
    check(
        "MPP Z3*Z4*Z5",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)

            [metadata]
              required_num_qubits = 3
              required_num_results = 1"#]],
    );
}

#[test]
fn mpp_product_of_all_three_bases_yields_expected_qir() {
    check(
        "MPP X0*Y1*Z2",
        &expect![[r#"
            [entry_point]
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

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            [metadata]
              required_num_qubits = 3
              required_num_results = 1"#]],
    );
}

#[test]
fn mpp_negated_product_yields_expected_qir() {
    // order of qubit indices in the output is not guaranteed to match the order of qubit indices in the input
    check(
        "MPP !Z3*Z4*Z5",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            [metadata]
              required_num_qubits = 3
              required_num_results = 1"#]],
    );
}

#[test]
fn mpp_with_readout_noise_yields_expected_qir() {
    check(
        "MPP(0.01) Z1*Z2",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__rt__readout_noise(double 0.01, double 0.01, ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            [metadata]
              required_num_qubits = 2
              required_num_results = 1
              uses_noise = true"#]],
    );
}

#[test]
fn spp_single_z_yields_expected_qir() {
    check(
        "SPP Z1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__s__body(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn spp_single_x_yields_expected_qir() {
    check(
        "SPP X1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn spp_negated_single_x_yields_expected_qir() {
    check(
        "SPP !X1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn spp_two_factor_product_yields_expected_qir() {
    check(
        "SPP X1*X2",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 0"#]],
    );
}

#[test]
fn spp_negated_three_factor_product_yields_expected_qir() {
    check(
        "SPP !X1*Y2*Z3",
        &expect![[r#"
            [entry_point]
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

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            [metadata]
              required_num_qubits = 3
              required_num_results = 0"#]],
    );
}

#[test]
fn spp_mixed_basis_product_yields_expected_qir() {
    check(
        "SPP X0*Y1*Z2",
        &expect![[r#"
            [entry_point]
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

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            [metadata]
              required_num_qubits = 3
              required_num_results = 0"#]],
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
            [metadata]
              required_num_qubits = 0
              required_num_results = 0"#]],
    );
}

#[test]
fn spp_dag_single_z_yields_expected_qir() {
    check(
        "SPP_DAG Z1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__s__adj(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn spp_dag_single_x_yields_expected_qir() {
    check(
        "SPP_DAG X1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn spp_dag_negated_single_x_yields_expected_qir() {
    check(
        "SPP_DAG !X1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn spp_dag_two_factor_product_yields_expected_qir() {
    check(
        "SPP_DAG X1*X2",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 0"#]],
    );
}

#[test]
fn spp_dag_negated_three_factor_product_yields_expected_qir() {
    check(
        "SPP_DAG !X1*Y2*Z3",
        &expect![[r#"
            [entry_point]
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

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            [metadata]
              required_num_qubits = 3
              required_num_results = 0"#]],
    );
}
