// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::compiler::tests::check;
use expect_test::expect;

#[test]
fn m_gate_yields_expected_qir() {
    let source = "M 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    set r_0 = M(qs[0]);
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn m_gate_with_negated_target_yields_expected_qir() {
    check(
        "M !0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    X(qs[0]);
                    set r_0 = M(qs[0]);
                    X(qs[0]);
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn m_gate_with_readout_noise_yields_expected_qir() {
    check(
        "M(0.1) 0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    set r_0 = M(qs[0]);
                    __quantum__rt__readout_noise(0.1, 0.1, r_0);
                    [r_0]
                }
                @NoiseIntrinsic()
                operation __quantum__rt__readout_noise(arg0 : Double, arg1 : Double, arg2 : Result) : Unit {
                    body intrinsic;
                }
            }"#]],
    );
}

#[test]
fn mr_gate_yields_expected_qir() {
    let source = "MR 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    set r_0 = MResetZ(qs[0]);
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mr_gate_with_negated_target_yields_expected_qir() {
    check(
        "MR !0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    X(qs[0]);
                    set r_0 = MResetZ(qs[0]);
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mrx_gate_yields_expected_qir() {
    let source = "MRX 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    H(qs[0]);
                    set r_0 = MResetZ(qs[0]);
                    H(qs[0]);
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mry_gate_yields_expected_qir() {
    let source = "MRY 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    Adjoint S(qs[0]);
                    H(qs[0]);
                    set r_0 = MResetZ(qs[0]);
                    H(qs[0]);
                    S(qs[0]);
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mx_gate_yields_expected_qir() {
    let source = "MX 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    H(qs[0]);
                    set r_0 = M(qs[0]);
                    H(qs[0]);
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn my_gate_yields_expected_qir() {
    let source = "MY 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    Adjoint S(qs[0]);
                    H(qs[0]);
                    set r_0 = M(qs[0]);
                    H(qs[0]);
                    S(qs[0]);
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn r_gate_yields_expected_qir() {
    let source = "R 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Reset(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn rx_gate_yields_expected_qir() {
    let source = "RX 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Reset(qs[0]);
                    H(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn ry_gate_yields_expected_qir() {
    let source = "RY 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Reset(qs[0]);
                    H(qs[0]);
                    S(qs[0]);
                    []
                }
            }"#]],
    );
}
