// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn t_gate_yields_expected_qir() {
    check(
        "T 0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__t__body(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn t_dag_gate_yields_expected_qir() {
    check(
        "T_DAG 0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__t__adj(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__t__adj(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn tpp_single_z_yields_expected_qir() {
    // same as T 0
    check(
        "TPP Z0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__t__body(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn tpp_single_x_yields_expected_qir() {
    check(
        "TPP X0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__t__body(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn tpp_single_y_yields_expected_qir() {
    check(
        "TPP Y0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__t__body(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn tpp_dag_single_z_yields_expected_qir() {
    // same as T_DAG 0
    check(
        "TPP_DAG Z0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__t__adj(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__t__adj(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn tpp_three_factor_product_yields_expected_qir() {
    check(
        "TPP X0*Y1*Z2",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))
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
              declare void @__quantum__qis__t__body(ptr)

            [metadata]
              required_num_qubits = 3
              required_num_results = 0"#]],
    );
}

#[test]
fn tpp_negated_product_applies_inverse() {
    check(
        "TPP !Z0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__t__adj(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__t__adj(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn tpp_dag_negated_product_applies_inverse() {
    check(
        "TPP_DAG !Z0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__t__body(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn tpp_identity_products_are_noops() {
    let source = indoc! {"
        TPP X0*X0 !Y1*Y1
        TPP_DAG Z2*Z2 !X3*X3
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
fn ch_gate_yields_expected_qir() {
    check(
        "CH 0 1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__ry__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__ry__body(double -0.7853981633974483, ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__ry__body(double, ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 0"#]],
    );
}

#[test]
fn ccz_gate_yields_expected_qir() {
    check(
        "CCZ 0 1 2",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__ccx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)

            [metadata]
              required_num_qubits = 3
              required_num_results = 0"#]],
    );
}

#[test]
fn ccx_gate_yields_expected_qir() {
    check(
        "CCX 0 1 2",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__ccx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))

            [declarations]
              declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)

            [metadata]
              required_num_qubits = 3
              required_num_results = 0"#]],
    );
}

#[test]
fn r_x_yields_expected_qir() {
    check(
        "R_X(0.25) 0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__rx__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__rx__body(double, ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn r_y_yields_expected_qir() {
    check(
        "R_Y(-0.375) 0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__ry__body(double -1.1780972450961724, ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__ry__body(double, ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn r_z_yields_expected_qir() {
    check(
        "R_Z(123.432) 0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__rz__body(double 387.77306441789534, ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__rz__body(double, ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn u3_yields_expected_qir() {
    check(
        "U3(0.1, 0.2, 0.3) 0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__rz__body(double 0.9424777960769379, ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__ry__body(double 0.3141592653589793, ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__rz__body(double 0.6283185307179586, ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__ry__body(double, ptr)
              declare void @__quantum__qis__rz__body(double, ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn r_xx_yields_expected_qir() {
    check(
        "R_XX(0.25) 0 1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__rxx__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [declarations]
              declare void @__quantum__qis__rxx__body(double, ptr, ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 0"#]],
    );
}

#[test]
fn r_yy_yields_expected_qir() {
    check(
        "R_YY(-0.6) 0 1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__ryy__body(double -1.8849555921538759, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [declarations]
              declare void @__quantum__qis__ryy__body(double, ptr, ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 0"#]],
    );
}

#[test]
fn r_zz_yields_expected_qir() {
    check(
        "R_ZZ(0.25) 0 1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__rzz__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [declarations]
              declare void @__quantum__qis__rzz__body(double, ptr, ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 0"#]],
    );
}

#[test]
fn r_pauli_single_z_yields_expected_qir() {
    // same as R_Z(0.25) 0
    check(
        "R_PAULI(0.25) Z0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__rz__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__rz__body(double, ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn r_pauli_single_x_yields_expected_qir() {
    check(
        "R_PAULI(0.25) X0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__rz__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__rz__body(double, ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn r_pauli_single_y_yields_expected_qir() {
    check(
        "R_PAULI(0.25) Y0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__rz__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__rz__body(double, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}

#[test]
fn r_pauli_mixed_basis_product_yields_expected_qir() {
    check(
        "R_PAULI(0.25) X0*Y1*Z2",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__rz__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__rz__body(double, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            [metadata]
              required_num_qubits = 3
              required_num_results = 0"#]],
    );
}

#[test]
fn r_pauli_negated_product_negates_angle() {
    check(
        "R_PAULI(0.25) !Z0",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__rz__body(double -0.7853981633974483, ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__rz__body(double, ptr)

            [metadata]
              required_num_qubits = 1
              required_num_results = 0"#]],
    );
}
