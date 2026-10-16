// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    parser,
    semantic::{self, Annotation, InstructionKind, Item, Noise},
};

fn lower(source: &str) -> Vec<InstructionKind> {
    let (parsed, parser_errors) = parser::parse(source);
    assert!(parser_errors.is_empty(), "{parser_errors:#?}");

    let (circuit, semantic_errors) = semantic::lower(parsed);
    assert!(semantic_errors.is_empty(), "{semantic_errors:#?}");
    circuit
        .items
        .into_iter()
        .map(|item| {
            let Item::Instruction(instruction) = item else {
                panic!("{source} should produce instructions");
            };
            instruction.kind
        })
        .collect()
}

#[test]
fn single_qubit_gates_broadcast_over_targets() {
    let gates = [
        "I",
        "X",
        "Y",
        "Z",
        "C_NXYZ",
        "C_NZYX",
        "C_XNYZ",
        "C_XYNZ",
        "C_XYZ",
        "C_ZNYX",
        "C_ZYNX",
        "C_ZYX",
        "H",
        "H_NXY",
        "H_NXZ",
        "H_NYZ",
        "H_XY",
        "H_YZ",
        "S",
        "SQRT_X",
        "SQRT_X_DAG",
        "SQRT_Y",
        "SQRT_Y_DAG",
        "S_DAG",
        "T",
        "T_DAG",
    ];

    for gate in gates {
        let source = format!("{gate} 2 7 13");
        let qubits = lower(&source)
            .into_iter()
            .map(|kind| {
                let InstructionKind::SingleQubitGate { qubit, .. } = kind else {
                    panic!("{gate} should produce single-qubit gates");
                };
                qubit
            })
            .collect::<Vec<_>>();

        assert_eq!(qubits, [2, 7, 13], "{gate} broadcast incorrect targets");
    }
}

#[test]
fn two_qubit_gates_broadcast_over_target_pairs() {
    let gates = [
        "CX",
        "CXSWAP",
        "CY",
        "CZ",
        "CZSWAP",
        "II",
        "ISWAP",
        "ISWAP_DAG",
        "SQRT_XX",
        "SQRT_XX_DAG",
        "SQRT_YY",
        "SQRT_YY_DAG",
        "SQRT_ZZ",
        "SQRT_ZZ_DAG",
        "SWAP",
        "SWAPCX",
        "XCX",
        "XCY",
        "XCZ",
        "YCX",
        "YCY",
        "YCZ",
        "CH",
    ];

    for gate in gates {
        let source = format!("{gate} 2 7 13 19");
        let pairs = lower(&source)
            .into_iter()
            .map(|kind| {
                let InstructionKind::TwoQubitGate { q0, q1, .. } = kind else {
                    panic!("{gate} should produce two-qubit gates");
                };
                (q0, q1)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            pairs,
            [(2, 7), (13, 19)],
            "{gate} broadcast incorrect pairs"
        );
    }
}

#[test]
fn classically_controllable_gates_broadcast_over_target_pairs() {
    let gates = [
        ("CX", "rec[-1] 2 rec[-3] 7"),
        ("CY", "rec[-1] 2 rec[-3] 7"),
        ("CZ", "rec[-1] 2 7 rec[-3]"),
        ("XCZ", "2 rec[-1] 7 rec[-3]"),
        ("YCZ", "2 rec[-1] 7 rec[-3]"),
    ];

    for (gate, targets) in gates {
        let source = format!("M 0 1 2\n{gate} {targets}");
        let pairs = lower(&source)
            .into_iter()
            .skip(3)
            .map(|kind| {
                let InstructionKind::ClassicallyControlledPauli {
                    control, target, ..
                } = kind
                else {
                    panic!("{gate} should produce classically controlled gates");
                };
                (control.offset, target)
            })
            .collect::<Vec<_>>();

        assert_eq!(pairs, [(1, 2), (3, 7)], "{gate} broadcast incorrect pairs");
    }
}

#[test]
fn three_qubit_gates_broadcast_over_target_triples() {
    for gate in ["CCZ", "CCX"] {
        let source = format!("{gate} 2 7 13 19 23 29");
        let triples = lower(&source)
            .into_iter()
            .map(|kind| {
                let InstructionKind::ThreeQubitGate { q0, q1, q2, .. } = kind else {
                    panic!("{gate} should produce three-qubit gates");
                };
                (q0, q1, q2)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            triples,
            [(2, 7, 13), (19, 23, 29)],
            "{gate} broadcast incorrect triples"
        );
    }
}

#[test]
fn resets_broadcast_over_targets() {
    for gate in ["R", "RX", "RY"] {
        let source = format!("{gate} 2 7 13");
        let qubits = lower(&source)
            .into_iter()
            .map(|kind| {
                let InstructionKind::Reset { qubit, .. } = kind else {
                    panic!("{gate} should produce resets");
                };
                qubit
            })
            .collect::<Vec<_>>();

        assert_eq!(qubits, [2, 7, 13], "{gate} broadcast incorrect targets");
    }
}

#[test]
fn single_qubit_measurements_broadcast_over_targets() {
    for gate in ["M", "MR", "MRX", "MRY", "MX", "MY"] {
        let source = format!("{gate} 2 7 13");
        let qubits = lower(&source)
            .into_iter()
            .map(|kind| {
                let InstructionKind::SingleQubitMeasurement { qubit, .. } = kind else {
                    panic!("{gate} should produce single-qubit measurements");
                };
                qubit
            })
            .collect::<Vec<_>>();

        assert_eq!(qubits, [2, 7, 13], "{gate} broadcast incorrect targets");
    }
}

#[test]
fn two_qubit_measurements_broadcast_over_target_pairs() {
    for gate in ["MXX", "MYY", "MZZ"] {
        let source = format!("{gate} 2 7 13 19");
        let pairs = lower(&source)
            .into_iter()
            .map(|kind| {
                let InstructionKind::TwoQubitMeasurement { q0, q1, .. } = kind else {
                    panic!("{gate} should produce two-qubit measurements");
                };
                (q0, q1)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            pairs,
            [(2, 7), (13, 19)],
            "{gate} broadcast incorrect pairs"
        );
    }
}

#[test]
fn pauli_product_instructions_broadcast_over_products() {
    let sources = [
        "MPP X2*Y7 Z13*X19",
        "SPP X2*Y7 Z13*X19",
        "SPP_DAG X2*Y7 Z13*X19",
        "TPP X2*Y7 Z13*X19",
        "TPP_DAG X2*Y7 Z13*X19",
        "R_PAULI(0.25) X2*Y7 Z13*X19",
    ];

    for source in sources {
        let products = lower(source)
            .into_iter()
            .map(|kind| {
                let product = match kind {
                    InstructionKind::PauliProductMeasurement { product, .. }
                    | InstructionKind::PauliProductGate { product, .. }
                    | InstructionKind::PauliProductRotation { product, .. } => product,
                    _ => panic!("{source} should produce Pauli product instructions"),
                };
                product
                    .factors
                    .into_iter()
                    .map(|factor| (factor.pauli.as_str(), factor.qubit))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            products,
            [vec![("X", 2), ("Y", 7)], vec![("Z", 13), ("X", 19)]],
            "{source} broadcast incorrect products"
        );
    }
}

#[test]
fn single_qubit_noise_broadcasts_over_targets() {
    let gates = [
        "DEPOLARIZE1(0.1)",
        "HERALDED_ERASE(0.1)",
        "HERALDED_PAULI_CHANNEL_1(0.1,0,0,0)",
        "PAULI_CHANNEL_1(0.1,0,0)",
        "X_ERROR(0.1)",
        "Y_ERROR(0.1)",
        "Z_ERROR(0.1)",
        "LOSS_ERROR(0.1)",
    ];

    for gate in gates {
        let source = format!("{gate} 2 7 13");
        let qubits = lower(&source)
            .into_iter()
            .map(|kind| {
                let InstructionKind::Noise(noise) = kind else {
                    panic!("{gate} should produce noise instructions");
                };
                match noise {
                    Noise::SingleQubitNoise { qubit, .. }
                    | Noise::HeraldedErase { qubit, .. }
                    | Noise::HeraldedPauliChannel1 { qubit, .. }
                    | Noise::PauliChannel1 { qubit, .. } => qubit,
                    _ => panic!("{gate} should produce single-qubit noise"),
                }
            })
            .collect::<Vec<_>>();

        assert_eq!(qubits, [2, 7, 13], "{gate} broadcast incorrect targets");
    }
}

#[test]
fn two_qubit_noise_broadcasts_over_target_pairs() {
    let gates = [
        "DEPOLARIZE2(0.1)",
        "PAULI_CHANNEL_2(0.1,0,0,0,0,0,0,0,0,0,0,0,0,0,0)",
    ];

    for gate in gates {
        let source = format!("{gate} 2 7 13 19");
        let pairs = lower(&source)
            .into_iter()
            .map(|kind| {
                let InstructionKind::Noise(noise) = kind else {
                    panic!("{gate} should produce noise instructions");
                };
                match noise {
                    Noise::Depolarize2 { q0, q1, .. } | Noise::PauliChannel2 { q0, q1, .. } => {
                        (q0, q1)
                    }
                    _ => panic!("{gate} should produce two-qubit noise"),
                }
            })
            .collect::<Vec<_>>();

        assert_eq!(
            pairs,
            [(2, 7), (13, 19)],
            "{gate} broadcast incorrect pairs"
        );
    }
}

#[test]
fn peek_loss_broadcasts_over_targets() {
    let source = "PEEK_LOSS 2 7 13";
    let qubits = lower(source)
        .into_iter()
        .map(|kind| {
            let InstructionKind::PeekLoss { qubit, .. } = kind else {
                panic!("PEEK_LOSS should produce peek-loss instructions");
            };
            qubit
        })
        .collect::<Vec<_>>();

    assert_eq!(qubits, [2, 7, 13]);
}

#[test]
fn annotations_broadcast_over_targets() {
    let source = "QUBIT_COORDS(1,2) 2 7 13";
    let coordinates = lower(source)
        .into_iter()
        .map(|kind| {
            let InstructionKind::Annotation(Annotation::QubitCoordinates { qubit, .. }) = kind
            else {
                panic!("QUBIT_COORDS should produce coordinate annotations");
            };
            qubit
        })
        .collect::<Vec<_>>();
    assert_eq!(coordinates, [2, 7, 13]);

    let source = "MPAD 0 1 0";
    let padding = lower(source)
        .into_iter()
        .map(|kind| {
            let InstructionKind::Annotation(Annotation::MeasurementPadding { value, .. }) = kind
            else {
                panic!("MPAD should produce measurement-padding annotations");
            };
            value
        })
        .collect::<Vec<_>>();
    assert_eq!(padding, [false, true, false]);
}

#[test]
fn single_qubit_rotations_broadcast_over_targets() {
    let gates = ["R_X(0.25)", "R_Y(0.25)", "R_Z(0.25)", "U3(0.1,0.2,0.3)"];

    for gate in gates {
        let source = format!("{gate} 2 7 13");
        let qubits = lower(&source)
            .into_iter()
            .map(|kind| match kind {
                InstructionKind::SingleQubitRotation { qubit, .. }
                | InstructionKind::U3 { qubit, .. } => qubit,
                _ => panic!("{gate} should produce single-qubit rotations"),
            })
            .collect::<Vec<_>>();

        assert_eq!(qubits, [2, 7, 13], "{gate} broadcast incorrect targets");
    }
}

#[test]
fn two_qubit_rotations_broadcast_over_target_pairs() {
    for gate in ["R_XX", "R_YY", "R_ZZ"] {
        let source = format!("{gate}(0.25) 2 7 13 19");
        let pairs = lower(&source)
            .into_iter()
            .map(|kind| {
                let InstructionKind::TwoQubitRotation { q0, q1, .. } = kind else {
                    panic!("{gate} should produce two-qubit rotations");
                };
                (q0, q1)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            pairs,
            [(2, 7), (13, 19)],
            "{gate} broadcast incorrect pairs"
        );
    }
}
