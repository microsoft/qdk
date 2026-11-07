// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn cx_gate_yields_expected_qsharp() {
    let source = "CX 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    CNOT(qs[0], qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn cxswap_gate_yields_expected_qsharp() {
    let source = "CXSWAP 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    CNOT(qs[0], qs[1]);
                    CNOT(qs[1], qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn cy_gate_yields_expected_qsharp() {
    let source = "CY 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    CY(qs[0], qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn cz_gate_yields_expected_qsharp() {
    let source = "CZ 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    CZ(qs[0], qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn czswap_gate_yields_expected_qsharp() {
    let source = "CZSWAP 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    H(qs[0]);
                    CNOT(qs[0], qs[1]);
                    CNOT(qs[1], qs[0]);
                    H(qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn ii_gate_yields_expected_qsharp() {
    let source = "II 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    []
                }
            }"#]],
    );
}

#[test]
fn iswap_gate_yields_expected_qsharp() {
    let source = "ISWAP 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    H(qs[0]);
                    CNOT(qs[0], qs[1]);
                    CNOT(qs[1], qs[0]);
                    H(qs[1]);
                    S(qs[1]);
                    S(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn iswap_dag_gate_yields_expected_qsharp() {
    let source = "ISWAP_DAG 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    Adjoint S(qs[0]);
                    Adjoint S(qs[1]);
                    H(qs[1]);
                    CNOT(qs[1], qs[0]);
                    CNOT(qs[0], qs[1]);
                    H(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn sqrt_xx_gate_yields_expected_qsharp() {
    let source = "SQRT_XX 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    H(qs[0]);
                    CNOT(qs[0], qs[1]);
                    H(qs[1]);
                    S(qs[0]);
                    S(qs[1]);
                    H(qs[0]);
                    H(qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn sqrt_xx_dag_gate_yields_expected_qsharp() {
    let source = "SQRT_XX_DAG 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    H(qs[0]);
                    CNOT(qs[0], qs[1]);
                    H(qs[1]);
                    Adjoint S(qs[0]);
                    Adjoint S(qs[1]);
                    H(qs[0]);
                    H(qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn sqrt_yy_gate_yields_expected_qsharp() {
    let source = "SQRT_YY 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    Adjoint S(qs[0]);
                    Adjoint S(qs[1]);
                    H(qs[0]);
                    CNOT(qs[0], qs[1]);
                    H(qs[1]);
                    S(qs[0]);
                    S(qs[1]);
                    H(qs[0]);
                    H(qs[1]);
                    S(qs[0]);
                    S(qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn sqrt_yy_dag_gate_yields_expected_qsharp() {
    let source = "SQRT_YY_DAG 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    Adjoint S(qs[0]);
                    S(qs[1]);
                    H(qs[0]);
                    CNOT(qs[0], qs[1]);
                    H(qs[1]);
                    S(qs[0]);
                    S(qs[1]);
                    H(qs[0]);
                    H(qs[1]);
                    S(qs[0]);
                    Adjoint S(qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn sqrt_zz_gate_yields_expected_qsharp() {
    let source = "SQRT_ZZ 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    H(qs[0]);
                    CNOT(qs[1], qs[0]);
                    H(qs[0]);
                    S(qs[1]);
                    S(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn sqrt_zz_dag_gate_yields_expected_qsharp() {
    let source = "SQRT_ZZ_DAG 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    H(qs[0]);
                    CNOT(qs[1], qs[0]);
                    H(qs[0]);
                    Adjoint S(qs[1]);
                    Adjoint S(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn swap_gate_yields_expected_qsharp() {
    let source = "SWAP 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    SWAP(qs[0], qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn swapcx_gate_yields_expected_qsharp() {
    let source = "SWAPCX 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    CNOT(qs[0], qs[1]);
                    CNOT(qs[1], qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn xcx_gate_yields_expected_qsharp() {
    let source = "XCX 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    H(qs[0]);
                    CNOT(qs[0], qs[1]);
                    H(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn xcy_gate_yields_expected_qsharp() {
    let source = "XCY 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    H(qs[0]);
                    Adjoint S(qs[1]);
                    CNOT(qs[0], qs[1]);
                    H(qs[0]);
                    S(qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn xcz_gate_yields_expected_qsharp() {
    let source = "XCZ 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    CNOT(qs[0], qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn ycx_gate_yields_expected_qsharp() {
    let source = "YCX 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    Adjoint S(qs[0]);
                    H(qs[1]);
                    CNOT(qs[1], qs[0]);
                    S(qs[0]);
                    H(qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn ycy_gate_yields_expected_qsharp() {
    let source = "YCY 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    Adjoint S(qs[0]);
                    Adjoint S(qs[1]);
                    H(qs[0]);
                    CNOT(qs[0], qs[1]);
                    H(qs[0]);
                    S(qs[0]);
                    S(qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn ycz_gate_yields_expected_qsharp() {
    let source = "YCZ 0 1";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    Adjoint S(qs[0]);
                    CNOT(qs[1], qs[0]);
                    S(qs[0]);
                    []
                }
            }"#]],
    );
}
