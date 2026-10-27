// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Helpers for testing measurement in the X, Y and Z bases with different lazily queued gates.

use super::{FlushLevel, SparseStateSim};
use qsc_data_structures::measurement::OneQubitMeasurement;

/// Lazily queued single-qubit gates used to set up the pending state before a Pauli measurement.
#[derive(Clone, Copy, Debug)]
pub(super) enum Pending {
    H,
    Rx(f64),
    Ry(f64),
}

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

/// Maps |z⟩ in the computational basis to |b⟩ in the eigenbasis of `pauli`.
fn to_basis(sim: &mut SparseStateSim, q: usize, pauli: OneQubitMeasurement) {
    match pauli {
        OneQubitMeasurement::Z => {}
        OneQubitMeasurement::X => sim.h(q),
        OneQubitMeasurement::Y => {
            sim.h(q);
            sim.s(q);
        }
    }
}

/// Maps |b⟩ in the eigenbasis of `pauli` to |z⟩ in the computational Z basis.
fn from_basis(sim: &mut SparseStateSim, q: usize, pauli: OneQubitMeasurement) {
    match pauli {
        OneQubitMeasurement::Z => {}
        OneQubitMeasurement::X => sim.h(q),
        OneQubitMeasurement::Y => {
            sim.sadj(q);
            sim.h(q);
        }
    }
}

/// Prepares the Bell pair (|00⟩ + |11⟩)/√2 with each qubit in the eigenbasis of its Pauli in `paulis`,
/// leaves `gates` pending on both qubits without changing the logical state, then checks that the
/// two `measure_pauli` results agree, are repeatable, and leave each qubit in the matching eigenstate.
pub(super) fn check_measure_pauli(paulis: [OneQubitMeasurement; 2], gates: &[Pending]) {
    let mut sim = SparseStateSim::default();
    sim.set_rng_seed(42);

    let qs = [sim.allocate(), sim.allocate()];
    sim.h(qs[0]);
    sim.mcx(&[qs[0]], qs[1]);

    for (q, pauli) in qs.into_iter().zip(paulis) {
        to_basis(&mut sim, q, pauli);
        // Undo, flush and redo the gates so they are left pending on an unchanged state.
        apply_pending(&mut sim, q, gates, true);
        sim.flush_queue(&[q], FlushLevel::HRxRy);
        apply_pending(&mut sim, q, gates, false);
    }

    let res = sim.measure_single_pauli(qs[0], paulis[0]);

    // For Bell state, both qubits should have the same measurement result.
    assert_eq!(sim.measure_single_pauli(qs[1], paulis[1]), res);

    // The qubits from Bell state are left is the same eigenstates post-measurement.
    for (q, pauli) in qs.into_iter().zip(paulis) {
        assert_eq!(sim.measure_single_pauli(q, pauli), res);
        from_basis(&mut sim, q, pauli);
        let expected = if res { 1.0 } else { 0.0 };
        assert!((sim.joint_probability(&[q]) - expected).abs() <= 1e-10,);
    }
}
