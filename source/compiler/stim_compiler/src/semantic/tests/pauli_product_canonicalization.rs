// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn single_pauli_target_becomes_single_factor_product() {
    check(
        "TPP X0",
        &expect![[r#"
            Circuit [0-6]:
                items:
                    Instruction [0-6]:
                        name_span: [0-3]
                        kind: PauliProductGate {
                            product: PauliProduct {
                                factors: [
                                    PauliFactor {
                                        pauli: X,
                                        q: 0 [4-6],
                                    },
                                ],
                                negated: false,
                            },
                            gate: T,
                        }"#]],
    );
}

#[test]
fn factors_are_sorted_by_qubit() {
    check(
        "R_PAULI(0.25) Z2*X0*Y1",
        &expect![[r#"
            Circuit [0-22]:
                items:
                    Instruction [0-22]:
                        name_span: [0-7]
                        kind: PauliProductRotation {
                            angle: 0.7853981633974483 [8-12],
                            product: PauliProduct {
                                factors: [
                                    PauliFactor {
                                        pauli: X,
                                        q: 0 [17-19],
                                    },
                                    PauliFactor {
                                        pauli: Y,
                                        q: 1 [20-22],
                                    },
                                    PauliFactor {
                                        pauli: Z,
                                        q: 2 [14-16],
                                    },
                                ],
                                negated: false,
                            },
                        }"#]],
    );
}

#[test]
fn negation_on_later_factor_negates_product() {
    check(
        "SPP X0*!Y1",
        &expect![[r#"
            Circuit [0-10]:
                items:
                    Instruction [0-10]:
                        name_span: [0-3]
                        kind: PauliProductGate {
                            product: PauliProduct {
                                factors: [
                                    PauliFactor {
                                        pauli: X,
                                        q: 0 [4-6],
                                    },
                                    PauliFactor {
                                        pauli: Y,
                                        q: 1 [7-10],
                                    },
                                ],
                                negated: true,
                            },
                            gate: S,
                        }"#]],
    );
}

#[test]
fn double_negation_cancels() {
    check(
        "SPP_DAG !X0*!Y1",
        &expect![[r#"
            Circuit [0-15]:
                items:
                    Instruction [0-15]:
                        name_span: [0-7]
                        kind: PauliProductGate {
                            product: PauliProduct {
                                factors: [
                                    PauliFactor {
                                        pauli: X,
                                        q: 0 [8-11],
                                    },
                                    PauliFactor {
                                        pauli: Y,
                                        q: 1 [12-15],
                                    },
                                ],
                                negated: false,
                            },
                            gate: S_DAG,
                        }"#]],
    );
}

#[test]
fn repeated_identical_factors_fold_to_single_factor() {
    check(
        "TPP_DAG X0*X0*X0",
        &expect![[r#"
            Circuit [0-16]:
                items:
                    Instruction [0-16]:
                        name_span: [0-7]
                        kind: PauliProductGate {
                            product: PauliProduct {
                                factors: [
                                    PauliFactor {
                                        pauli: X,
                                        q: 0 [8-10],
                                    },
                                ],
                                negated: false,
                            },
                            gate: T_DAG,
                        }"#]],
    );
}

#[test]
fn folded_phase_negates_product() {
    check(
        "MPP X0*Y0*X1*Y1",
        &expect![[r#"
            Circuit [0-15]:
                items:
                    Instruction [0-15]:
                        name_span: [0-3]
                        kind: PauliProductMeasurement {
                            readout_noise: 0.0 [0-0],
                            product: PauliProduct {
                                factors: [
                                    PauliFactor {
                                        pauli: Z,
                                        q: 0 [4-6],
                                    },
                                    PauliFactor {
                                        pauli: Z,
                                        q: 1 [10-12],
                                    },
                                ],
                                negated: true,
                            },
                        }"#]],
    );
}

#[test]
fn explicit_negation_cancels_folded_negative_phase() {
    check(
        "TPP !X0*Y0*X1*Y1",
        &expect![[r#"
            Circuit [0-16]:
                items:
                    Instruction [0-16]:
                        name_span: [0-3]
                        kind: PauliProductGate {
                            product: PauliProduct {
                                factors: [
                                    PauliFactor {
                                        pauli: Z,
                                        q: 0 [4-7],
                                    },
                                    PauliFactor {
                                        pauli: Z,
                                        q: 1 [11-13],
                                    },
                                ],
                                negated: false,
                            },
                            gate: T,
                        }"#]],
    );
}

#[test]
fn non_adjacent_repeated_qubits_fold_together() {
    check(
        "R_PAULI(0.25) X0*Z1*Z0*X1",
        &expect![[r#"
            Circuit [0-25]:
                items:
                    Instruction [0-25]:
                        name_span: [0-7]
                        kind: PauliProductRotation {
                            angle: 0.7853981633974483 [8-12],
                            product: PauliProduct {
                                factors: [
                                    PauliFactor {
                                        pauli: Y,
                                        q: 0 [14-16],
                                    },
                                    PauliFactor {
                                        pauli: Y,
                                        q: 1 [17-19],
                                    },
                                ],
                                negated: false,
                            },
                        }"#]],
    );
}

#[test]
fn factors_folding_to_identity_are_dropped() {
    check(
        "SPP_DAG Y0*Y0*Z1*Z1*X2",
        &expect![[r#"
            Circuit [0-22]:
                items:
                    Instruction [0-22]:
                        name_span: [0-7]
                        kind: PauliProductGate {
                            product: PauliProduct {
                                factors: [
                                    PauliFactor {
                                        pauli: X,
                                        q: 2 [20-22],
                                    },
                                ],
                                negated: false,
                            },
                            gate: S_DAG,
                        }"#]],
    );
}

#[test]
fn multiple_products_are_canonicalized_independently() {
    check(
        "TPP Z2*X0 X3*!Y1",
        &expect![[r#"
            Circuit [0-16]:
                items:
                    Instruction [0-16]:
                        name_span: [0-3]
                        kind: PauliProductGate {
                            product: PauliProduct {
                                factors: [
                                    PauliFactor {
                                        pauli: X,
                                        q: 0 [7-9],
                                    },
                                    PauliFactor {
                                        pauli: Z,
                                        q: 2 [4-6],
                                    },
                                ],
                                negated: false,
                            },
                            gate: T,
                        }
                    Instruction [0-16]:
                        name_span: [0-3]
                        kind: PauliProductGate {
                            product: PauliProduct {
                                factors: [
                                    PauliFactor {
                                        pauli: Y,
                                        q: 1 [13-16],
                                    },
                                    PauliFactor {
                                        pauli: X,
                                        q: 3 [10-12],
                                    },
                                ],
                                negated: true,
                            },
                            gate: T,
                        }"#]],
    );
}

#[test]
fn identity_measurement_product_is_rejected() {
    check(
        "MPP X0*X0",
        &expect![[r#"
        Qdk.Stim.Semantic.UnsupportedTarget

          x unsupported target in instruction: MPP
           ,----
         1 | MPP X0*X0
           :     ^^^^^
           `----
    "#]],
    );
}

#[test]
fn identity_gate_products_become_empty_products() {
    check(
        "SPP X0*X0 !Y1*Y1",
        &expect![[r#"
            Circuit [0-16]:
                items:
                    Instruction [0-16]:
                        name_span: [0-3]
                        kind: PauliProductGate {
                            product: PauliProduct {
                                factors: [],
                                negated: false,
                            },
                            gate: S,
                        }
                    Instruction [0-16]:
                        name_span: [0-3]
                        kind: PauliProductGate {
                            product: PauliProduct {
                                factors: [],
                                negated: true,
                            },
                            gate: S,
                        }"#]],
    );
}

#[test]
fn anti_hermitian_product_is_rejected() {
    check(
        "TPP_DAG X0*Y0",
        &expect![[r#"
        Qdk.Stim.Semantic.AntiHermitianPauliProduct

          x Pauli product must be Hermitian
           ,----
         1 | TPP_DAG X0*Y0
           :         ^^^^^
           `----
    "#]],
    );
}
