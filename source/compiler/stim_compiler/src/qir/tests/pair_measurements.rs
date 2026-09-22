// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;

#[test]
fn mxx_measurement_yields_correct_qir() {
    let source = "MXX 0 1";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 1"#]],
    );
}

#[test]
fn mxx_with_readout_noise_yields_correct_qir() {
    check(
        "MXX(0.01) 0 1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__rt__readout_noise(double 0.01, double 0.01, ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            [metadata]
              required_num_qubits = 2
              required_num_results = 1
              uses_noise = true"#]],
    );
}

#[test]
fn myy_measurement_yields_correct_qir() {
    let source = "MYY 0 1";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__z__body(ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 1"#]],
    );
}

#[test]
fn myy_with_readout_noise_yields_correct_qir() {
    check(
        "MYY(0.01) 0 1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__rt__readout_noise(double 0.01, double 0.01, ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__z__body(ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            [metadata]
              required_num_qubits = 2
              required_num_results = 1
              uses_noise = true"#]],
    );
}

#[test]
fn mzz_measurement_yields_correct_qir() {
    let source = "MZZ 0 1";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [declarations]
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)

            [metadata]
              required_num_qubits = 2
              required_num_results = 1"#]],
    );
}

#[test]
fn mzz_with_readout_noise_yields_correct_qir() {
    check(
        "MZZ(0.01) 0 1",
        &expect![[r#"
            [entry_point]
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__rt__readout_noise(double 0.01, double 0.01, ptr inttoptr (i64 0 to ptr))

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
