// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

// Single-qubit gates

#[test]
fn i_lowers() {
    check(
        "I 0",
        &expect![[r#"
        Circuit [0-3]:
            items:
                [0-3] SingleQubitGate {
                    qubit: 0,
                    gate: I,
                }"#]],
    );
}

#[test]
fn x_lowers() {
    check(
        "X 0",
        &expect![[r#"
        Circuit [0-3]:
            items:
                [0-3] SingleQubitGate {
                    qubit: 0,
                    gate: X,
                }"#]],
    );
}

#[test]
fn y_lowers() {
    check(
        "Y 0",
        &expect![[r#"
        Circuit [0-3]:
            items:
                [0-3] SingleQubitGate {
                    qubit: 0,
                    gate: Y,
                }"#]],
    );
}

#[test]
fn z_lowers() {
    check(
        "Z 0",
        &expect![[r#"
        Circuit [0-3]:
            items:
                [0-3] SingleQubitGate {
                    qubit: 0,
                    gate: Z,
                }"#]],
    );
}

#[test]
fn c_nxyz_lowers() {
    check(
        "C_NXYZ 0",
        &expect![[r#"
        Circuit [0-8]:
            items:
                [0-8] SingleQubitGate {
                    qubit: 0,
                    gate: C_NXYZ,
                }"#]],
    );
}

#[test]
fn c_nzyx_lowers() {
    check(
        "C_NZYX 0",
        &expect![[r#"
        Circuit [0-8]:
            items:
                [0-8] SingleQubitGate {
                    qubit: 0,
                    gate: C_NZYX,
                }"#]],
    );
}

#[test]
fn c_xnyz_lowers() {
    check(
        "C_XNYZ 0",
        &expect![[r#"
        Circuit [0-8]:
            items:
                [0-8] SingleQubitGate {
                    qubit: 0,
                    gate: C_XNYZ,
                }"#]],
    );
}

#[test]
fn c_xynz_lowers() {
    check(
        "C_XYNZ 0",
        &expect![[r#"
        Circuit [0-8]:
            items:
                [0-8] SingleQubitGate {
                    qubit: 0,
                    gate: C_XYNZ,
                }"#]],
    );
}

#[test]
fn c_xyz_lowers() {
    check(
        "C_XYZ 0",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] SingleQubitGate {
                    qubit: 0,
                    gate: C_XYZ,
                }"#]],
    );
}

#[test]
fn c_znyx_lowers() {
    check(
        "C_ZNYX 0",
        &expect![[r#"
        Circuit [0-8]:
            items:
                [0-8] SingleQubitGate {
                    qubit: 0,
                    gate: C_ZNYX,
                }"#]],
    );
}

#[test]
fn c_zynx_lowers() {
    check(
        "C_ZYNX 0",
        &expect![[r#"
        Circuit [0-8]:
            items:
                [0-8] SingleQubitGate {
                    qubit: 0,
                    gate: C_ZYNX,
                }"#]],
    );
}

#[test]
fn c_zyx_lowers() {
    check(
        "C_ZYX 0",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] SingleQubitGate {
                    qubit: 0,
                    gate: C_ZYX,
                }"#]],
    );
}

#[test]
fn h_lowers() {
    check(
        "H 0",
        &expect![[r#"
        Circuit [0-3]:
            items:
                [0-3] SingleQubitGate {
                    qubit: 0,
                    gate: H,
                }"#]],
    );
}

#[test]
fn h_nxy_lowers() {
    check(
        "H_NXY 0",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] SingleQubitGate {
                    qubit: 0,
                    gate: H_NXY,
                }"#]],
    );
}

#[test]
fn h_nxz_lowers() {
    check(
        "H_NXZ 0",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] SingleQubitGate {
                    qubit: 0,
                    gate: H_NXZ,
                }"#]],
    );
}

#[test]
fn h_nyz_lowers() {
    check(
        "H_NYZ 0",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] SingleQubitGate {
                    qubit: 0,
                    gate: H_NYZ,
                }"#]],
    );
}

#[test]
fn h_xy_lowers() {
    check(
        "H_XY 0",
        &expect![[r#"
        Circuit [0-6]:
            items:
                [0-6] SingleQubitGate {
                    qubit: 0,
                    gate: H_XY,
                }"#]],
    );
}

#[test]
fn h_yz_lowers() {
    check(
        "H_YZ 0",
        &expect![[r#"
        Circuit [0-6]:
            items:
                [0-6] SingleQubitGate {
                    qubit: 0,
                    gate: H_YZ,
                }"#]],
    );
}

#[test]
fn s_lowers() {
    check(
        "S 0",
        &expect![[r#"
        Circuit [0-3]:
            items:
                [0-3] SingleQubitGate {
                    qubit: 0,
                    gate: S,
                }"#]],
    );
}

#[test]
fn sqrt_x_lowers() {
    check(
        "SQRT_X 0",
        &expect![[r#"
        Circuit [0-8]:
            items:
                [0-8] SingleQubitGate {
                    qubit: 0,
                    gate: SQRT_X,
                }"#]],
    );
}

#[test]
fn sqrt_x_dag_lowers() {
    check(
        "SQRT_X_DAG 0",
        &expect![[r#"
        Circuit [0-12]:
            items:
                [0-12] SingleQubitGate {
                    qubit: 0,
                    gate: SQRT_X_DAG,
                }"#]],
    );
}

#[test]
fn sqrt_y_lowers() {
    check(
        "SQRT_Y 0",
        &expect![[r#"
        Circuit [0-8]:
            items:
                [0-8] SingleQubitGate {
                    qubit: 0,
                    gate: SQRT_Y,
                }"#]],
    );
}

#[test]
fn sqrt_y_dag_lowers() {
    check(
        "SQRT_Y_DAG 0",
        &expect![[r#"
        Circuit [0-12]:
            items:
                [0-12] SingleQubitGate {
                    qubit: 0,
                    gate: SQRT_Y_DAG,
                }"#]],
    );
}

#[test]
fn s_dag_lowers() {
    check(
        "S_DAG 0",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] SingleQubitGate {
                    qubit: 0,
                    gate: S_DAG,
                }"#]],
    );
}

// Two-qubit gates

#[test]
fn cxswap_lowers() {
    check(
        "CXSWAP 0 1",
        &expect![[r#"
        Circuit [0-10]:
            items:
                [0-10] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: CXSWAP,
                }"#]],
    );
}

#[test]
fn czswap_lowers() {
    check(
        "CZSWAP 0 1",
        &expect![[r#"
        Circuit [0-10]:
            items:
                [0-10] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: CZSWAP,
                }"#]],
    );
}

#[test]
fn ii_lowers() {
    check(
        "II 0 1",
        &expect![[r#"
        Circuit [0-6]:
            items:
                [0-6] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: II,
                }"#]],
    );
}

#[test]
fn iswap_lowers() {
    check(
        "ISWAP 0 1",
        &expect![[r#"
        Circuit [0-9]:
            items:
                [0-9] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: ISWAP,
                }"#]],
    );
}

#[test]
fn iswap_dag_lowers() {
    check(
        "ISWAP_DAG 0 1",
        &expect![[r#"
        Circuit [0-13]:
            items:
                [0-13] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: ISWAP_DAG,
                }"#]],
    );
}

#[test]
fn sqrt_xx_lowers() {
    check(
        "SQRT_XX 0 1",
        &expect![[r#"
        Circuit [0-11]:
            items:
                [0-11] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: SQRT_XX,
                }"#]],
    );
}

#[test]
fn sqrt_xx_dag_lowers() {
    check(
        "SQRT_XX_DAG 0 1",
        &expect![[r#"
        Circuit [0-15]:
            items:
                [0-15] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: SQRT_XX_DAG,
                }"#]],
    );
}

#[test]
fn sqrt_yy_lowers() {
    check(
        "SQRT_YY 0 1",
        &expect![[r#"
        Circuit [0-11]:
            items:
                [0-11] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: SQRT_YY,
                }"#]],
    );
}

#[test]
fn sqrt_yy_dag_lowers() {
    check(
        "SQRT_YY_DAG 0 1",
        &expect![[r#"
        Circuit [0-15]:
            items:
                [0-15] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: SQRT_YY_DAG,
                }"#]],
    );
}

#[test]
fn sqrt_zz_lowers() {
    check(
        "SQRT_ZZ 0 1",
        &expect![[r#"
        Circuit [0-11]:
            items:
                [0-11] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: SQRT_ZZ,
                }"#]],
    );
}

#[test]
fn sqrt_zz_dag_lowers() {
    check(
        "SQRT_ZZ_DAG 0 1",
        &expect![[r#"
        Circuit [0-15]:
            items:
                [0-15] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: SQRT_ZZ_DAG,
                }"#]],
    );
}

#[test]
fn swap_lowers() {
    check(
        "SWAP 0 1",
        &expect![[r#"
        Circuit [0-8]:
            items:
                [0-8] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: SWAP,
                }"#]],
    );
}

#[test]
fn swapcx_lowers() {
    check(
        "SWAPCX 0 1",
        &expect![[r#"
        Circuit [0-10]:
            items:
                [0-10] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: SWAPCX,
                }"#]],
    );
}

#[test]
fn xcx_lowers() {
    check(
        "XCX 0 1",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: XCX,
                }"#]],
    );
}

#[test]
fn xcy_lowers() {
    check(
        "XCY 0 1",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: XCY,
                }"#]],
    );
}

#[test]
fn ycx_lowers() {
    check(
        "YCX 0 1",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: YCX,
                }"#]],
    );
}

#[test]
fn ycy_lowers() {
    check(
        "YCY 0 1",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: YCY,
                }"#]],
    );
}

// Classically controllable gates with qubit targets

#[test]
fn cx_lowers() {
    check(
        "CX 0 1",
        &expect![[r#"
        Circuit [0-6]:
            items:
                [0-6] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: CX,
                }"#]],
    );
}

#[test]
fn cy_lowers() {
    check(
        "CY 0 1",
        &expect![[r#"
        Circuit [0-6]:
            items:
                [0-6] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: CY,
                }"#]],
    );
}

#[test]
fn cz_lowers() {
    check(
        "CZ 0 1",
        &expect![[r#"
        Circuit [0-6]:
            items:
                [0-6] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: CZ,
                }"#]],
    );
}

#[test]
fn xcz_lowers() {
    check(
        "XCZ 0 1",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: XCZ,
                }"#]],
    );
}

#[test]
fn ycz_lowers() {
    check(
        "YCZ 0 1",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: YCZ,
                }"#]],
    );
}

// Classically controllable gates with measurement-record controls

#[test]
fn cx_with_record_control_lowers() {
    let source = indoc! {"
        M 0
        CX rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-17]:
            items:
                [0-3] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }
                [4-16] ClassicallyControlledPauli {
                    control: MeasurementRecord {
                        offset: 1,
                        span: Span {
                            lo: 7,
                            hi: 14,
                        },
                    },
                    target: 1,
                    pauli: X,
                }"#]],
    );
}

#[test]
fn cx_with_mixed_quantum_and_classical_pairs_lowers() {
    let source = indoc! {"
        M 0
        CX rec[-1] 1 2 3
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-21]:
            items:
                [0-3] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }
                [4-20] ClassicallyControlledPauli {
                    control: MeasurementRecord {
                        offset: 1,
                        span: Span {
                            lo: 7,
                            hi: 14,
                        },
                    },
                    target: 1,
                    pauli: X,
                }
                [4-20] TwoQubitGate {
                    q0: 2,
                    q1: 3,
                    gate: CX,
                }"#]],
    );
}

#[test]
fn cy_with_record_control_lowers() {
    let source = indoc! {"
        M 0
        CY rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-17]:
            items:
                [0-3] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }
                [4-16] ClassicallyControlledPauli {
                    control: MeasurementRecord {
                        offset: 1,
                        span: Span {
                            lo: 7,
                            hi: 14,
                        },
                    },
                    target: 1,
                    pauli: Y,
                }"#]],
    );
}

#[test]
fn cz_with_record_control_on_first_target_lowers() {
    let source = indoc! {"
        M 0
        CZ rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-17]:
            items:
                [0-3] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }
                [4-16] ClassicallyControlledPauli {
                    control: MeasurementRecord {
                        offset: 1,
                        span: Span {
                            lo: 7,
                            hi: 14,
                        },
                    },
                    target: 1,
                    pauli: Z,
                }"#]],
    );
}

#[test]
fn cz_with_record_control_on_second_target_lowers() {
    let source = indoc! {"
        M 0
        CZ 1 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-17]:
            items:
                [0-3] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }
                [4-16] ClassicallyControlledPauli {
                    control: MeasurementRecord {
                        offset: 1,
                        span: Span {
                            lo: 9,
                            hi: 16,
                        },
                    },
                    target: 1,
                    pauli: Z,
                }"#]],
    );
}

#[test]
fn xcz_with_record_control_lowers() {
    let source = indoc! {"
        M 0
        XCZ 1 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-18]:
            items:
                [0-3] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }
                [4-17] ClassicallyControlledPauli {
                    control: MeasurementRecord {
                        offset: 1,
                        span: Span {
                            lo: 10,
                            hi: 17,
                        },
                    },
                    target: 1,
                    pauli: X,
                }"#]],
    );
}

#[test]
fn ycz_with_record_control_lowers() {
    let source = indoc! {"
        M 0
        YCZ 1 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-18]:
            items:
                [0-3] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }
                [4-17] ClassicallyControlledPauli {
                    control: MeasurementRecord {
                        offset: 1,
                        span: Span {
                            lo: 10,
                            hi: 17,
                        },
                    },
                    target: 1,
                    pauli: Y,
                }"#]],
    );
}

// Noise

#[test]
fn correlated_error_lowers() {
    check(
        "CORRELATED_ERROR(0.01) X0",
        &expect![[r#"
        Circuit [0-25]:
            items:
                [0-25] Noise(
                    CorrelatedError {
                        kind: Initial,
                        probability: 0.01,
                        faults: [
                            Fault {
                                kind: X,
                                qubit: 0,
                            },
                        ],
                    },
                )"#]],
    );
}

#[test]
fn correlated_error_with_probability_one_lowers() {
    check(
        "CORRELATED_ERROR(1) X0",
        &expect![[r#"
        Circuit [0-22]:
            items:
                [0-22] Noise(
                    CorrelatedError {
                        kind: Initial,
                        probability: 1.0,
                        faults: [
                            Fault {
                                kind: X,
                                qubit: 0,
                            },
                        ],
                    },
                )"#]],
    );
}

#[test]
fn correlated_error_with_loss_lowers() {
    check(
        "CORRELATED_ERROR(0.01) L0",
        &expect![[r#"
        Circuit [0-25]:
            items:
                [0-25] Noise(
                    CorrelatedError {
                        kind: Initial,
                        probability: 0.01,
                        faults: [
                            Fault {
                                kind: Loss,
                                qubit: 0,
                            },
                        ],
                    },
                )"#]],
    );
}

#[test]
fn else_correlated_error_lowers() {
    check(
        "ELSE_CORRELATED_ERROR(0.01) X0",
        &expect![[r#"
        Circuit [0-30]:
            items:
                [0-30] Noise(
                    CorrelatedError {
                        kind: Else,
                        probability: 0.01,
                        faults: [
                            Fault {
                                kind: X,
                                qubit: 0,
                            },
                        ],
                    },
                )"#]],
    );
}

#[test]
fn else_correlated_error_with_loss_lowers() {
    check(
        "ELSE_CORRELATED_ERROR(0.01) L0",
        &expect![[r#"
        Circuit [0-30]:
            items:
                [0-30] Noise(
                    CorrelatedError {
                        kind: Else,
                        probability: 0.01,
                        faults: [
                            Fault {
                                kind: Loss,
                                qubit: 0,
                            },
                        ],
                    },
                )"#]],
    );
}

#[test]
fn depolarize1_lowers() {
    check(
        "DEPOLARIZE1(0.01) 0",
        &expect![[r#"
        Circuit [0-19]:
            items:
                [0-19] Noise(
                    SingleQubitNoise {
                        kind: Depolarize,
                        probability: 0.01,
                        qubit: 0,
                    },
                )"#]],
    );
}

#[test]
fn depolarize2_lowers() {
    check(
        "DEPOLARIZE2(0.01) 0 1",
        &expect![[r#"
        Circuit [0-21]:
            items:
                [0-21] Noise(
                    Depolarize2 {
                        probability: 0.01,
                        q0: 0,
                        q1: 1,
                    },
                )"#]],
    );
}

#[test]
fn heralded_erase_lowers() {
    check(
        "HERALDED_ERASE(0.01) 0",
        &expect![[r#"
        Circuit [0-22]:
            items:
                [0-22] Noise(
                    HeraldedErase {
                        probability: 0.01,
                        qubit: 0,
                    },
                )"#]],
    );
}

#[test]
fn heralded_pauli_channel_1_lowers() {
    check(
        "HERALDED_PAULI_CHANNEL_1(0.01,0,0,0) 0",
        &expect![[r#"
        Circuit [0-38]:
            items:
                [0-38] Noise(
                    HeraldedPauliChannel1 {
                        probabilities: [
                            0.01,
                            0.0,
                            0.0,
                            0.0,
                        ],
                        qubit: 0,
                    },
                )"#]],
    );
}

#[test]
fn pauli_channel_1_lowers() {
    check(
        "PAULI_CHANNEL_1(0.01,0,0) 0",
        &expect![[r#"
        Circuit [0-27]:
            items:
                [0-27] Noise(
                    PauliChannel1 {
                        probabilities: [
                            0.01,
                            0.0,
                            0.0,
                        ],
                        qubit: 0,
                    },
                )"#]],
    );
}

#[test]
fn pauli_channel_1_with_probabilities_summing_to_one_lowers() {
    check(
        "PAULI_CHANNEL_1(0.2,0.3,0.5) 0",
        &expect![[r#"
        Circuit [0-30]:
            items:
                [0-30] Noise(
                    PauliChannel1 {
                        probabilities: [
                            0.2,
                            0.3,
                            0.5,
                        ],
                        qubit: 0,
                    },
                )"#]],
    );
}

#[test]
fn identity_errors_with_valid_probability_lists_lower() {
    // I_ERROR and II_ERROR are just for validation, so they don't produce
    // anything in the semantic AST
    let source = indoc! {"
        I_ERROR 0
        I_ERROR(0.2,0.3,0.5) 1
        II_ERROR 2 3
        II_ERROR(0.2,0.3,0.5) 4 5
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-72]:
            items: <empty>"#]],
    );
}

#[test]
fn pauli_channel_2_lowers() {
    check(
        "PAULI_CHANNEL_2(0.01,0,0,0,0,0,0,0,0,0,0,0,0,0,0) 0 1",
        &expect![[r#"
            Circuit [0-53]:
                items:
                    [0-53] Noise(
                        PauliChannel2 {
                            probabilities: [
                                0.01,
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                            ],
                            q0: 0,
                            q1: 1,
                        },
                    )"#]],
    );
}

#[test]
fn x_error_lowers() {
    check(
        "X_ERROR(0.01) 0",
        &expect![[r#"
        Circuit [0-15]:
            items:
                [0-15] Noise(
                    SingleQubitNoise {
                        kind: Fault(
                            X,
                        ),
                        probability: 0.01,
                        qubit: 0,
                    },
                )"#]],
    );
}

#[test]
fn y_error_lowers() {
    check(
        "Y_ERROR(0.01) 0",
        &expect![[r#"
        Circuit [0-15]:
            items:
                [0-15] Noise(
                    SingleQubitNoise {
                        kind: Fault(
                            Y,
                        ),
                        probability: 0.01,
                        qubit: 0,
                    },
                )"#]],
    );
}

#[test]
fn z_error_lowers() {
    check(
        "Z_ERROR(0.01) 0",
        &expect![[r#"
        Circuit [0-15]:
            items:
                [0-15] Noise(
                    SingleQubitNoise {
                        kind: Fault(
                            Z,
                        ),
                        probability: 0.01,
                        qubit: 0,
                    },
                )"#]],
    );
}

#[test]
fn loss_error_lowers() {
    check(
        "LOSS_ERROR(0.01) 0",
        &expect![[r#"
        Circuit [0-18]:
            items:
                [0-18] Noise(
                    SingleQubitNoise {
                        kind: Fault(
                            Loss,
                        ),
                        probability: 0.01,
                        qubit: 0,
                    },
                )"#]],
    );
}

// Single-qubit measurements

#[test]
fn m_lowers() {
    check(
        "M 0",
        &expect![[r#"
        Circuit [0-3]:
            items:
                [0-3] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn m_with_readout_noise_lowers() {
    check(
        "M(1) 0",
        &expect![[r#"
        Circuit [0-6]:
            items:
                [0-6] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 1.0,
                    negated: false,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn m_with_negated_target_lowers() {
    check(
        "M !0",
        &expect![[r#"
        Circuit [0-4]:
            items:
                [0-4] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: true,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn m_with_readout_noise_and_negated_target_lowers() {
    check(
        "M(0.1) !0",
        &expect![[r#"
        Circuit [0-9]:
            items:
                [0-9] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 0.1,
                    negated: true,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn mr_lowers() {
    check(
        "MR 0",
        &expect![[r#"
        Circuit [0-4]:
            items:
                [0-4] SingleQubitMeasurement {
                    reset: true,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn mr_with_readout_noise_lowers() {
    check(
        "MR(0.1) 0",
        &expect![[r#"
        Circuit [0-9]:
            items:
                [0-9] SingleQubitMeasurement {
                    reset: true,
                    observable: Z,
                    readout_noise: 0.1,
                    negated: false,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn mr_with_negated_target_lowers() {
    check(
        "MR !0",
        &expect![[r#"
        Circuit [0-5]:
            items:
                [0-5] SingleQubitMeasurement {
                    reset: true,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: true,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn mrx_lowers() {
    check(
        "MRX 0",
        &expect![[r#"
        Circuit [0-5]:
            items:
                [0-5] SingleQubitMeasurement {
                    reset: true,
                    observable: X,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn mrx_with_readout_noise_lowers() {
    check(
        "MRX(0.1) 0",
        &expect![[r#"
        Circuit [0-10]:
            items:
                [0-10] SingleQubitMeasurement {
                    reset: true,
                    observable: X,
                    readout_noise: 0.1,
                    negated: false,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn mrx_with_negated_target_lowers() {
    check(
        "MRX !0",
        &expect![[r#"
        Circuit [0-6]:
            items:
                [0-6] SingleQubitMeasurement {
                    reset: true,
                    observable: X,
                    readout_noise: 0.0,
                    negated: true,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn mry_lowers() {
    check(
        "MRY 0",
        &expect![[r#"
        Circuit [0-5]:
            items:
                [0-5] SingleQubitMeasurement {
                    reset: true,
                    observable: Y,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn mry_with_readout_noise_lowers() {
    check(
        "MRY(0.1) 0",
        &expect![[r#"
        Circuit [0-10]:
            items:
                [0-10] SingleQubitMeasurement {
                    reset: true,
                    observable: Y,
                    readout_noise: 0.1,
                    negated: false,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn mry_with_negated_target_lowers() {
    check(
        "MRY !0",
        &expect![[r#"
        Circuit [0-6]:
            items:
                [0-6] SingleQubitMeasurement {
                    reset: true,
                    observable: Y,
                    readout_noise: 0.0,
                    negated: true,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn mx_lowers() {
    check(
        "MX 0",
        &expect![[r#"
        Circuit [0-4]:
            items:
                [0-4] SingleQubitMeasurement {
                    reset: false,
                    observable: X,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn mx_with_readout_noise_lowers() {
    check(
        "MX(0.1) 0",
        &expect![[r#"
        Circuit [0-9]:
            items:
                [0-9] SingleQubitMeasurement {
                    reset: false,
                    observable: X,
                    readout_noise: 0.1,
                    negated: false,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn mx_with_negated_target_lowers() {
    check(
        "MX !0",
        &expect![[r#"
        Circuit [0-5]:
            items:
                [0-5] SingleQubitMeasurement {
                    reset: false,
                    observable: X,
                    readout_noise: 0.0,
                    negated: true,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn my_lowers() {
    check(
        "MY 0",
        &expect![[r#"
        Circuit [0-4]:
            items:
                [0-4] SingleQubitMeasurement {
                    reset: false,
                    observable: Y,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn my_with_readout_noise_lowers() {
    check(
        "MY(0.1) 0",
        &expect![[r#"
        Circuit [0-9]:
            items:
                [0-9] SingleQubitMeasurement {
                    reset: false,
                    observable: Y,
                    readout_noise: 0.1,
                    negated: false,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn my_with_negated_target_lowers() {
    check(
        "MY !0",
        &expect![[r#"
        Circuit [0-5]:
            items:
                [0-5] SingleQubitMeasurement {
                    reset: false,
                    observable: Y,
                    readout_noise: 0.0,
                    negated: true,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn r_lowers() {
    check(
        "R 0",
        &expect![[r#"
        Circuit [0-3]:
            items:
                [0-3] Reset {
                    qubit: 0,
                    basis: Z,
                }"#]],
    );
}

#[test]
fn rx_lowers() {
    check(
        "RX 0",
        &expect![[r#"
        Circuit [0-4]:
            items:
                [0-4] Reset {
                    qubit: 0,
                    basis: X,
                }"#]],
    );
}

#[test]
fn ry_lowers() {
    check(
        "RY 0",
        &expect![[r#"
        Circuit [0-4]:
            items:
                [0-4] Reset {
                    qubit: 0,
                    basis: Y,
                }"#]],
    );
}

// Two-qubit measurements

#[test]
fn mxx_lowers() {
    check(
        "MXX 0 1",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] TwoQubitMeasurement {
                    readout_noise: 0.0,
                    observable: XX,
                    negated: false,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

#[test]
fn mxx_with_readout_noise_lowers() {
    check(
        "MXX(0.1) 0 1",
        &expect![[r#"
        Circuit [0-12]:
            items:
                [0-12] TwoQubitMeasurement {
                    readout_noise: 0.1,
                    observable: XX,
                    negated: false,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

#[test]
fn mxx_with_negated_target_lowers() {
    check(
        "MXX !0 1",
        &expect![[r#"
        Circuit [0-8]:
            items:
                [0-8] TwoQubitMeasurement {
                    readout_noise: 0.0,
                    observable: XX,
                    negated: true,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

#[test]
fn myy_lowers() {
    check(
        "MYY 0 1",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] TwoQubitMeasurement {
                    readout_noise: 0.0,
                    observable: YY,
                    negated: false,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

#[test]
fn myy_with_readout_noise_lowers() {
    check(
        "MYY(0.1) 0 1",
        &expect![[r#"
        Circuit [0-12]:
            items:
                [0-12] TwoQubitMeasurement {
                    readout_noise: 0.1,
                    observable: YY,
                    negated: false,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

#[test]
fn myy_with_negated_target_lowers() {
    check(
        "MYY !0 1",
        &expect![[r#"
        Circuit [0-8]:
            items:
                [0-8] TwoQubitMeasurement {
                    readout_noise: 0.0,
                    observable: YY,
                    negated: true,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

#[test]
fn mzz_lowers() {
    check(
        "MZZ 0 1",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] TwoQubitMeasurement {
                    readout_noise: 0.0,
                    observable: ZZ,
                    negated: false,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

#[test]
fn mzz_with_readout_noise_lowers() {
    check(
        "MZZ(0.1) 0 1",
        &expect![[r#"
        Circuit [0-12]:
            items:
                [0-12] TwoQubitMeasurement {
                    readout_noise: 0.1,
                    observable: ZZ,
                    negated: false,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

#[test]
fn mzz_with_negated_target_lowers() {
    check(
        "MZZ !0 1",
        &expect![[r#"
        Circuit [0-8]:
            items:
                [0-8] TwoQubitMeasurement {
                    readout_noise: 0.0,
                    observable: ZZ,
                    negated: true,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

// Generalized Pauli product gates

#[test]
fn mpp_lowers() {
    check(
        "MPP X0*Y1",
        &expect![[r#"
        Circuit [0-9]:
            items:
                [0-9] PauliProductMeasurement {
                    readout_noise: 0.0,
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: false,
                    },
                }"#]],
    );
}

#[test]
fn mpp_with_readout_noise_lowers() {
    check(
        "MPP(0.1) X0*Y1",
        &expect![[r#"
        Circuit [0-14]:
            items:
                [0-14] PauliProductMeasurement {
                    readout_noise: 0.1,
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: false,
                    },
                }"#]],
    );
}

#[test]
fn mpp_with_negated_target_lowers() {
    check(
        "MPP !X0*Y1",
        &expect![[r#"
        Circuit [0-10]:
            items:
                [0-10] PauliProductMeasurement {
                    readout_noise: 0.0,
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: true,
                    },
                }"#]],
    );
}

#[test]
fn spp_lowers() {
    check(
        "SPP X0*Y1",
        &expect![[r#"
        Circuit [0-9]:
            items:
                [0-9] PauliProductGate {
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: false,
                    },
                    gate: S,
                }"#]],
    );
}

#[test]
fn spp_with_negated_target_lowers() {
    check(
        "SPP X0*!Y1",
        &expect![[r#"
        Circuit [0-10]:
            items:
                [0-10] PauliProductGate {
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: true,
                    },
                    gate: S,
                }"#]],
    );
}

#[test]
fn spp_dag_lowers() {
    check(
        "SPP_DAG X0*Y1",
        &expect![[r#"
        Circuit [0-13]:
            items:
                [0-13] PauliProductGate {
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: false,
                    },
                    gate: S_DAG,
                }"#]],
    );
}

#[test]
fn spp_dag_with_negated_target_lowers() {
    check(
        "SPP_DAG !X0*Y1",
        &expect![[r#"
        Circuit [0-14]:
            items:
                [0-14] PauliProductGate {
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: true,
                    },
                    gate: S_DAG,
                }"#]],
    );
}

// Control flow

#[test]
fn require_lowers() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-35]:
            items:
                SelectBlock:
                    body:
                        [11-14] SingleQubitMeasurement {
                            reset: false,
                            observable: Z,
                            readout_noise: 0.0,
                            negated: false,
                            qubit: 0,
                        }
                        [17-32] Require {
                            records: [
                                NegatableMeasurementRecord {
                                    record: MeasurementRecord {
                                        offset: 1,
                                        span: Span {
                                            lo: 25,
                                            hi: 32,
                                        },
                                    },
                                    negated: false,
                                },
                            ],
                        }"#]],
    );
}

#[test]
fn require_with_negated_record_targets_lowers() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE !rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-36]:
            items:
                SelectBlock:
                    body:
                        [11-14] SingleQubitMeasurement {
                            reset: false,
                            observable: Z,
                            readout_noise: 0.0,
                            negated: false,
                            qubit: 0,
                        }
                        [17-33] Require {
                            records: [
                                NegatableMeasurementRecord {
                                    record: MeasurementRecord {
                                        offset: 1,
                                        span: Span {
                                            lo: 25,
                                            hi: 33,
                                        },
                                    },
                                    negated: true,
                                },
                            ],
                        }"#]],
    );
}

#[test]
fn notleaked_lowers() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-37]:
            items:
                SelectBlock:
                    body:
                        [11-14] SingleQubitMeasurement {
                            reset: false,
                            observable: Z,
                            readout_noise: 0.0,
                            negated: false,
                            qubit: 0,
                        }
                        [17-34] NotLeaked {
                            records: [
                                MeasurementRecord {
                                    offset: 1,
                                    span: Span {
                                        lo: 27,
                                        hi: 34,
                                    },
                                },
                            ],
                        }"#]],
    );
}

// Miscellaneous

#[test]
fn peek_loss_lowers() {
    check(
        "PEEK_LOSS 0",
        &expect![[r#"
        Circuit [0-11]:
            items:
                [0-11] PeekLoss {
                    readout_noise: 0.0,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn peek_loss_with_readout_noise_lowers() {
    check(
        "PEEK_LOSS(0.1) 0",
        &expect![[r#"
        Circuit [0-16]:
            items:
                [0-16] PeekLoss {
                    readout_noise: 0.1,
                    qubit: 0,
                }"#]],
    );
}

// Annotations

#[test]
fn detector_lowers() {
    let source = indoc! {"
        M 0
        DETECTOR rec[-1]
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-21]:
            items:
                [0-3] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }
                [4-20] Annotation(
                    Detector {
                        coordinates: [],
                        records: [
                            MeasurementRecord {
                                offset: 1,
                                span: Span {
                                    lo: 13,
                                    hi: 20,
                                },
                            },
                        ],
                    },
                )"#]],
    );
}

#[test]
fn detector_with_coordinates_lowers() {
    check(
        "DETECTOR(1,2,3)",
        &expect![[r#"
        Circuit [0-15]:
            items:
                [0-15] Annotation(
                    Detector {
                        coordinates: [
                            1.0,
                            2.0,
                            3.0,
                        ],
                        records: [],
                    },
                )"#]],
    );
}

#[test]
fn detector_with_sixteen_coordinates_lowers() {
    check(
        "DETECTOR(0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15)",
        &expect![[r#"
            Circuit [0-47]:
                items:
                    [0-47] Annotation(
                        Detector {
                            coordinates: [
                                0.0,
                                1.0,
                                2.0,
                                3.0,
                                4.0,
                                5.0,
                                6.0,
                                7.0,
                                8.0,
                                9.0,
                                10.0,
                                11.0,
                                12.0,
                                13.0,
                                14.0,
                                15.0,
                            ],
                            records: [],
                        },
                    )"#]],
    );
}

#[test]
fn mpad_lowers() {
    check(
        "MPAD 0",
        &expect![[r#"
        Circuit [0-6]:
            items:
                [0-6] Annotation(
                    MeasurementPadding {
                        readout_noise: 0.0,
                        value: false,
                    },
                )"#]],
    );
}

#[test]
fn mpad_with_readout_noise_lowers() {
    check(
        "MPAD(0.1) 0",
        &expect![[r#"
            Circuit [0-11]:
                items:
                    [0-11] Annotation(
                        MeasurementPadding {
                            readout_noise: 0.1,
                            value: false,
                        },
                    )"#]],
    );
}

#[test]
fn observable_include_lowers() {
    let source = indoc! {"
        M 0
        OBSERVABLE_INCLUDE(0) rec[-1] X1
    "};
    check(
        source,
        &expect![[r#"
        Circuit [0-37]:
            items:
                [0-3] SingleQubitMeasurement {
                    reset: false,
                    observable: Z,
                    readout_noise: 0.0,
                    negated: false,
                    qubit: 0,
                }
                [4-36] Annotation(
                    ObservableInclude {
                        logical_observable: 0,
                        targets: [
                            Record(
                                MeasurementRecord {
                                    offset: 1,
                                    span: Span {
                                        lo: 26,
                                        hi: 33,
                                    },
                                },
                            ),
                            Pauli(
                                PauliFactor {
                                    pauli: X,
                                    qubit: 1,
                                },
                            ),
                        ],
                    },
                )"#]],
    );
}

#[test]
fn observable_include_with_max_index_lowers() {
    check(
        "OBSERVABLE_INCLUDE(4294967295) X0",
        &expect![[r#"
        Circuit [0-33]:
            items:
                [0-33] Annotation(
                    ObservableInclude {
                        logical_observable: 4294967295,
                        targets: [
                            Pauli(
                                PauliFactor {
                                    pauli: X,
                                    qubit: 0,
                                },
                            ),
                        ],
                    },
                )"#]],
    );
}

#[test]
fn qubit_coords_lowers() {
    check(
        "QUBIT_COORDS(1,2) 0",
        &expect![[r#"
        Circuit [0-19]:
            items:
                [0-19] Annotation(
                    QubitCoordinates {
                        coordinates: [
                            1.0,
                            2.0,
                        ],
                        qubit: 0,
                    },
                )"#]],
    );
}

#[test]
fn shift_coords_lowers() {
    check(
        "SHIFT_COORDS(1,2)",
        &expect![[r#"
        Circuit [0-17]:
            items:
                [0-17] Annotation(
                    ShiftCoordinates {
                        offsets: [
                            1.0,
                            2.0,
                        ],
                    },
                )"#]],
    );
}

#[test]
fn tick_lowers() {
    check(
        "TICK",
        &expect![[r#"
        Circuit [0-4]:
            items:
                [0-4] Annotation(
                    Tick,
                )"#]],
    );
}

// Non-Clifford gates

#[test]
fn t_lowers() {
    check(
        "T 0",
        &expect![[r#"
        Circuit [0-3]:
            items:
                [0-3] SingleQubitGate {
                    qubit: 0,
                    gate: T,
                }"#]],
    );
}

#[test]
fn t_dag_lowers() {
    check(
        "T_DAG 0",
        &expect![[r#"
        Circuit [0-7]:
            items:
                [0-7] SingleQubitGate {
                    qubit: 0,
                    gate: T_DAG,
                }"#]],
    );
}

#[test]
fn tpp_lowers() {
    check(
        "TPP X0*Y1",
        &expect![[r#"
        Circuit [0-9]:
            items:
                [0-9] PauliProductGate {
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: false,
                    },
                    gate: T,
                }"#]],
    );
}

#[test]
fn tpp_with_negated_target_lowers() {
    check(
        "TPP !X0*Y1",
        &expect![[r#"
        Circuit [0-10]:
            items:
                [0-10] PauliProductGate {
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: true,
                    },
                    gate: T,
                }"#]],
    );
}

#[test]
fn tpp_dag_lowers() {
    check(
        "TPP_DAG X0*Y1",
        &expect![[r#"
        Circuit [0-13]:
            items:
                [0-13] PauliProductGate {
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: false,
                    },
                    gate: T_DAG,
                }"#]],
    );
}

#[test]
fn tpp_dag_with_negated_target_lowers() {
    check(
        "TPP_DAG !X0*Y1",
        &expect![[r#"
        Circuit [0-14]:
            items:
                [0-14] PauliProductGate {
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: true,
                    },
                    gate: T_DAG,
                }"#]],
    );
}

#[test]
fn ch_lowers() {
    check(
        "CH 0 1",
        &expect![[r#"
        Circuit [0-6]:
            items:
                [0-6] TwoQubitGate {
                    q0: 0,
                    q1: 1,
                    gate: CH,
                }"#]],
    );
}

#[test]
fn ccz_lowers() {
    check(
        "CCZ 0 1 2",
        &expect![[r#"
        Circuit [0-9]:
            items:
                [0-9] ThreeQubitGate {
                    q0: 0,
                    q1: 1,
                    q2: 2,
                    gate: CCZ,
                }"#]],
    );
}

#[test]
fn ccx_lowers() {
    check(
        "CCX 0 1 2",
        &expect![[r#"
        Circuit [0-9]:
            items:
                [0-9] ThreeQubitGate {
                    q0: 0,
                    q1: 1,
                    q2: 2,
                    gate: CCX,
                }"#]],
    );
}

// Single-qubit rotations

#[test]
fn r_x_lowers() {
    check(
        "R_X(0.25) 0",
        &expect![[r#"
        Circuit [0-11]:
            items:
                [0-11] SingleQubitRotation {
                    axis: X,
                    angle: 0.7853981633974483,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn r_x_with_angle_in_radians_lowers() {
    check(
        "R_X(1rad) 0",
        &expect![[r#"
        Circuit [0-11]:
            items:
                [0-11] SingleQubitRotation {
                    axis: X,
                    angle: 1.0,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn r_y_lowers() {
    check(
        "R_Y(0.25) 0",
        &expect![[r#"
        Circuit [0-11]:
            items:
                [0-11] SingleQubitRotation {
                    axis: Y,
                    angle: 0.7853981633974483,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn r_z_lowers() {
    check(
        "R_Z(0.25) 0",
        &expect![[r#"
        Circuit [0-11]:
            items:
                [0-11] SingleQubitRotation {
                    axis: Z,
                    angle: 0.7853981633974483,
                    qubit: 0,
                }"#]],
    );
}

// U3

#[test]
fn u3_lowers() {
    check(
        "U3(0.1,0.2,0.3) 0",
        &expect![[r#"
        Circuit [0-17]:
            items:
                [0-17] U3 {
                    theta: 0.3141592653589793,
                    phi: 0.6283185307179586,
                    lambda: 0.9424777960769379,
                    qubit: 0,
                }"#]],
    );
}

#[test]
fn u3_with_mixed_angle_units_lowers() {
    check(
        "U3(0.1,-0.2rad,0.3rad) 0",
        &expect![[r#"
        Circuit [0-24]:
            items:
                [0-24] U3 {
                    theta: 0.3141592653589793,
                    phi: -0.2,
                    lambda: 0.3,
                    qubit: 0,
                }"#]],
    );
}

// Two-qubit rotations

#[test]
fn r_xx_lowers() {
    check(
        "R_XX(0.25) 0 1",
        &expect![[r#"
        Circuit [0-14]:
            items:
                [0-14] TwoQubitRotation {
                    axis: XX,
                    angle: 0.7853981633974483,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

#[test]
fn r_xx_with_angle_in_radians_lowers() {
    check(
        "R_XX(1rad) 0 1",
        &expect![[r#"
        Circuit [0-14]:
            items:
                [0-14] TwoQubitRotation {
                    axis: XX,
                    angle: 1.0,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

#[test]
fn r_yy_lowers() {
    check(
        "R_YY(0.25) 0 1",
        &expect![[r#"
        Circuit [0-14]:
            items:
                [0-14] TwoQubitRotation {
                    axis: YY,
                    angle: 0.7853981633974483,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

#[test]
fn r_zz_lowers() {
    check(
        "R_ZZ(0.25) 0 1",
        &expect![[r#"
        Circuit [0-14]:
            items:
                [0-14] TwoQubitRotation {
                    axis: ZZ,
                    angle: 0.7853981633974483,
                    q0: 0,
                    q1: 1,
                }"#]],
    );
}

// Pauli product rotations

#[test]
fn r_pauli_lowers() {
    check(
        "R_PAULI(0.25) X0*Y1",
        &expect![[r#"
        Circuit [0-19]:
            items:
                [0-19] PauliProductRotation {
                    angle: 0.7853981633974483,
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: false,
                    },
                }"#]],
    );
}

#[test]
fn r_pauli_with_angle_in_radians_lowers() {
    check(
        "R_PAULI(1rad) X0*Y1",
        &expect![[r#"
        Circuit [0-19]:
            items:
                [0-19] PauliProductRotation {
                    angle: 1.0,
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: false,
                    },
                }"#]],
    );
}

#[test]
fn r_pauli_with_negated_target_lowers() {
    check(
        "R_PAULI(0.25) !X0*Y1",
        &expect![[r#"
        Circuit [0-20]:
            items:
                [0-20] PauliProductRotation {
                    angle: 0.7853981633974483,
                    product: PauliProduct {
                        factors: [
                            PauliFactor {
                                pauli: X,
                                qubit: 0,
                            },
                            PauliFactor {
                                pauli: Y,
                                qubit: 1,
                            },
                        ],
                        negated: true,
                    },
                }"#]],
    );
}
