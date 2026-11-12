// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::compiler::tests::check;
use expect_test::expect;

#[test]
fn mxx_measurement_yields_correct_qsharp() {
    let source = "MXX 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    let r_0 = within {
                        CNOT(qs[0], qs[1]);
                        H(qs[0]);
                    } apply {
                        M(qs[0])
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mxx_with_target_negated_yields_correct_qsharp() {
    // negation is already normalized in semantic pass, so this test suffices
    check(
        "MXX !0 1",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    let r_0 = within {
                        CNOT(qs[0], qs[1]);
                        H(qs[0]);
                        X(qs[0]);
                    } apply {
                        M(qs[0])
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mxx_with_readout_noise_yields_correct_qsharp() {
    check(
        "MXX(0.01) 0 1",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    let r_0 = within {
                        CNOT(qs[0], qs[1]);
                        H(qs[0]);
                    } apply {
                        M(qs[0])
                    };
                    __quantum__rt__readout_noise(0.01, 0.01, r_0);
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
fn myy_measurement_yields_correct_qsharp() {
    let source = "MYY 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    let r_0 = within {
                        S(qs[0]);
                        S(qs[1]);
                        CNOT(qs[0], qs[1]);
                        H(qs[0]);
                    } apply {
                        M(qs[0])
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mzz_measurement_yields_correct_qsharp() {
    let source = "MZZ 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    let r_0 = within {
                        CNOT(qs[0], qs[1]);
                    } apply {
                        M(qs[1])
                    };
                    [r_0]
                }
            }"#]],
    );
}
