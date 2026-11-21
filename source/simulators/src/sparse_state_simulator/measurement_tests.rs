// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for measurement of one and two qubits in the X, Y and Z bases, with different lazily queued gates.

use super::{FlushLevel, SparseStateSim, nearly_zero::NearlyZero};
use qsc_data_structures::measurement::{JointMeasurementBasis, MeasurementBasis};
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

/// Lazily queued single-qubit gates used to set up the pending state before a Pauli measurement.
#[derive(Clone, Copy, Debug)]
enum Pending {
    H,
    Rx(f64),
    Ry(f64),
}

/// Combinations of pending H, Rx and Ry that exercise each shortcut in the X and Y measurement preparation.
const PENDING_GATES: &[&[Pending]] = &[
    &[],
    &[Pending::H],
    &[Pending::Ry(FRAC_PI_2)],
    &[Pending::Ry(-FRAC_PI_2)],
    &[Pending::Ry(0.3)],
    &[Pending::Rx(0.3)],
    &[Pending::Rx(FRAC_PI_2)],
    &[Pending::Rx(-FRAC_PI_2)],
    &[Pending::Ry(FRAC_PI_4), Pending::Ry(FRAC_PI_4)],
    &[Pending::Rx(FRAC_PI_4), Pending::Rx(FRAC_PI_4)],
    &[Pending::H, Pending::Ry(FRAC_PI_2)],
    &[Pending::H, Pending::Ry(-FRAC_PI_2)],
    &[Pending::H, Pending::Ry(0.42)],
    &[Pending::Rx(0.42), Pending::Ry(FRAC_PI_2)],
    &[Pending::Rx(0.42), Pending::Ry(-FRAC_PI_2)],
    &[Pending::Rx(FRAC_PI_2), Pending::Ry(-PI)],
    &[Pending::Rx(-FRAC_PI_2), Pending::Ry(PI)],
    &[Pending::Rx(-PI), Pending::Ry(FRAC_PI_2)],
    &[Pending::Rx(PI), Pending::Ry(-FRAC_PI_2)],
    &[Pending::Rx(0.6), Pending::Ry(0.9)],
];

/// Permutations of bools representing all possible two-qubit states.
const ALL_BITS: [[bool; 2]; 4] = [[false, false], [false, true], [true, false], [true, true]];

fn apply_pending(sim: &mut SparseStateSim, q: usize, gates: &[Pending], inverse: bool) {
    let sign = if inverse { -1.0 } else { 1.0 };
    let mut ordered = gates.to_vec();
    if inverse {
        ordered.reverse();
    }
    for gate in ordered {
        match gate {
            Pending::H => sim.h(q),
            Pending::Rx(theta) => sim.rx(sign * theta, q),
            Pending::Ry(theta) => sim.ry(sign * theta, q),
        }
    }
}

/// Undoes, flushes and redoes `gates` so they are left pending on an unchanged state.
fn leave_pending(sim: &mut SparseStateSim, q: usize, gates: &[Pending]) {
    apply_pending(sim, q, gates, true);
    sim.flush_queue(&[q], FlushLevel::HRxRy);
    apply_pending(sim, q, gates, false);
}

/// Maps |z⟩ in the computational basis to |b⟩ in the eigenbasis of `pauli`.
fn to_basis(sim: &mut SparseStateSim, q: usize, pauli: MeasurementBasis) {
    match pauli {
        MeasurementBasis::Z => {}
        MeasurementBasis::X => sim.h(q),
        MeasurementBasis::Y => {
            sim.h(q);
            sim.s(q);
        }
    }
}

/// Checks that `q` is in the eigenstate of `pauli` with eigenvalue (-1)^`bit`, without changing the state.
fn assert_eigenstate(sim: &mut SparseStateSim, q: usize, pauli: MeasurementBasis, bit: bool) {
    match pauli {
        MeasurementBasis::X => assert_eq!(sim.qubit_is_plus(q), !bit),
        MeasurementBasis::Y => assert_eq!(sim.qubit_is_plus_i(q), !bit),
        MeasurementBasis::Z => assert_eq!(sim.qubit_is_zero(q), !bit),
    }
}

/// Measures a qubit in each Pauli basis with different combinations of pending H, Rx and Ry.
#[test]
fn test_measure_single_qubit() {
    for basis in [
        MeasurementBasis::Z,
        MeasurementBasis::X,
        MeasurementBasis::Y,
    ] {
        // Prepares each eigenstate of `basis`, leaves each combination of `PENDING_GATES` pending on it, and checks
        // that `measure_single_qubit` returns exactly the eigenvalue and leaves the qubit in the same eigenstate.
        let mut sim = SparseStateSim::default();

        for bit in [false, true] {
            let q = sim.allocate();
            if bit {
                sim.x(q);
            }
            to_basis(&mut sim, q, basis);
            for gates in PENDING_GATES {
                leave_pending(&mut sim, q, gates);
                let res = sim.measure_single_qubit(q, basis);
                assert_eq!(res, bit, "{basis:?}, {gates:?}");
                assert_eigenstate(&mut sim, q, basis, res);
            }
            sim.release(q);
        }
    }
}

/// Measures a Bell pair in each Pauli basis with different combinations of pending H, Rx and Ry.
#[test]
fn test_measure_single_qubit_with_bell_pair() {
    for b1 in [
        MeasurementBasis::Z,
        MeasurementBasis::X,
        MeasurementBasis::Y,
    ] {
        for b2 in [
            MeasurementBasis::Z,
            MeasurementBasis::X,
            MeasurementBasis::Y,
        ] {
            // For each combination of `PENDING_GATES`, prepare the Bell pair (|00⟩ + |11⟩)/√2 with each qubit in the
            // eigenbasis of its Pauli in `bases` and the gates pending on both qubits, then check that the two
            // `measure_single_qubit` results agree, are repeatable, and leave each qubit in the matching eigenstate.
            let bases = [b1, b2];
            let mut sim = SparseStateSim::default();

            for gates in PENDING_GATES {
                let qs = [sim.allocate(), sim.allocate()];
                sim.h(qs[0]);
                sim.mcx(&[qs[0]], qs[1]);

                for (q, pauli) in qs.into_iter().zip(bases) {
                    to_basis(&mut sim, q, pauli);
                    leave_pending(&mut sim, q, gates);
                }
                let res = sim.measure_single_qubit(qs[0], bases[0]);

                // For Bell state, both qubits should have the same measurement result.
                assert_eq!(
                    sim.measure_single_qubit(qs[1], bases[1]),
                    res,
                    "{bases:?}, {gates:?}"
                );

                // The qubits from Bell state are left in the same eigenstates post-measurement.
                for (q, pauli) in qs.into_iter().zip(bases) {
                    assert_eigenstate(&mut sim, q, pauli, res);
                }
                sim.release(qs[0]);
                sim.release(qs[1]);
            }
        }
    }
}

/// Measures in each joint basis with different combinations of pending H, Rx and Ry on both qubits.
#[test]
fn test_measure_joint_qubits() {
    for basis in [
        JointMeasurementBasis::XX,
        JointMeasurementBasis::YY,
        JointMeasurementBasis::ZZ,
        JointMeasurementBasis::ZY,
    ] {
        // Prepares each product of eigenstates of the single-qubit Paulis in `bases`, leaves each combination of
        // `PENDING_GATES` pending on both qubits, and checks that `measure_two_qubits` returns exactly the parity
        // of the eigenvalues, is repeatable, and leaves both qubits in the same eigenstates.
        let paulis = match basis {
            JointMeasurementBasis::XX => [MeasurementBasis::X, MeasurementBasis::X],
            JointMeasurementBasis::YY => [MeasurementBasis::Y, MeasurementBasis::Y],
            JointMeasurementBasis::ZZ => [MeasurementBasis::Z, MeasurementBasis::Z],
            JointMeasurementBasis::ZY => [MeasurementBasis::Z, MeasurementBasis::Y],
        };
        let mut sim = SparseStateSim::default();

        for bits in ALL_BITS {
            let qs = [sim.allocate(), sim.allocate()];
            for ((q, pauli), bit) in qs.into_iter().zip(paulis).zip(bits) {
                if bit {
                    sim.x(q);
                }
                to_basis(&mut sim, q, pauli);
            }
            let parity = bits[0] ^ bits[1];
            for gates in PENDING_GATES {
                for q in qs {
                    leave_pending(&mut sim, q, gates);
                }
                assert_eq!(
                    sim.measure_joint_qubits(qs[0], qs[1], basis),
                    parity,
                    "{basis:?}, {bits:?}, {gates:?}"
                );
                assert_eq!(
                    sim.measure_joint_qubits(qs[0], qs[1], basis),
                    parity,
                    "{basis:?}, {bits:?}, {gates:?}"
                );
                for ((q, pauli), bit) in qs.into_iter().zip(paulis).zip(bits) {
                    assert_eigenstate(&mut sim, q, pauli, bit);
                }
            }
            sim.release(qs[0]);
            sim.release(qs[1]);
        }
    }
}

/// Verify two-qubit measurements on each Bell state, which is an eigenstate of XX, YY and ZZ, and that
/// measuring one parity doesn't disturb the others.
#[test]
fn test_symmetric_joint_measurements_bell_states() {
    let mut sim = SparseStateSim::default();
    let q0 = sim.allocate();
    let q1 = sim.allocate();

    // Bell state |Φ+⟩ = (|00⟩ + |11⟩) / √2.
    sim.h(q0);
    sim.mcx(&[q0], q1);
    assert!(!sim.measure_joint_qubits(q0, q1, JointMeasurementBasis::XX));
    assert!(sim.measure_joint_qubits(q0, q1, JointMeasurementBasis::YY));
    assert!(!sim.measure_joint_qubits(q0, q1, JointMeasurementBasis::ZZ));

    // Bell state |Φ-⟩ = (|00⟩ - |11⟩) / √2.
    sim.z(q0);
    assert!(sim.measure_joint_qubits(q0, q1, JointMeasurementBasis::XX));
    assert!(!sim.measure_joint_qubits(q0, q1, JointMeasurementBasis::YY));
    assert!(!sim.measure_joint_qubits(q0, q1, JointMeasurementBasis::ZZ));

    // Bell state |Ψ-⟩ = (|01⟩ - |10⟩) / √2.
    sim.x(q1);
    assert!(sim.measure_joint_qubits(q0, q1, JointMeasurementBasis::XX));
    assert!(sim.measure_joint_qubits(q0, q1, JointMeasurementBasis::YY));
    assert!(sim.measure_joint_qubits(q0, q1, JointMeasurementBasis::ZZ));

    // Bell state |Ψ+⟩ = (|01⟩ + |10⟩) / √2.
    sim.z(q0);
    assert!(!sim.measure_joint_qubits(q0, q1, JointMeasurementBasis::XX));
    assert!(!sim.measure_joint_qubits(q0, q1, JointMeasurementBasis::YY));
    assert!(sim.measure_joint_qubits(q0, q1, JointMeasurementBasis::ZZ));

    assert!((sim.joint_probability(&[q0]) - 0.5).is_nearly_zero());
    assert!((sim.joint_probability(&[q1]) - 0.5).is_nearly_zero());
    sim.release(q1);
    sim.release(q0);
}
