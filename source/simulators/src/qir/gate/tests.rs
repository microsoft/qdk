// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The gates of the machine's QIR instructions: which instructions are
//! gates, their mnemonics, the matrix of each against its textbook value,
//! and the angle.

use crate::qir::quantum_instruction_encoding::QIR_INSTRUCTIONS;
use crate::qir::test_utils::{I, L, O};
use crate::qir::{OperatorMatrix, QirGate, QirInstruction};
use num_complex::Complex64;
use std::f64::consts::{FRAC_1_SQRT_2, PI};

/// The matrix as rows `M[out][in]`, read through [`OperatorMatrix::entry`].
fn rows(matrix: &OperatorMatrix) -> Vec<Vec<Complex64>> {
    let dimension = matrix.dimension();
    (0..dimension)
        .map(|output| {
            (0..dimension)
                .map(|input| matrix.entry(output, input))
                .collect()
        })
        .collect()
}

/// The one-qubit gates that take no angle.
const ONE_QUBIT_FIXED: [QirGate; 10] = [
    QirGate::X,
    QirGate::Y,
    QirGate::Z,
    QirGate::H,
    QirGate::S,
    QirGate::SAdj,
    QirGate::T,
    QirGate::TAdj,
    QirGate::Sx,
    QirGate::SxAdj,
];
/// The one-qubit rotations.
const ONE_QUBIT_ROTATIONS: [QirGate; 3] = [QirGate::Rx, QirGate::Ry, QirGate::Rz];
/// The two-qubit gates that take no angle.
const TWO_QUBIT_FIXED: [QirGate; 4] = [QirGate::Cx, QirGate::Cy, QirGate::Cz, QirGate::Swap];
/// The two-qubit rotations.
const TWO_QUBIT_ROTATIONS: [QirGate; 3] = [QirGate::Rxx, QirGate::Ryy, QirGate::Rzz];

/// The matrix of `gate`, at `angle` for a rotation.
fn matrix(gate: QirGate, angle: f64) -> OperatorMatrix {
    let rotation = ONE_QUBIT_ROTATIONS.contains(&gate) || TWO_QUBIT_ROTATIONS.contains(&gate);
    gate.matrix_at(rotation.then_some(angle))
}

/// The matrix of `gate`, which takes no angle.
fn fixed(gate: QirGate) -> OperatorMatrix {
    matrix(gate, f64::NAN)
}

/// The whole `qir` set, which a simulator builds its table of: its
/// unitary instructions are gates, and nothing else is.
#[test]
fn only_the_unitary_instructions_are_gates() {
    let mnemonics = |gates: bool| {
        QIR_INSTRUCTIONS
            .iter()
            .map(|(_, instruction)| instruction)
            .filter(|instruction| matches!(instruction, QirInstruction::Gate(_)) == gates)
            .map(|instruction| instruction.mnemonic())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        mnemonics(true),
        [
            "x", "y", "z", "h", "s", "s_adj", "t", "t_adj", "sx", "sx_adj", "rx", "ry", "rz", "cx",
            "cz", "rxx", "ryy", "rzz", "swap", "cy"
        ]
    );
    assert_eq!(
        mnemonics(false),
        [
            "reset",
            "mz",
            "mresetz",
            "move",
            "mx",
            "my",
            "correlated_noise",
            "peek_loss",
            "readout_noise"
        ]
    );
}

/// Each gate that takes no angle, against its textbook matrix.
#[test]
fn fixed_gates_match_their_textbook_matrices() {
    let h = Complex64::new(FRAC_1_SQRT_2, 0.0);
    let cases = [
        (QirGate::X, vec![vec![O, L], vec![L, O]]),
        (QirGate::Y, vec![vec![O, -I], vec![I, O]]),
        (QirGate::Z, vec![vec![L, O], vec![O, -L]]),
        (QirGate::H, vec![vec![h, h], vec![h, -h]]),
        (QirGate::S, vec![vec![L, O], vec![O, I]]),
        (QirGate::SAdj, vec![vec![L, O], vec![O, -I]]),
        (
            QirGate::T,
            vec![vec![L, O], vec![O, (L + I) * FRAC_1_SQRT_2]],
        ),
        (
            QirGate::TAdj,
            vec![vec![L, O], vec![O, (L - I) * FRAC_1_SQRT_2]],
        ),
        (
            QirGate::Sx,
            vec![
                vec![(L + I) / 2.0, (L - I) / 2.0],
                vec![(L - I) / 2.0, (L + I) / 2.0],
            ],
        ),
        (
            QirGate::SxAdj,
            vec![
                vec![(L - I) / 2.0, (L + I) / 2.0],
                vec![(L + I) / 2.0, (L - I) / 2.0],
            ],
        ),
        (
            QirGate::Cx,
            vec![
                vec![L, O, O, O],
                vec![O, L, O, O],
                vec![O, O, O, L],
                vec![O, O, L, O],
            ],
        ),
        (
            QirGate::Cy,
            vec![
                vec![L, O, O, O],
                vec![O, L, O, O],
                vec![O, O, O, -I],
                vec![O, O, I, O],
            ],
        ),
        (
            QirGate::Cz,
            vec![
                vec![L, O, O, O],
                vec![O, L, O, O],
                vec![O, O, L, O],
                vec![O, O, O, -L],
            ],
        ),
        (
            QirGate::Swap,
            vec![
                vec![L, O, O, O],
                vec![O, O, L, O],
                vec![O, L, O, O],
                vec![O, O, O, L],
            ],
        ),
    ];
    for (gate, expected) in cases {
        assert_eq!(rows(&fixed(gate)), expected, "{gate:?}");
    }
}

/// Each rotation at θ = 2π/3, where cos(θ/2) = 1/2 and sin(θ/2) = √3/2
/// differ and neither is zero: a sine swapped with the cosine, which
/// π/2 hides, shows here, and so does a flipped cosine sign, which π hides.
#[test]
fn rotations_match_their_textbook_matrices_where_sine_and_cosine_differ() {
    let (cosine, sine) = (
        Complex64::new(0.5, 0.0),
        Complex64::new(3f64.sqrt() / 2.0, 0.0),
    );
    let cases = [
        // Rx(θ) = [[cos θ/2, -i sin θ/2], [-i sin θ/2, cos θ/2]].
        (
            QirGate::Rx,
            vec![vec![cosine, -I * sine], vec![-I * sine, cosine]],
        ),
        // Ry(θ) = [[cos θ/2, -sin θ/2], [sin θ/2, cos θ/2]].
        (QirGate::Ry, vec![vec![cosine, -sine], vec![sine, cosine]]),
        // Rz(θ) = diag(e^{-iθ/2}, e^{iθ/2}).
        (
            QirGate::Rz,
            vec![vec![cosine - I * sine, O], vec![O, cosine + I * sine]],
        ),
        // Rxx(θ) = cos(θ/2) I - i sin(θ/2) X⊗X.
        (
            QirGate::Rxx,
            vec![
                vec![cosine, O, O, -I * sine],
                vec![O, cosine, -I * sine, O],
                vec![O, -I * sine, cosine, O],
                vec![-I * sine, O, O, cosine],
            ],
        ),
        // Ryy(θ) = cos(θ/2) I - i sin(θ/2) Y⊗Y, where Y⊗Y is -1 on
        // |00⟩↔|11⟩ and +1 on |01⟩↔|10⟩.
        (
            QirGate::Ryy,
            vec![
                vec![cosine, O, O, I * sine],
                vec![O, cosine, -I * sine, O],
                vec![O, -I * sine, cosine, O],
                vec![I * sine, O, O, cosine],
            ],
        ),
        // Rzz(θ) = diag(e^{-iθ/2}, e^{iθ/2}, e^{iθ/2}, e^{-iθ/2}).
        (
            QirGate::Rzz,
            vec![
                vec![cosine - I * sine, O, O, O],
                vec![O, cosine + I * sine, O, O],
                vec![O, O, cosine + I * sine, O],
                vec![O, O, O, cosine - I * sine],
            ],
        ),
    ];
    for (gate, expected) in cases {
        let actual = rows(&matrix(gate, 2.0 * PI / 3.0));
        let close = actual.len() == expected.len()
            && actual
                .iter()
                .flatten()
                .zip(expected.iter().flatten())
                .all(|(a, e)| (a - e).norm() <= 1e-15);
        assert!(close, "{gate:?}: {actual:?} != {expected:?}");
    }
}

/// A rotation's matrix asked for with no angle.
#[test]
#[should_panic(expected = "a rotation takes an angle, and a gate that is not one takes none")]
fn a_rotation_given_no_angle_panics() {
    let _ = QirGate::Rzz.matrix_at(None);
}

/// X's matrix asked for at an angle.
#[test]
#[should_panic(expected = "a rotation takes an angle, and a gate that is not one takes none")]
fn a_fixed_gate_given_an_angle_panics() {
    let _ = QirGate::X.matrix_at(Some(0.0));
}

/// Every gate that takes no angle, bit by bit: each zero part, real or
/// imaginary, is +0.0, so that every simulator applies the same bits.
#[test]
fn fixed_gates_have_no_negative_zero() {
    for gate in ONE_QUBIT_FIXED.into_iter().chain(TWO_QUBIT_FIXED) {
        for value in fixed(gate).row_major() {
            for part in [value.re, value.im] {
                assert!(part != 0.0 || part.to_bits() == 0, "{gate:?}: {value}");
            }
        }
    }
}
