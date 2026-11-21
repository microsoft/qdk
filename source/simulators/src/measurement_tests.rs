// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for measurement of one and two qubits in the X, Y and Z bases, run on each
//! `Simulator` implementation.

use crate::{
    MeasurementResult, QubitID, Simulator, cpu_full_state_simulator::FullStateSimulator,
    noise_config::CumulativeNoiseConfig, stabilizer_simulator::StabilizerSimulator,
};
use qsc_data_structures::measurement::{JointMeasurementBasis, MeasurementBasis};
use std::sync::Arc;

/// Permutations of bools representing all possible two-qubit states.
const ALL_BITS: [[bool; 2]; 4] = [[false, false], [false, true], [true, false], [true, true]];

fn new_sim<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>(
    num_qubits: usize,
    num_results: usize,
    seed: u32,
) -> S {
    S::new(
        num_qubits,
        num_results,
        seed,
        Arc::new(CumulativeNoiseConfig::default()),
    )
}

fn to_result(bit: bool) -> MeasurementResult {
    if bit {
        MeasurementResult::One
    } else {
        MeasurementResult::Zero
    }
}

fn check_mx_on_x_eigenstates_is_deterministic<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>() {
    let mut sim = new_sim::<S>(2, 2, 0);
    sim.h(0); // |+>
    sim.x(1);
    sim.h(1); // |->
    sim.mx(0, 0);
    sim.mx(1, 1);
    assert_eq!(
        sim.measurements(),
        [MeasurementResult::Zero, MeasurementResult::One]
    );
}

#[test]
fn mx_on_x_eigenstates_is_deterministic() {
    check_mx_on_x_eigenstates_is_deterministic::<FullStateSimulator>();
    check_mx_on_x_eigenstates_is_deterministic::<StabilizerSimulator>();
}

fn check_my_on_y_eigenstates_is_deterministic<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>() {
    let mut sim = new_sim::<S>(2, 2, 0);
    sim.h(0);
    sim.s(0); // |i>
    sim.h(1);
    sim.s_adj(1); // |-i>
    sim.my(0, 0);
    sim.my(1, 1);
    assert_eq!(
        sim.measurements(),
        [MeasurementResult::Zero, MeasurementResult::One]
    );
}

#[test]
fn my_on_y_eigenstates_is_deterministic() {
    check_my_on_y_eigenstates_is_deterministic::<FullStateSimulator>();
    check_my_on_y_eigenstates_is_deterministic::<StabilizerSimulator>();
}

fn check_mx_leaves_qubit_in_measured_eigenstate<
    S: Simulator<Noise = Arc<CumulativeNoiseConfig>>,
>() {
    for seed in 8..12 {
        let mut sim = new_sim::<S>(1, 3, seed);
        sim.mx(0, 0);
        sim.mx(0, 1);
        sim.h(0);
        sim.mz(0, 2);
        let m = sim.measurements();
        assert_eq!(m[0], m[1], "repeated MX should agree (seed {seed})");
        assert_eq!(m[0], m[2], "MX should not reset the qubit (seed {seed})");
    }
}

#[test]
fn mx_leaves_qubit_in_measured_eigenstate() {
    check_mx_leaves_qubit_in_measured_eigenstate::<FullStateSimulator>();
    check_mx_leaves_qubit_in_measured_eigenstate::<StabilizerSimulator>();
}

fn check_my_leaves_qubit_in_measured_eigenstate<
    S: Simulator<Noise = Arc<CumulativeNoiseConfig>>,
>() {
    for seed in 8..12 {
        let mut sim = new_sim::<S>(1, 3, seed);
        sim.my(0, 0);
        sim.my(0, 1);
        sim.s_adj(0);
        sim.h(0);
        sim.mz(0, 2);
        let m = sim.measurements();
        assert_eq!(m[0], m[1], "repeated MY should agree (seed {seed})");
        assert_eq!(m[0], m[2], "MY should not reset the qubit (seed {seed})");
    }
}

#[test]
fn my_leaves_qubit_in_measured_eigenstate() {
    check_my_leaves_qubit_in_measured_eigenstate::<FullStateSimulator>();
    check_my_leaves_qubit_in_measured_eigenstate::<StabilizerSimulator>();
}

fn check_mz_on_basis_states_is_deterministic<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>() {
    let mut sim = new_sim::<S>(2, 2, 0);
    sim.x(1);
    sim.mz(0, 0);
    sim.mz(1, 1);
    assert_eq!(
        sim.measurements(),
        [MeasurementResult::Zero, MeasurementResult::One]
    );
}

#[test]
fn mz_on_basis_states_is_deterministic() {
    check_mz_on_basis_states_is_deterministic::<FullStateSimulator>();
    check_mz_on_basis_states_is_deterministic::<StabilizerSimulator>();
}

/// Prepares `q` in the eigenstate of `pauli` with eigenvalue (-1)^`bit`.
fn prepare_eigenstate<S: Simulator>(sim: &mut S, q: QubitID, pauli: MeasurementBasis, bit: bool) {
    if bit {
        sim.x(q);
    }
    match pauli {
        MeasurementBasis::Z => {}
        MeasurementBasis::X => sim.h(q),
        MeasurementBasis::Y => {
            sim.h(q);
            sim.s(q);
        }
    }
}

/// Prepares each product of eigenstates of the single-qubit Paulis in each joint basis, and checks
/// that the joint measurement returns exactly the parity of the eigenvalues and is repeatable.
fn check_joint_measurements_on_eigenstates_are_deterministic<
    S: Simulator<Noise = Arc<CumulativeNoiseConfig>>,
>() {
    for basis in [
        JointMeasurementBasis::XX,
        JointMeasurementBasis::YY,
        JointMeasurementBasis::ZZ,
        JointMeasurementBasis::ZY,
    ] {
        let paulis = match basis {
            JointMeasurementBasis::XX => [MeasurementBasis::X, MeasurementBasis::X],
            JointMeasurementBasis::YY => [MeasurementBasis::Y, MeasurementBasis::Y],
            JointMeasurementBasis::ZZ => [MeasurementBasis::Z, MeasurementBasis::Z],
            JointMeasurementBasis::ZY => [MeasurementBasis::Z, MeasurementBasis::Y],
        };

        for bits in ALL_BITS {
            let mut sim = new_sim::<S>(2, 2, 0);
            for (q, (pauli, bit)) in paulis.into_iter().zip(bits).enumerate() {
                prepare_eigenstate(&mut sim, q, pauli, bit);
            }
            for result_id in 0..2 {
                match basis {
                    JointMeasurementBasis::XX => sim.mxx(0, 1, result_id),
                    JointMeasurementBasis::YY => sim.myy(0, 1, result_id),
                    JointMeasurementBasis::ZZ => sim.mzz(0, 1, result_id),
                    JointMeasurementBasis::ZY => sim.mzy(0, 1, result_id),
                }
            }
            let parity = to_result(bits[0] ^ bits[1]);
            assert_eq!(sim.measurements(), [parity, parity], "{basis:?}, {bits:?}");
        }
    }
}

#[test]
fn joint_measurements_on_eigenstates_are_deterministic() {
    check_joint_measurements_on_eigenstates_are_deterministic::<FullStateSimulator>();
    check_joint_measurements_on_eigenstates_are_deterministic::<StabilizerSimulator>();
}

/// Each Bell state is a joint eigenstate of XX, YY and ZZ, so measuring one parity must not disturb
/// the others: a measurement that collapsed the superposition would make the later parities random.
fn check_joint_measurements_on_bell_states<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>() {
    let mut sim = new_sim::<S>(2, 12, 0);
    let measure_all = |sim: &mut S, first_result: usize| {
        sim.mxx(0, 1, first_result);
        sim.myy(0, 1, first_result + 1);
        sim.mzz(0, 1, first_result + 2);
    };

    // Bell state |Φ+⟩ = (|00⟩ + |11⟩) / √2.
    sim.h(0);
    sim.cx(0, 1);
    measure_all(&mut sim, 0);

    // Bell state |Φ-⟩ = (|00⟩ - |11⟩) / √2.
    sim.z(0);
    measure_all(&mut sim, 3);

    // Bell state |Ψ-⟩ = (|01⟩ - |10⟩) / √2.
    sim.x(1);
    measure_all(&mut sim, 6);

    // Bell state |Ψ+⟩ = (|01⟩ + |10⟩) / √2.
    sim.z(0);
    measure_all(&mut sim, 9);

    // Expected (XX, YY, ZZ) parities for each Bell state, in order.
    let expected = [
        [false, true, false],
        [true, false, false],
        [true, true, true],
        [false, false, true],
    ]
    .into_iter()
    .flatten()
    .map(to_result)
    .collect::<Vec<_>>();
    assert_eq!(sim.measurements(), expected.as_slice());
}

#[test]
fn joint_measurements_on_bell_states() {
    check_joint_measurements_on_bell_states::<FullStateSimulator>();
    check_joint_measurements_on_bell_states::<StabilizerSimulator>();
}
