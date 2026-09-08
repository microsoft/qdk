// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;

#[test]
fn m_gate_yields_expected_qir() {
    let source = "M 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__m__body(ptr, ptr)

            required_num_qubits: 2
            required_num_results: 2"#]],
    );
}

#[test]
fn m_gate_with_readout_noise_yields_expected_qir() {
    check(
        "M(0.1) 0 1",
        &expect![[r#"
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__rt__readout_noise(double 0.1, double 0.1, ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__rt__readout_noise(double 0.1, double 0.1, ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 2
            required_num_results: 2
            uses_noise: true"#]],
    );
}

#[test]
fn mr_gate_yields_expected_qir() {
    let source = "MR 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__mresetz__body(ptr, ptr)

            required_num_qubits: 2
            required_num_results: 2"#]],
    );
}

#[test]
fn mrx_gate_yields_expected_qir() {
    let source = "MRX 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__mresetz__body(ptr, ptr)

            required_num_qubits: 2
            required_num_results: 2"#]],
    );
}

#[test]
fn mry_gate_yields_expected_qir() {
    let source = "MRY 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__mresetz__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 2"#]],
    );
}

#[test]
fn mrz_gate_yields_expected_qir() {
    let source = "MRZ 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__mresetz__body(ptr, ptr)

            required_num_qubits: 2
            required_num_results: 2"#]],
    );
}

#[test]
fn mx_gate_yields_expected_qir() {
    let source = "MX 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)

            required_num_qubits: 2
            required_num_results: 2"#]],
    );
}

#[test]
fn my_gate_yields_expected_qir() {
    let source = "MY 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 2"#]],
    );
}

#[test]
fn mz_gate_yields_expected_qir() {
    let source = "MZ 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__m__body(ptr, ptr)

            required_num_qubits: 2
            required_num_results: 2"#]],
    );
}

#[test]
fn r_gate_yields_expected_qir() {
    let source = "R 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__reset__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn rx_gate_yields_expected_qir() {
    let source = "RX 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__reset__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn ry_gate_yields_expected_qir() {
    let source = "RY 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__reset__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}

#[test]
fn rz_gate_yields_expected_qir() {
    let source = "RZ 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__reset__body(ptr)

            required_num_qubits: 2
            required_num_results: 0"#]],
    );
}
