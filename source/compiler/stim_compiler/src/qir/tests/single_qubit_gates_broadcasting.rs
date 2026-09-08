// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn i_gate_yields_expected_qir() {
    let source = "I 0 1";
    check(
        source,
        &expect![[r#"
            required_num_qubits: 0
            required_num_results: 0"#]],
    );
}

#[test]
fn x_gate_yields_expected_qir() {
    let source = "X 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn y_gate_yields_expected_qir() {
    let source = "Y 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__y__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__y__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__y__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn z_gate_yields_expected_qir() {
    let source = "Z 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn c_nxyz_gate_yields_expected_qir() {
    let source = "C_NXYZ 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn c_nzyx_gate_yields_expected_qir() {
    let source = "C_NZYX 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn c_xnyz_gate_yields_expected_qir() {
    let source = "C_XNYZ 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn c_xynz_gate_yields_expected_qir() {
    let source = "C_XYNZ 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn c_xyz_gate_yields_expected_qir() {
    let source = "C_XYZ 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn c_znyx_gate_yields_expected_qir() {
    let source = "C_ZNYX 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn c_zynx_gate_yields_expected_qir() {
    let source = "C_ZYNX 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn c_zyx_gate_yields_expected_qir() {
    let source = "C_ZYX 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn h_gate_yields_expected_qir() {
    let source = "H 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn h_xz_gate_yields_expected_qir() {
    let source = "H_XZ 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn h_nxy_gate_yields_expected_qir() {
    let source = "H_NXY 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn h_nxz_gate_yields_expected_qir() {
    let source = "H_NXZ 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn h_nyz_gate_yields_expected_qir() {
    let source = "H_NYZ 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__sx__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__sx__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__sx__body(ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn h_xy_gate_yields_expected_qir() {
    let source = "H_XY 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn h_yz_gate_yields_expected_qir() {
    let source = "H_YZ 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__sx__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__sx__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__sx__body(ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn s_gate_yields_expected_qir() {
    let source = "S 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn sqrt_z_gate_yields_expected_qir() {
    let source = "SQRT_Z 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn sqrt_x_gate_yields_expected_qir() {
    let source = "SQRT_X 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__sx__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__sx__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__sx__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn sqrt_x_dag_gate_yields_expected_qir() {
    let source = "SQRT_X_DAG 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn sqrt_y_gate_yields_expected_qir() {
    let source = "SQRT_Y 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn sqrt_y_dag_gate_yields_expected_qir() {
    let source = "SQRT_Y_DAG 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn s_dag_gate_yields_expected_qir() {
    let source = "S_DAG 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__s__adj(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn sqrt_z_dag_gate_yields_expected_qir() {
    let source = "SQRT_Z_DAG 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__s__adj(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}
