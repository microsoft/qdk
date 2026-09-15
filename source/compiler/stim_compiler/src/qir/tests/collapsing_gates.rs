// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;

#[test]
fn m_gate_yields_expected_qir() {
    let source = "M 0";
    check(
        source,
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
fn m_gate_with_readout_noise_yields_expected_qir() {
    check(
        "M(0.1) 0",
        &expect![[r#"
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__rt__readout_noise(double 0.1, double 0.1, ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 1
            required_num_results: 1
            uses_noise: true"#]],
    );
}

#[test]
fn m_gate_with_zero_readout_noise_emits_no_noise_call() {
    check(
        "M(0.0) 0",
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
fn m_gate_with_max_readout_noise_yields_expected_qir() {
    check(
        "M(1.0) 0",
        &expect![[r#"
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__rt__readout_noise(double 1.0, double 1.0, ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 1
            required_num_results: 1
            uses_noise: true"#]],
    );
}

#[test]
fn m_gate_with_invalid_readout_noise_yields_error() {
    check(
        "M(1.1) 0",
        &expect![[r#"
            Qdk.Stim.Compiler.InvalidProbability

              x probability for M must be between 0 and 1; found 1.1
               ,----
             1 | M(1.1) 0
               :   ^^^
               `----
        "#]],
    );

    check(
        "M(-0.1) 0",
        &expect![[r#"
            Qdk.Stim.Compiler.InvalidProbability

              x probability for M must be between 0 and 1; found -0.1
               ,----
             1 | M(-0.1) 0
               :   ^^^^
               `----
        "#]],
    );
}

#[test]
fn m_gate_with_readout_noise_in_radians_yields_error() {
    check(
        "M(0.1rad) 0",
        &expect![[r#"
        Qdk.Stim.Compiler.UnexpectedRadians

          x argument for M cannot be specified in radians
           ,----
         1 | M(0.1rad) 0
           :   ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn m_gate_with_two_args_yields_error() {
    check(
        "M(0.1, 0.2) 0",
        &expect![[r#"
            Qdk.Stim.Compiler.TooManyArgs

              x too many arguments for instruction M; expected 1, found 2
               ,----
             1 | M(0.1, 0.2) 0
               :        ^^^
               `----
        "#]],
    );
}

#[test]
fn mr_gate_yields_expected_qir() {
    let source = "MR 0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__mresetz__body(ptr, ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mr_gate_with_readout_noise_yields_expected_qir() {
    check(
        "MR(0.1) 0",
        &expect![[r#"
            body:
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__rt__readout_noise(double 0.1, double 0.1, ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__mresetz__body(ptr, ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 1
            required_num_results: 1
            uses_noise: true"#]],
    );
}

#[test]
fn mrx_gate_yields_expected_qir() {
    let source = "MRX 0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__mresetz__body(ptr, ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mrx_gate_with_readout_noise_yields_expected_qir() {
    check(
        "MRX(0.1) 0",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__rt__readout_noise(double 0.1, double 0.1, ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__mresetz__body(ptr, ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 1
            required_num_results: 1
            uses_noise: true"#]],
    );
}

#[test]
fn mry_gate_yields_expected_qir() {
    let source = "MRY 0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__mresetz__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mry_gate_with_readout_noise_yields_expected_qir() {
    check(
        "MRY(0.1) 0",
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__rt__readout_noise(double 0.1, double 0.1, ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__mresetz__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 1
            required_num_results: 1
            uses_noise: true"#]],
    );
}

#[test]
fn mrz_gate_yields_expected_qir() {
    let source = "MRZ 0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__mresetz__body(ptr, ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mx_gate_yields_expected_qir() {
    let source = "MX 0";
    check(
        source,
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
fn mx_gate_with_readout_noise_yields_expected_qir() {
    check(
        "MX(0.1) 0",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__rt__readout_noise(double 0.1, double 0.1, ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 1
            required_num_results: 1
            uses_noise: true"#]],
    );
}

#[test]
fn my_gate_yields_expected_qir() {
    let source = "MY 0";
    check(
        source,
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
fn my_gate_with_readout_noise_yields_expected_qir() {
    check(
        "MY(0.1) 0",
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__rt__readout_noise(double 0.1, double 0.1, ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 1
            required_num_results: 1
            uses_noise: true"#]],
    );
}

#[test]
fn mz_gate_yields_expected_qir() {
    let source = "MZ 0";
    check(
        source,
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
fn r_gate_yields_expected_qir() {
    let source = "R 0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__reset__body(ptr)

            required_num_qubits: 1
            required_num_results: 0"#]],
    );
}

#[test]
fn rx_gate_yields_expected_qir() {
    let source = "RX 0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__reset__body(ptr)

            required_num_qubits: 1
            required_num_results: 0"#]],
    );
}

#[test]
fn ry_gate_yields_expected_qir() {
    let source = "RY 0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__reset__body(ptr)
              declare void @__quantum__qis__s__body(ptr)

            required_num_qubits: 1
            required_num_results: 0"#]],
    );
}

#[test]
fn m_gate_with_negated_target_yields_expected_qir() {
    let source = "M !0";
    check(
        source,
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
fn mx_gate_with_negated_target_yields_expected_qir() {
    let source = "MX !0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mx_gate_with_negated_target_and_readout_noise_yields_expected_qir() {
    check(
        "MX(0.1) !0",
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__rt__readout_noise(double 0.1, double 0.1, ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 1
            required_num_results: 1
            uses_noise: true"#]],
    );
}

#[test]
fn my_gate_with_negated_target_yields_expected_qir() {
    let source = "MY !0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mr_gate_with_negated_target_yields_expected_qir() {
    let source = "MR !0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__mresetz__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mrx_gate_with_negated_target_yields_expected_qir() {
    let source = "MRX !0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__mresetz__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn mry_gate_with_negated_target_yields_expected_qir() {
    let source = "MRY !0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__h__body(ptr)
              declare void @__quantum__qis__mresetz__body(ptr, ptr)
              declare void @__quantum__qis__s__adj(ptr)
              declare void @__quantum__qis__s__body(ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn rz_gate_yields_expected_qir() {
    let source = "RZ 0";
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__reset__body(ptr)

            required_num_qubits: 1
            required_num_results: 0"#]],
    );
}
