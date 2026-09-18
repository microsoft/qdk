// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;

#[test]
fn mxx_measurement_yields_correct_qir() {
    let source = "MXX 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 2"#]],
    );
}

#[test]
fn mxx_with_negated_target_yields_correct_qir() {
    let source = "MXX !0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 4
            required_num_results: 2"#]],
    );
}

#[test]
fn mxx_with_readout_noise_yields_correct_qir() {
    check(
        "MXX(0.01) 0 1 2 3",
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__rt__readout_noise(double 0.01, double 0.01, ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__rt__readout_noise(double 0.01, double 0.01, ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 4
            required_num_results: 2
            uses_noise: true"#]],
    );
}

#[test]
fn myy_measurement_yields_correct_qir() {
    let source = "MYY 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
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
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 4
            required_num_results: 2"#]],
    );
}

#[test]
fn myy_with_negated_target_yields_correct_qir() {
    let source = "MYY !0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__x__body(ptr)
              declare void @__quantum__qis__z__body(ptr)

            required_num_qubits: 4
            required_num_results: 2"#]],
    );
}

#[test]
fn myy_with_readout_noise_yields_correct_qir() {
    check(
        "MYY(0.01) 0 1 2 3",
        &expect![[r#"
            body:
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
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__z__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
                call void @__quantum__rt__readout_noise(double 0.01, double 0.01, ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__z__body(ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 4
            required_num_results: 2
            uses_noise: true"#]],
    );
}

#[test]
fn mzz_measurement_yields_correct_qir() {
    let source = "MZZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)

            required_num_qubits: 4
            required_num_results: 2"#]],
    );
}

#[test]
fn mzz_with_negated_target_yields_correct_qir() {
    let source = "MZZ !0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 4
            required_num_results: 2"#]],
    );
}

#[test]
fn mzz_with_readout_noise_yields_correct_qir() {
    check(
        "MZZ(0.01) 0 1 2 3",
        &expect![[r#"
            body:
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__rt__readout_noise(double 0.01, double 0.01, ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
                call void @__quantum__rt__readout_noise(double 0.01, double 0.01, ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @__quantum__qis__cx__body(ptr, ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 4
            required_num_results: 2
            uses_noise: true"#]],
    );
}

#[test]
fn mzz_with_odd_number_of_targets_yields_error() {
    let source = "MZZ 0 1 2";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.OddTargetCount

              x instruction MZZ requires an even number of targets
               ,----
             1 | MZZ 0 1 2
               : ^^^^^^^^^
               `----
        "#]],
    );
}
