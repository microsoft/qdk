// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests that each rewrite means the same thing as the gate it replaces.
//!
//! A rewrite that is merely plausible produces a program that runs and is
//! silently wrong, so each one is checked against the gate's own matrix rather
//! than against an expected instruction sequence.

use std::f64::consts::FRAC_1_SQRT_2;

use num_complex::Complex64;
use qsc_circuit_store::StandardGate;

use super::{Step, decompose};
use crate::circuit::{GateLowering, gate_lowering};

type Matrix = Vec<Vec<Complex64>>;

const TOLERANCE: f64 = 1e-12;

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

fn identity(dim: usize) -> Matrix {
    (0..dim)
        .map(|row| {
            (0..dim)
                .map(|col| if row == col { c(1.0, 0.0) } else { c(0.0, 0.0) })
                .collect()
        })
        .collect()
}

fn multiply(left: &Matrix, right: &Matrix) -> Matrix {
    let dim = left.len();
    (0..dim)
        .map(|row| {
            (0..dim)
                .map(|col| {
                    (0..dim)
                        .map(|k| left[row][k] * right[k][col])
                        .sum::<Complex64>()
                })
                .collect()
        })
        .collect()
}

fn adjoint(matrix: &Matrix) -> Matrix {
    let dim = matrix.len();
    (0..dim)
        .map(|row| (0..dim).map(|col| matrix[col][row].conj()).collect())
        .collect()
}

/// Bit of `value` belonging to `qubit`, with qubit zero the most significant.
fn bit(value: usize, qubit: usize, width: usize) -> usize {
    (value >> (width - 1 - qubit)) & 1
}

/// Places a gate acting on `acting` into a register of `width` qubits.
fn embed(gate: &Matrix, acting: &[usize], width: usize) -> Matrix {
    let dim = 1 << width;
    let arity = acting.len();
    let sub_index = |value: usize| {
        acting
            .iter()
            .enumerate()
            .fold(0, |index, (position, qubit)| {
                index | (bit(value, *qubit, width) << (arity - 1 - position))
            })
    };
    (0..dim)
        .map(|row| {
            (0..dim)
                .map(|col| {
                    let untouched_agree = (0..width)
                        .filter(|qubit| !acting.contains(qubit))
                        .all(|qubit| bit(row, qubit, width) == bit(col, qubit, width));
                    if untouched_agree {
                        gate[sub_index(row)][sub_index(col)]
                    } else {
                        c(0.0, 0.0)
                    }
                })
                .collect()
        })
        .collect()
}

/// Adds a control qubit ahead of the qubits `gate` already acts on.
fn controlled(gate: &Matrix) -> Matrix {
    let dim = gate.len();
    let mut out = identity(dim * 2);
    for row in 0..dim {
        for col in 0..dim {
            out[dim + row][dim + col] = gate[row][col];
        }
    }
    out
}

fn single(entries: [[Complex64; 2]; 2]) -> Matrix {
    entries.iter().map(|row| row.to_vec()).collect()
}

fn rz(angle: f64) -> Matrix {
    let half = angle / 2.0;
    single([
        [Complex64::from_polar(1.0, -half), c(0.0, 0.0)],
        [c(0.0, 0.0), Complex64::from_polar(1.0, half)],
    ])
}

fn ry(angle: f64) -> Matrix {
    let half = angle / 2.0;
    single([
        [c(half.cos(), 0.0), c(-half.sin(), 0.0)],
        [c(half.sin(), 0.0), c(half.cos(), 0.0)],
    ])
}

fn rx(angle: f64) -> Matrix {
    let half = angle / 2.0;
    single([
        [c(half.cos(), 0.0), c(0.0, -half.sin())],
        [c(0.0, -half.sin()), c(half.cos(), 0.0)],
    ])
}

fn pauli_x() -> Matrix {
    single([[c(0.0, 0.0), c(1.0, 0.0)], [c(1.0, 0.0), c(0.0, 0.0)]])
}

fn pauli_y() -> Matrix {
    single([[c(0.0, 0.0), c(0.0, -1.0)], [c(0.0, 1.0), c(0.0, 0.0)]])
}

fn pauli_z() -> Matrix {
    single([[c(1.0, 0.0), c(0.0, 0.0)], [c(0.0, 0.0), c(-1.0, 0.0)]])
}

fn hadamard() -> Matrix {
    single([
        [c(FRAC_1_SQRT_2, 0.0), c(FRAC_1_SQRT_2, 0.0)],
        [c(FRAC_1_SQRT_2, 0.0), c(-FRAC_1_SQRT_2, 0.0)],
    ])
}

fn phase() -> Matrix {
    single([[c(1.0, 0.0), c(0.0, 0.0)], [c(0.0, 0.0), c(0.0, 1.0)]])
}

fn sqrt_x() -> Matrix {
    single([[c(0.5, 0.5), c(0.5, -0.5)], [c(0.5, -0.5), c(0.5, 0.5)]])
}

fn swap() -> Matrix {
    vec![
        vec![c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(0.0, 0.0)],
        vec![c(0.0, 0.0), c(0.0, 0.0), c(1.0, 0.0), c(0.0, 0.0)],
        vec![c(0.0, 0.0), c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0)],
        vec![c(0.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(1.0, 0.0)],
    ]
}

/// The matrix of a gate, used both as a reference and inside a rewrite.
fn gate_matrix(gate: StandardGate, angles: &[f64]) -> Matrix {
    let angle = angles.first().copied().unwrap_or_default();
    match gate {
        StandardGate::I => identity(2),
        StandardGate::X => pauli_x(),
        StandardGate::Y => pauli_y(),
        StandardGate::Z => pauli_z(),
        StandardGate::H => hadamard(),
        StandardGate::S => phase(),
        StandardGate::SDag => adjoint(&phase()),
        StandardGate::SqrtX => sqrt_x(),
        StandardGate::SqrtXDag => adjoint(&sqrt_x()),
        StandardGate::Rx => rx(angle),
        StandardGate::Ry => ry(angle),
        StandardGate::Rz => rz(angle),
        StandardGate::Cx => controlled(&pauli_x()),
        StandardGate::Cy => controlled(&pauli_y()),
        StandardGate::Cz => controlled(&pauli_z()),
        StandardGate::Crx => controlled(&rx(angle)),
        StandardGate::Cry => controlled(&ry(angle)),
        StandardGate::Crz => controlled(&rz(angle)),
        StandardGate::Swap => swap(),
        StandardGate::Ccx => controlled(&controlled(&pauli_x())),
        StandardGate::Cswap => controlled(&swap()),
        other => panic!("no reference matrix for {}", other.name()),
    }
}

/// Multiplies the rewrite's gates together, in the order they are applied.
fn rewrite_matrix(steps: &[Step], width: usize) -> Matrix {
    let mut total = identity(1 << width);
    for step in steps {
        let acting: Vec<usize> = step.qubits.iter().map(|qubit| *qubit as usize).collect();
        let placed = embed(&gate_matrix(step.gate, &step.angles), &acting, width);
        // A gate applied later multiplies on the left.
        total = multiply(&placed, &total);
    }
    total
}

/// Asserts two matrices describe the same operation, up to an overall phase.
fn assert_same_operator(actual: &Matrix, expected: &Matrix, what: &str) {
    let dim = expected.len();
    let mut ratio = None;
    'outer: for row in 0..dim {
        for col in 0..dim {
            if expected[row][col].norm() > 1e-6 {
                ratio = Some(actual[row][col] / expected[row][col]);
                break 'outer;
            }
        }
    }
    let ratio = ratio.expect("expected matrix should not be all zeros");
    assert!(
        (ratio.norm() - 1.0).abs() < TOLERANCE,
        "{what}: rewrite differs by a scale of {}, not just a phase",
        ratio.norm()
    );
    for row in 0..dim {
        for col in 0..dim {
            let difference = actual[row][col] - ratio * expected[row][col];
            assert!(
                difference.norm() < TOLERANCE,
                "{what}: entry ({row}, {col}) differs by {}",
                difference.norm()
            );
        }
    }
}

/// Checks a rewrite against the matrix of the gate it replaces.
fn check_unitary(gate: StandardGate, angles: &[f64]) {
    let width = usize::from(gate.qubit_arity());
    let qubits: Vec<u32> = (0..u32::from(gate.qubit_arity())).collect();
    let steps = decompose(gate, &qubits, angles)
        .unwrap_or_else(|| panic!("{} should have a rewrite", gate.name()));

    assert_same_operator(
        &rewrite_matrix(&steps, width),
        &gate_matrix(gate, angles),
        gate.name(),
    );
}

fn check_placed_unitary(gate: StandardGate, angles: &[f64], qubits: &[u32], width: usize) {
    let steps = decompose(gate, qubits, angles).expect("gate has a decomposition");
    let acting: Vec<_> = qubits.iter().map(|qubit| *qubit as usize).collect();
    assert_same_operator(
        &rewrite_matrix(&steps, width),
        &embed(&gate_matrix(gate, angles), &acting, width),
        gate.name(),
    );
}

#[test]
fn adjoint_square_root_of_x_preserves_a_nonzero_operand() {
    check_placed_unitary(StandardGate::SqrtXDag, &[], &[2], 4);
}

#[test]
fn controlled_swap_preserves_permuted_noncontiguous_operands() {
    check_placed_unitary(StandardGate::Cswap, &[], &[3, 0, 2], 4);
}

#[test]
fn controlled_x_rotation_preserves_permuted_noncontiguous_operands() {
    check_placed_unitary(StandardGate::Crx, &[-2.5], &[3, 0], 4);
}

#[test]
fn controlled_y_rotation_preserves_permuted_noncontiguous_operands() {
    check_placed_unitary(StandardGate::Cry, &[-2.5], &[3, 0], 4);
}

#[test]
fn controlled_z_rotation_preserves_permuted_noncontiguous_operands() {
    check_placed_unitary(StandardGate::Crz, &[-2.5], &[3, 0], 4);
}

#[test]
fn the_adjoint_square_root_of_x_is_rewritten_faithfully() {
    check_unitary(StandardGate::SqrtXDag, &[]);
}

#[test]
fn the_controlled_swap_is_rewritten_faithfully() {
    check_unitary(StandardGate::Cswap, &[]);
}

#[test]
fn controlled_rotations_are_rewritten_faithfully() {
    // Includes zero, a half turn, a full turn, a negative angle, and a value
    // beyond a full turn, since a rewrite built from half angles can be right
    // near zero and wrong elsewhere.
    for angle in [
        0.0,
        0.1,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
        2.0 * std::f64::consts::PI,
        -2.5,
        7.0,
    ] {
        check_unitary(StandardGate::Crx, &[angle]);
        check_unitary(StandardGate::Cry, &[angle]);
        check_unitary(StandardGate::Crz, &[angle]);
    }
}

/// Splits a measurement rewrite into what runs before and after the measurement.
fn measurement_parts(gate: StandardGate, qubit: u32, width: usize) -> (Matrix, Matrix) {
    let steps = decompose(gate, &[qubit], &[])
        .unwrap_or_else(|| panic!("{} should have a rewrite", gate.name()));
    let at = steps
        .iter()
        .position(|step| step.gate == StandardGate::MeasureZ)
        .expect("a measurement rewrite should measure");
    assert_eq!(
        steps[at].qubits,
        vec![qubit],
        "measurement uses the actual operand"
    );
    (
        rewrite_matrix(&steps[..at], width),
        rewrite_matrix(&steps[at + 1..], width),
    )
}

/// Checks that a measurement rewrite measures the observable it claims to.
fn check_measurement(gate: StandardGate, observable: &Matrix) {
    check_placed_measurement(gate, observable, 0, 1);
}

fn check_placed_measurement(gate: StandardGate, observable: &Matrix, qubit: u32, width: usize) {
    let (before, after) = measurement_parts(gate, qubit, width);
    let observable = embed(observable, &[qubit as usize], width);

    // Measuring an observable means rotating it onto Z, so that the Z
    // measurement that follows reports the observable's own outcome.
    let rotated = multiply(&multiply(&before, &observable), &adjoint(&before));
    assert_same_operator(
        &rotated,
        &embed(&pauli_z(), &[qubit as usize], width),
        gate.name(),
    );

    // Rotating back leaves the qubit in an eigenstate of what was measured
    // rather than of Z, which is what a basis measurement means.
    assert_same_operator(&after, &adjoint(&before), gate.name());
}

#[test]
fn measuring_x_preserves_a_nonzero_operand_and_spectators() {
    check_placed_measurement(StandardGate::MeasureX, &pauli_x(), 2, 4);
}

#[test]
fn measuring_y_preserves_a_nonzero_operand_and_spectators() {
    check_placed_measurement(StandardGate::MeasureY, &pauli_y(), 2, 4);
}

#[test]
fn measuring_x_measures_the_x_observable() {
    check_measurement(StandardGate::MeasureX, &pauli_x());
}

#[test]
fn measuring_y_measures_the_y_observable() {
    check_measurement(StandardGate::MeasureY, &pauli_y());
}

/// Checks that a preparation rewrite leaves the state it claims to.
fn check_preparation(gate: StandardGate, observable: &Matrix) {
    check_placed_preparation(gate, observable, 0, 1);
}

fn check_placed_preparation(gate: StandardGate, observable: &Matrix, qubit: u32, width: usize) {
    let steps = decompose(gate, &[qubit], &[])
        .unwrap_or_else(|| panic!("{} should have a rewrite", gate.name()));
    assert_eq!(
        steps[0].gate,
        StandardGate::PrepareZ,
        "{} should start from a known state",
        gate.name()
    );
    assert_eq!(
        steps[0].qubits,
        vec![qubit],
        "preparation uses the actual operand"
    );

    // The prepared state is what the rotation makes of the zero state.
    let rotation = rewrite_matrix(&steps[1..], width);
    let state: Vec<_> = rotation.iter().map(|row| row[0]).collect();
    let observable = embed(observable, &[qubit as usize], width);

    // It has to be the observable's plus-one eigenvector, or the preparation
    // names the wrong basis, or the wrong sign within it.
    let applied: Vec<Complex64> = observable
        .iter()
        .map(|row| {
            row.iter()
                .zip(&state)
                .map(|(entry, amplitude)| entry * amplitude)
                .sum()
        })
        .collect();
    for index in 0..state.len() {
        assert!(
            (applied[index] - state[index]).norm() < TOLERANCE,
            "{}: prepared state is not a plus-one eigenstate",
            gate.name()
        );
        if (0..width)
            .any(|spectator| spectator != qubit as usize && bit(index, spectator, width) != 0)
        {
            assert!(
                state[index].norm() < TOLERANCE,
                "a spectator left its zero state"
            );
        }
    }
}

#[test]
fn preparing_x_preserves_a_nonzero_operand_and_spectators() {
    check_placed_preparation(StandardGate::PrepareX, &pauli_x(), 2, 4);
}

#[test]
fn preparing_y_preserves_a_nonzero_operand_and_spectators() {
    check_placed_preparation(StandardGate::PrepareY, &pauli_y(), 2, 4);
}

#[test]
fn preparing_x_leaves_the_x_eigenstate() {
    check_preparation(StandardGate::PrepareX, &pauli_x());
}

#[test]
fn preparing_y_leaves_the_y_eigenstate() {
    check_preparation(StandardGate::PrepareY, &pauli_y());
}

#[test]
fn a_rewrite_only_produces_gates_that_need_no_rewriting() {
    // Without this, a rewrite could name a gate that itself has no intrinsic,
    // and lowering would fail late instead of never.
    for gate in StandardGate::all() {
        let qubits: Vec<u32> = (0..u32::from(gate.qubit_arity())).collect();
        let angles = vec![0.5; usize::from(gate.param_arity())];
        let Some(steps) = decompose(gate, &qubits, &angles) else {
            continue;
        };
        for step in steps {
            assert!(
                matches!(gate_lowering(step.gate), GateLowering::Intrinsic(_)),
                "{} rewrites to {}, which has no intrinsic",
                gate.name(),
                step.gate.name()
            );
        }
    }
}

#[test]
fn a_rewrite_stays_within_the_operands_of_the_gate_it_replaces() {
    // Touching a qubit outside the gate's own operands would corrupt an
    // unrelated part of the circuit.
    for gate in StandardGate::all() {
        let arity = u32::from(gate.qubit_arity());
        let qubits: Vec<u32> = (0..arity).collect();
        let angles = vec![0.5; usize::from(gate.param_arity())];
        let Some(steps) = decompose(gate, &qubits, &angles) else {
            continue;
        };
        for step in steps {
            for qubit in step.qubits {
                assert!(
                    qubit < arity,
                    "{} rewrites onto qubit {qubit}, outside its own operands",
                    gate.name()
                );
            }
        }
    }
}

#[test]
fn a_rewrite_uses_the_operands_it_was_given() {
    // The rewrite has to follow the qubits the gate was applied to, not the
    // positions used when it was written.
    let steps = decompose(StandardGate::Cswap, &[7, 3, 5], &[]).expect("cswap should rewrite");
    let touched: Vec<Vec<u32>> = steps.into_iter().map(|step| step.qubits).collect();

    assert_eq!(touched, vec![vec![5, 3], vec![7, 3, 5], vec![5, 3]]);
}

#[test]
fn exactly_the_gates_without_an_intrinsic_are_rewritten() {
    let rewritten: Vec<&str> = StandardGate::all()
        .into_iter()
        .filter(|gate| {
            let qubits: Vec<u32> = (0..u32::from(gate.qubit_arity())).collect();
            let angles = vec![0.5; usize::from(gate.param_arity())];
            decompose(*gate, &qubits, &angles).is_some()
        })
        .map(StandardGate::name)
        .collect();

    assert_eq!(
        rewritten,
        vec![
            "sxdg",
            "cswap",
            "crx",
            "cry",
            "crz",
            "measure_x",
            "measure_y",
            "prepare_x",
            "prepare_y",
        ]
    );
}
