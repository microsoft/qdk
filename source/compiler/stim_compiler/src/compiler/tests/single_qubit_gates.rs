// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn i_gate_yields_expected_qsharp() {
    let source = "I 0";
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
fn x_gate_yields_expected_qsharp() {
    let source = "X 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    X(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn y_gate_yields_expected_qsharp() {
    let source = "Y 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Y(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn z_gate_yields_expected_qsharp() {
    let source = "Z 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Z(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn c_nxyz_gate_yields_expected_qsharp() {
    let source = "C_NXYZ 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Adjoint S(qs[0]);
                    H(qs[0]);
                    Z(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn c_nzyx_gate_yields_expected_qsharp() {
    let source = "C_NZYX 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Z(qs[0]);
                    H(qs[0]);
                    Adjoint S(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn c_xnyz_gate_yields_expected_qsharp() {
    let source = "C_XNYZ 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    S(qs[0]);
                    H(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn c_xynz_gate_yields_expected_qsharp() {
    let source = "C_XYNZ 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    S(qs[0]);
                    H(qs[0]);
                    Z(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn c_xyz_gate_yields_expected_qsharp() {
    let source = "C_XYZ 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Adjoint S(qs[0]);
                    H(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn c_znyx_gate_yields_expected_qsharp() {
    let source = "C_ZNYX 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    H(qs[0]);
                    Adjoint S(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn c_zynx_gate_yields_expected_qsharp() {
    let source = "C_ZYNX 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Z(qs[0]);
                    H(qs[0]);
                    S(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn c_zyx_gate_yields_expected_qsharp() {
    let source = "C_ZYX 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    H(qs[0]);
                    S(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn h_gate_yields_expected_qsharp() {
    let source = "H 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    H(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn h_nxy_gate_yields_expected_qsharp() {
    let source = "H_NXY 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    S(qs[0]);
                    X(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn h_nxz_gate_yields_expected_qsharp() {
    let source = "H_NXZ 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Z(qs[0]);
                    H(qs[0]);
                    Z(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn h_nyz_gate_yields_expected_qsharp() {
    let source = "H_NYZ 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Z(qs[0]);
                    SX(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn h_xy_gate_yields_expected_qsharp() {
    let source = "H_XY 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    X(qs[0]);
                    S(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn h_yz_gate_yields_expected_qsharp() {
    let source = "H_YZ 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    SX(qs[0]);
                    Z(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn s_gate_yields_expected_qsharp() {
    let source = "S 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    S(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn sqrt_x_gate_yields_expected_qsharp() {
    let source = "SQRT_X 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    SX(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn sqrt_x_dag_gate_yields_expected_qsharp() {
    let source = "SQRT_X_DAG 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    S(qs[0]);
                    H(qs[0]);
                    S(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn sqrt_y_gate_yields_expected_qsharp() {
    let source = "SQRT_Y 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Z(qs[0]);
                    H(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn sqrt_y_dag_gate_yields_expected_qsharp() {
    let source = "SQRT_Y_DAG 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    H(qs[0]);
                    Z(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn s_dag_gate_yields_expected_qsharp() {
    let source = "S_DAG 0";
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Adjoint S(qs[0]);
                    []
                }
            }"#]],
    );
}
