// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn cx_gate_yields_expected_qir() {
    let source = "CX 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn cnot_gate_yields_expected_qir() {
    let source = "CNOT 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn zcx_gate_yields_expected_qir() {
    let source = "ZCX 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn cxswap_gate_yields_expected_qir() {
    let source = "CXSWAP 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn cy_gate_yields_expected_qir() {
    let source = "CY 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cy__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cy__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cy__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn zcy_gate_yields_expected_qir() {
    let source = "ZCY 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cy__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cy__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cy__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn cz_gate_yields_expected_qir() {
    let source = "CZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cz__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn zcz_gate_yields_expected_qir() {
    let source = "ZCZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cz__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn czswap_gate_yields_expected_qir() {
    let source = "CZSWAP 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn swapcz_gate_yields_expected_qir() {
    let source = "SWAPCZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn ii_gate_yields_expected_qir() {
    let source = "II 0 1 2 3";
    check(
        source,
        &expect![[r#"
            required_num_qubits: 0
            required_num_results: 0"#]],
    );
}

#[test]
fn iswap_gate_yields_expected_qir() {
    let source = "ISWAP 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn iswap_dag_gate_yields_expected_qir() {
    let source = "ISWAP_DAG 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn sqrt_xx_gate_yields_expected_qir() {
    let source = "SQRT_XX 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn sqrt_xx_dag_gate_yields_expected_qir() {
    let source = "SQRT_XX_DAG 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn sqrt_yy_gate_yields_expected_qir() {
    let source = "SQRT_YY 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn sqrt_yy_dag_gate_yields_expected_qir() {
    let source = "SQRT_YY_DAG 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn sqrt_zz_gate_yields_expected_qir() {
    let source = "SQRT_ZZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn sqrt_zz_dag_gate_yields_expected_qir() {
    let source = "SQRT_ZZ_DAG 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn swap_gate_yields_expected_qir() {
    let source = "SWAP 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__swap__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__swap__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__swap__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn swapcx_gate_yields_expected_qir() {
    let source = "SWAPCX 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn xcx_gate_yields_expected_qir() {
    let source = "XCX 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn xcy_gate_yields_expected_qir() {
    let source = "XCY 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn xcz_gate_yields_expected_qir() {
    let source = "XCZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn ycx_gate_yields_expected_qir() {
    let source = "YCX 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn ycy_gate_yields_expected_qir() {
    let source = "YCY 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn ycz_gate_yields_expected_qir() {
    let source = "YCZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 4
            required_num_results: 0"#]],
    );
}

#[test]
fn cx_with_odd_number_of_targets_yields_error() {
    let source = "CX 0 1 2";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.OddTargetCount

              x instruction CX requires an even number of targets
               ,----
             1 | CX 0 1 2
               : ^^^^^^^^
               `----
        "#]],
    );
}
