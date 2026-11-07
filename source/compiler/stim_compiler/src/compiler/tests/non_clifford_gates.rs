// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn t_gate_yields_expected_qir() {
    check(
        "T 0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    T(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn t_dag_gate_yields_expected_qir() {
    check(
        "T_DAG 0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Adjoint T(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn tpp_single_z_yields_expected_qir() {
    // same as T 0
    check(
        "TPP Z0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {} apply {
                        T(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn tpp_single_x_yields_expected_qir() {
    check(
        "TPP X0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {
                        H(qs[0]);
                    } apply {
                        T(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn tpp_single_y_yields_expected_qir() {
    check(
        "TPP Y0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {
                        Adjoint S(qs[0]);
                        H(qs[0]);
                    } apply {
                        T(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn tpp_dag_single_z_yields_expected_qir() {
    // same as T_DAG 0
    check(
        "TPP_DAG Z0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {} apply {
                        Adjoint T(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn tpp_three_factor_product_yields_expected_qir() {
    check(
        "TPP X0*Y1*Z2",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    within {
                        H(qs[0]);
                        Adjoint S(qs[1]);
                        H(qs[1]);
                        CNOT(qs[1], qs[0]);
                        CNOT(qs[2], qs[0]);
                    } apply {
                        T(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn tpp_negated_product_applies_inverse() {
    check(
        "TPP !Z0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {} apply {
                        Adjoint T(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn tpp_dag_negated_product_applies_inverse() {
    check(
        "TPP_DAG !Z0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {} apply {
                        T(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn tpp_identity_products_are_noops() {
    let source = indoc! {"
        TPP X0*X0 !Y1*Y1
        TPP_DAG Z2*Z2 !X3*X3
    "};
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
fn ch_gate_yields_expected_qir() {
    check(
        "CH 0 1",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    Controlled H([qs[0]], qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn ccz_gate_yields_expected_qir() {
    check(
        "CCZ 0 1 2",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    H(qs[0]);
                    CCNOT(qs[1], qs[2], qs[0]);
                    H(qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn ccx_gate_yields_expected_qir() {
    check(
        "CCX 0 1 2",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    CCNOT(qs[0], qs[1], qs[2]);
                    []
                }
            }"#]],
    );
}

#[test]
fn r_x_yields_expected_qir() {
    check(
        "R_X(0.25) 0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Rx(0.7853981633974483, qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn r_y_yields_expected_qir() {
    check(
        "R_Y(-0.375) 0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Ry(-1.1780972450961724, qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn r_z_yields_expected_qir() {
    check(
        "R_Z(123.432) 0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Rz(387.77306441789534, qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn u3_yields_expected_qir() {
    check(
        "U3(0.1, 0.2, 0.3) 0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    Rz(0.9424777960769379, qs[0]);
                    Ry(0.3141592653589793, qs[0]);
                    Rz(0.6283185307179586, qs[0]);
                    []
                }
            }"#]],
    );
}

#[test]
fn r_xx_yields_expected_qir() {
    check(
        "R_XX(0.25) 0 1",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    Rxx(0.7853981633974483, qs[0], qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn r_yy_yields_expected_qir() {
    check(
        "R_YY(-0.6) 0 1",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    Ryy(-1.8849555921538759, qs[0], qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn r_zz_yields_expected_qir() {
    check(
        "R_ZZ(0.25) 0 1",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    Rzz(0.7853981633974483, qs[0], qs[1]);
                    []
                }
            }"#]],
    );
}

#[test]
fn r_pauli_single_z_yields_expected_qir() {
    // same as R_Z(0.25) 0
    check(
        "R_PAULI(0.25) Z0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {} apply {
                        Rz(0.7853981633974483, qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn r_pauli_single_x_yields_expected_qir() {
    check(
        "R_PAULI(0.25) X0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {
                        H(qs[0]);
                    } apply {
                        Rz(0.7853981633974483, qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn r_pauli_single_y_yields_expected_qir() {
    check(
        "R_PAULI(0.25) Y0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {
                        Adjoint S(qs[0]);
                        H(qs[0]);
                    } apply {
                        Rz(0.7853981633974483, qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn r_pauli_mixed_basis_product_yields_expected_qir() {
    check(
        "R_PAULI(0.25) X0*Y1*Z2",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    within {
                        H(qs[0]);
                        Adjoint S(qs[1]);
                        H(qs[1]);
                        CNOT(qs[1], qs[0]);
                        CNOT(qs[2], qs[0]);
                    } apply {
                        Rz(0.7853981633974483, qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn r_pauli_negated_product_negates_angle() {
    check(
        "R_PAULI(0.25) !Z0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {} apply {
                        Rz(-0.7853981633974483, qs[0]);
                    };
                    []
                }
            }"#]],
    );
}
