// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn mpp_single_x_yields_expected_qsharp() {
    // same as MX 0
    check(
        "MPP X0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    within {
                        H(qs[0]);
                    } apply {
                        set r_0 = M(qs[0]);
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mpp_single_y_yields_expected_qsharp() {
    // same as MY 0
    check(
        "MPP Y0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    within {
                        Adjoint S(qs[0]);
                        H(qs[0]);
                    } apply {
                        set r_0 = M(qs[0]);
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mpp_single_z_yields_expected_qsharp() {
    // same as MZ 0
    check(
        "MPP Z0",
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
fn mpp_negated_single_pauli_yields_expected_qsharp() {
    // same as MZ !5
    check(
        "MPP !Z5",
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
fn mpp_two_factor_product_yields_expected_qsharp() {
    check(
        "MPP X1*Y2",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    within {
                        H(qs[0]);
                        Adjoint S(qs[1]);
                        H(qs[1]);
                        CNOT(qs[1], qs[0]);
                    } apply {
                        set r_0 = M(qs[0]);
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mpp_three_factor_product_yields_expected_qsharp() {
    // order of qubit indices in the output is not guaranteed to match the order of qubit indices in the input
    check(
        "MPP Z3*Z4*Z5",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    mutable r_0 = Zero;
                    within {
                        CNOT(qs[0], qs[1]);
                        CNOT(qs[2], qs[1]);
                    } apply {
                        set r_0 = M(qs[1]);
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mpp_product_of_all_three_bases_yields_expected_qsharp() {
    check(
        "MPP X0*Y1*Z2",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    mutable r_0 = Zero;
                    within {
                        H(qs[0]);
                        Adjoint S(qs[1]);
                        H(qs[1]);
                        CNOT(qs[1], qs[0]);
                        CNOT(qs[2], qs[0]);
                    } apply {
                        set r_0 = M(qs[0]);
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mpp_negated_product_yields_expected_qsharp() {
    // order of qubit indices in the output is not guaranteed to match the order of qubit indices in the input
    check(
        "MPP !Z3*Z4*Z5",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    mutable r_0 = Zero;
                    within {
                        CNOT(qs[0], qs[1]);
                        CNOT(qs[2], qs[1]);
                    } apply {
                        X(qs[1]);
                        set r_0 = M(qs[1]);
                        X(qs[1]);
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn mpp_with_readout_noise_yields_expected_qsharp() {
    check(
        "MPP(0.01) Z1*Z2",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    within {
                        CNOT(qs[0], qs[1]);
                    } apply {
                        set r_0 = M(qs[1]);
                        __quantum__rt__readout_noise(0.01, 0.01, r_0);
                    };
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
fn spp_single_z_yields_expected_qsharp() {
    check(
        "SPP Z1",
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
fn spp_single_x_yields_expected_qsharp() {
    check(
        "SPP X1",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {
                        H(qs[0]);
                    } apply {
                        S(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn spp_negated_single_x_yields_expected_qsharp() {
    check(
        "SPP !X1",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {
                        H(qs[0]);
                    } apply {
                        Adjoint S(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn spp_two_factor_product_yields_expected_qsharp() {
    check(
        "SPP X1*X2",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    within {
                        H(qs[0]);
                        H(qs[1]);
                        CNOT(qs[1], qs[0]);
                    } apply {
                        S(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn spp_negated_three_factor_product_yields_expected_qsharp() {
    check(
        "SPP !X1*Y2*Z3",
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
                        Adjoint S(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn spp_mixed_basis_product_yields_expected_qsharp() {
    check(
        "SPP X0*Y1*Z2",
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
                        S(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn spp_identity_products_are_noops() {
    let source = indoc! {"
    SPP X0*X0 !Y1*Y1
    SPP_DAG Z2*Z2 !X3*X3
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
fn spp_dag_single_z_yields_expected_qsharp() {
    check(
        "SPP_DAG Z1",
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

#[test]
fn spp_dag_single_x_yields_expected_qsharp() {
    check(
        "SPP_DAG X1",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {
                        H(qs[0]);
                    } apply {
                        Adjoint S(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn spp_dag_negated_single_x_yields_expected_qsharp() {
    check(
        "SPP_DAG !X1",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    within {
                        H(qs[0]);
                    } apply {
                        S(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn spp_dag_two_factor_product_yields_expected_qsharp() {
    check(
        "SPP_DAG X1*X2",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    within {
                        H(qs[0]);
                        H(qs[1]);
                        CNOT(qs[1], qs[0]);
                    } apply {
                        Adjoint S(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}

#[test]
fn spp_dag_negated_three_factor_product_yields_expected_qsharp() {
    check(
        "SPP_DAG !X1*Y2*Z3",
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
                        S(qs[0]);
                    };
                    []
                }
            }"#]],
    );
}
