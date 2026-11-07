// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The gates of the machine's QIR instructions: which instructions are
//! gates, their mnemonics, the matrix of each against its textbook value,
//! the operand order, the angle, and the identities the gates satisfy.

use super::{correlated_noise, gate, measure, op, program, reset, ret};
use crate::bytecode::encoding::{
    FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_AUX2_IMM, OP_PEEK_LOSS, OP_READOUT_NOISE, OPID_CX, OPID_CY,
    OPID_CZ, OPID_H, OPID_MOVE, OPID_MRESETZ, OPID_MZ, OPID_RX, OPID_RXX, OPID_RY, OPID_RYY,
    OPID_RZ, OPID_RZZ, OPID_S, OPID_S_ADJ, OPID_SWAP, OPID_SX, OPID_SX_ADJ, OPID_T, OPID_T_ADJ,
    OPID_X, OPID_Y, OPID_Z,
};
use crate::qir::OperatorMatrix;
use crate::qir::{QIR_INSTRUCTIONS, QirGate, QirInstruction, QirProgram};
use crate::tests::qir::operator_matrix::{I, L, O, apply, rows};
use num_complex::Complex64;
use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_2, PI};

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

/// The instructions of a program that uses every instruction of the `qir`
/// set.
fn every_instruction() -> Vec<QirInstruction> {
    let mut block: Vec<_> = [
        OPID_X,
        OPID_Y,
        OPID_Z,
        OPID_H,
        OPID_S,
        OPID_S_ADJ,
        OPID_T,
        OPID_T_ADJ,
        OPID_SX,
        OPID_SX_ADJ,
        OPID_RX,
        OPID_RY,
        OPID_RZ,
        OPID_CX,
        OPID_CZ,
        OPID_RXX,
        OPID_RYY,
        OPID_RZZ,
        OPID_SWAP,
        OPID_CY,
    ]
    .into_iter()
    .map(|op_id| gate(op_id, 0, 1))
    .collect();
    block.extend([
        measure(OPID_MZ, 0, 0),
        measure(OPID_MRESETZ, 0, 1),
        reset(0),
        gate(OPID_MOVE, 0, 0),
        correlated_noise(0, 0),
        op(OP_PEEK_LOSS, FLAG_AUX0_IMM | FLAG_AUX1_IMM),
        op(
            OP_READOUT_NOISE,
            FLAG_AUX0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
        ),
        ret(),
    ]);
    QirProgram::new(&program(&[&block]))
        .expect("the program has no fault")
        .instructions()
        .to_vec()
}

/// The matrix of `gate`, at `angle` for a rotation.
fn matrix(gate: QirGate, angle: f64) -> OperatorMatrix {
    let rotation = ONE_QUBIT_ROTATIONS.contains(&gate) || TWO_QUBIT_ROTATIONS.contains(&gate);
    gate.matrix_at(rotation.then_some(angle))
}

/// The matrix of `gate`, which takes no angle.
fn fixed(gate: QirGate) -> OperatorMatrix {
    matrix(gate, f64::NAN)
}

/// Every gate.
fn every_gate() -> impl Iterator<Item = QirGate> {
    ONE_QUBIT_FIXED
        .into_iter()
        .chain(ONE_QUBIT_ROTATIONS)
        .chain(TWO_QUBIT_FIXED)
        .chain(TWO_QUBIT_ROTATIONS)
}

/// Every gate's matrix, rotations at an angle of 0.7.
fn every_matrix() -> impl Iterator<Item = (QirGate, OperatorMatrix)> {
    every_gate().map(|gate| (gate, matrix(gate, 0.7)))
}

/// `left · right`, as rows.
fn product(left: &OperatorMatrix, right: &OperatorMatrix) -> Vec<Vec<Complex64>> {
    let (left, right) = (rows(left), rows(right));
    (0..left.len())
        .map(|i| {
            (0..left.len())
                .map(|j| (0..left.len()).map(|k| left[i][k] * right[k][j]).sum())
                .collect()
        })
        .collect()
}

/// `a ⊗ b` for one-qubit matrices, `a` on the first (most significant)
/// operand.
fn kron(a: &OperatorMatrix, b: &OperatorMatrix) -> OperatorMatrix {
    let mut values = [O; 16];
    for (index, value) in values.iter_mut().enumerate() {
        let (output, input) = (index / 4, index % 4);
        *value = a.entry(output / 2, input / 2) * b.entry(output % 2, input % 2);
    }
    OperatorMatrix::Two(values)
}

/// The 4×4 matrix of `rows`.
fn two(rows: &[Vec<Complex64>]) -> OperatorMatrix {
    let values: Vec<Complex64> = rows.iter().flatten().copied().collect();
    OperatorMatrix::Two(values.try_into().expect("a 4×4 matrix"))
}

/// The 2×2 identity.
fn identity() -> OperatorMatrix {
    OperatorMatrix::One([L, O, O, L])
}

/// Asserts that `actual` and `expected` differ by at most 1e-15 in each
/// entry.
fn assert_rows_close(actual: &[Vec<Complex64>], expected: &[Vec<Complex64>]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        for (a, e) in actual.iter().zip(expected) {
            assert!((a - e).norm() <= 1e-15, "{actual:?} != {expected:?}");
        }
    }
}

/// A program using the whole set, which a simulator builds its table of:
/// its unitary instructions are gates, and nothing else is.
#[test]
fn only_the_unitary_instructions_are_gates() {
    let instructions = every_instruction();
    assert_eq!(
        instructions.len(),
        QIR_INSTRUCTIONS.len(),
        "the program uses the whole set"
    );
    let mnemonics = |gates: bool| {
        instructions
            .iter()
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

/// Each rotation at an angle whose matrix is known exactly.
#[test]
fn rotations_match_their_textbook_matrices_at_known_angles() {
    let h = FRAC_1_SQRT_2;
    let c = Complex64::new;
    // Rx(π/2) = (I - iX)/√2.
    assert_rows_close(
        &rows(&matrix(QirGate::Rx, FRAC_PI_2)),
        &[vec![c(h, 0.0), c(0.0, -h)], vec![c(0.0, -h), c(h, 0.0)]],
    );
    // Ry(π/2) = (I - iY)/√2.
    assert_rows_close(
        &rows(&matrix(QirGate::Ry, FRAC_PI_2)),
        &[vec![c(h, 0.0), c(-h, 0.0)], vec![c(h, 0.0), c(h, 0.0)]],
    );
    // Rz(π) = diag(-i, i).
    assert_rows_close(&rows(&matrix(QirGate::Rz, PI)), &[vec![-I, O], vec![O, I]]);
    // Rzz(π) = -i Z⊗Z = diag(-i, i, i, -i).
    assert_rows_close(
        &rows(&matrix(QirGate::Rzz, PI)),
        &[
            vec![-I, O, O, O],
            vec![O, I, O, O],
            vec![O, O, I, O],
            vec![O, O, O, -I],
        ],
    );
    // Rxx(π) = -i X⊗X and Ryy(π) = -i Y⊗Y, where Y⊗Y is -1 on |00⟩↔|11⟩
    // and +1 on |01⟩↔|10⟩.
    assert_rows_close(
        &rows(&matrix(QirGate::Rxx, PI)),
        &[
            vec![O, O, O, -I],
            vec![O, O, -I, O],
            vec![O, -I, O, O],
            vec![-I, O, O, O],
        ],
    );
    assert_rows_close(
        &rows(&matrix(QirGate::Ryy, PI)),
        &[
            vec![O, O, O, I],
            vec![O, O, -I, O],
            vec![O, -I, O, O],
            vec![I, O, O, O],
        ],
    );
}

/// Ry(π/2) on |0⟩: the -1/√2 is `M[0][1]`, so it gives |+⟩, where a
/// transposed layout would give |-⟩.
#[test]
fn ry_takes_zero_to_plus() {
    let h = Complex64::new(FRAC_1_SQRT_2, 0.0);
    assert_rows_close(
        &[apply(&matrix(QirGate::Ry, FRAC_PI_2), &[L, O])],
        &[vec![h, h]],
    );
}

/// Y on |1⟩: Y is not symmetric, so Y|1⟩ = -i|0⟩, not i|0⟩.
#[test]
fn y_takes_one_to_minus_i_zero() {
    assert_eq!(apply(&fixed(QirGate::Y), &[O, L]), [-I, O]);
}

/// CX on |10⟩ and |01⟩, its first operand the control: |control, target⟩
/// = |10⟩ is index 2 and becomes |11⟩, index 3; |01⟩ stays.
#[test]
fn the_first_operand_is_the_most_significant_bit() {
    let cx = fixed(QirGate::Cx);
    assert_eq!(apply(&cx, &[O, O, L, O]), [O, O, O, L]);
    assert_eq!(apply(&cx, &[O, L, O, O]), [O, L, O, O]);
}

/// CY on |10⟩ and |01⟩: |10⟩ (index 2) becomes Y|0⟩ = i|1⟩ on the target,
/// i.e. i|11⟩; |01⟩ (control 0) is untouched.
#[test]
fn cy_applies_y_to_its_second_operand() {
    let cy = fixed(QirGate::Cy);
    assert_eq!(apply(&cy, &[O, O, L, O]), [O, O, O, I]);
    assert_eq!(apply(&cy, &[O, L, O, O]), [O, L, O, O]);
}

/// SX, S and T (and their inverses) applied twice.
#[test]
fn square_roots_square_to_their_gates() {
    let (sx, s, s_adj) = (fixed(QirGate::Sx), fixed(QirGate::S), fixed(QirGate::SAdj));
    let (t, t_adj) = (fixed(QirGate::T), fixed(QirGate::TAdj));
    assert_eq!(product(&sx, &sx), rows(&fixed(QirGate::X)));
    assert_eq!(product(&s, &s), rows(&fixed(QirGate::Z)));
    assert_rows_close(&product(&t, &t), &rows(&s));
    assert_rows_close(&product(&t_adj, &t_adj), &rows(&s_adj));
}

/// S, T and SX, each followed by its inverse.
#[test]
fn adjoints_undo_their_gates() {
    for (gate, adjoint) in [
        (QirGate::S, QirGate::SAdj),
        (QirGate::T, QirGate::TAdj),
        (QirGate::Sx, QirGate::SxAdj),
    ] {
        assert_rows_close(&product(&fixed(adjoint), &fixed(gate)), &rows(&identity()));
    }
}

/// Y = iXZ.
#[test]
fn y_is_i_times_x_times_z() {
    let i_xz: Vec<Vec<Complex64>> = product(&fixed(QirGate::X), &fixed(QirGate::Z))
        .into_iter()
        .map(|row| row.into_iter().map(|value| I * value).collect())
        .collect();
    assert_rows_close(&rows(&fixed(QirGate::Y)), &i_xz);
}

/// CY = (I⊗S)·CX·(I⊗S†), because Y = S X S†.
#[test]
fn cy_is_cx_conjugated_by_s_on_its_target() {
    let conjugated = product(
        &two(&product(
            &kron(&identity(), &fixed(QirGate::S)),
            &fixed(QirGate::Cx),
        )),
        &kron(&identity(), &fixed(QirGate::SAdj)),
    );
    assert_rows_close(&rows(&fixed(QirGate::Cy)), &conjugated);
}

/// Rxx = (H⊗H)·Rzz·(H⊗H) and Ryy = (S⊗S)·Rxx·(S†⊗S†), at the same angle.
#[test]
fn rxx_and_ryy_are_rzz_in_another_basis() {
    let angle = 0.7;
    let (rzz, rxx) = (matrix(QirGate::Rzz, angle), matrix(QirGate::Rxx, angle));
    let hadamards = kron(&fixed(QirGate::H), &fixed(QirGate::H));
    assert_rows_close(
        &rows(&rxx),
        &product(&two(&product(&hadamards, &rzz)), &hadamards),
    );
    assert_rows_close(
        &rows(&matrix(QirGate::Ryy, angle)),
        &product(
            &two(&product(
                &kron(&fixed(QirGate::S), &fixed(QirGate::S)),
                &rxx,
            )),
            &kron(&fixed(QirGate::SAdj), &fixed(QirGate::SAdj)),
        ),
    );
}

/// SWAP = CX(0→1)·CX(1→0)·CX(0→1), where CX(1→0) = (H⊗H)·CX·(H⊗H).
#[test]
fn swap_is_three_cx() {
    let cx = fixed(QirGate::Cx);
    let hadamards = kron(&fixed(QirGate::H), &fixed(QirGate::H));
    let reversed_cx = two(&product(&two(&product(&hadamards, &cx)), &hadamards));
    let three_cx = product(&two(&product(&cx, &reversed_cx)), &cx);
    assert_rows_close(&rows(&fixed(QirGate::Swap)), &three_cx);
}

/// Every gate, rotations at an angle of 0.7: `M†M = 1`.
#[test]
fn every_gate_is_unitary() {
    for (gate, matrix) in every_matrix() {
        let matrix = rows(&matrix);
        let dimension = matrix.len();
        let gram: Vec<Vec<Complex64>> = (0..dimension)
            .map(|i| {
                (0..dimension)
                    .map(|j| {
                        (0..dimension)
                            .map(|k| matrix[k][i].conj() * matrix[k][j])
                            .sum()
                    })
                    .collect()
            })
            .collect();
        let identity: Vec<Vec<Complex64>> = (0..dimension)
            .map(|i| (0..dimension).map(|j| if i == j { L } else { O }).collect())
            .collect();
        assert!(
            gram.iter()
                .flatten()
                .zip(identity.iter().flatten())
                .all(|(a, e)| (a - e).norm() <= 1e-15),
            "{gate:?}"
        );
    }
}

/// Every gate: a one-qubit gate's matrix is 2×2, a two-qubit gate's 4×4.
#[test]
fn one_qubit_gates_are_2x2_and_two_qubit_gates_4x4() {
    for (gate, matrix) in every_matrix() {
        let qubits = if ONE_QUBIT_FIXED.contains(&gate) || ONE_QUBIT_ROTATIONS.contains(&gate) {
            1
        } else {
            2
        };
        assert_eq!(
            (
                matrix.qubit_count(),
                matrix.dimension(),
                matrix.row_major().len()
            ),
            (qubits, 1 << qubits, 1 << (2 * qubits)),
            "{gate:?}"
        );
    }
}

/// Every gate, rotations at an angle of 0.7: the diagonal ones, which a
/// simulator may store as their diagonal, are those of the Z basis.
#[test]
fn only_the_z_basis_gates_are_diagonal() {
    let diagonal: Vec<_> = every_matrix()
        .filter(|(_, matrix)| matrix.is_diagonal())
        .map(|(gate, _)| gate)
        .collect();
    assert_eq!(
        diagonal,
        [
            QirGate::Z,
            QirGate::S,
            QirGate::SAdj,
            QirGate::T,
            QirGate::TAdj,
            QirGate::Rz,
            QirGate::Cz,
            QirGate::Rzz
        ]
    );
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
