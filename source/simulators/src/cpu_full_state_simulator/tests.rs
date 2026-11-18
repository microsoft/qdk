// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
use super::*;

#[cfg(test)]
use crate::noise_config::NoiseConfig;

#[cfg(test)]
fn new_sim(num_qubits: usize, num_results: usize, seed: u32) -> FullStateSimulator {
    let noise: Arc<CumulativeNoiseConfig> = Arc::new(<NoiseConfig<f64, f64>>::NOISELESS.into());
    FullStateSimulator::new(num_qubits, num_results, seed, noise)
}

/// Permutations of bools representing all possible two-qubit states.
#[cfg(test)]
const ALL_BITS: [[bool; 2]; 4] = [[false, false], [false, true], [true, false], [true, true]];

#[test]
fn mx_on_x_eigenstates_is_deterministic() {
    let mut sim = new_sim(2, 2, 0);
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
fn my_on_y_eigenstates_is_deterministic() {
    let mut sim = new_sim(2, 2, 0);
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
fn mx_leaves_qubit_in_measured_eigenstate() {
    for seed in 0..16 {
        let mut sim = new_sim(1, 3, seed);
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
fn my_leaves_qubit_in_measured_eigenstate() {
    for seed in 0..16 {
        let mut sim = new_sim(1, 3, seed);
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
fn mz_on_basis_states_is_deterministic() {
    let mut sim = new_sim(2, 2, 0);
    sim.x(1);
    sim.mz(0, 0);
    sim.mz(1, 1);
    assert_eq!(
        sim.measurements(),
        [MeasurementResult::Zero, MeasurementResult::One]
    );
}

/// Prepares `q` in the eigenstate of `pauli` with eigenvalue (-1)^`bit`.
#[cfg(test)]
fn prepare_eigenstate(
    sim: &mut FullStateSimulator,
    q: QubitID,
    pauli: MeasurementBasis,
    bit: bool,
) {
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

/// Measures each joint basis on the products of its single-qubit eigenstates.
#[test]
fn two_qubit_measurements_on_eigenstates_are_deterministic() {
    for basis in [
        JointMeasurementBasis::XX,
        JointMeasurementBasis::YY,
        JointMeasurementBasis::ZZ,
        JointMeasurementBasis::ZY,
    ] {
        // Prepares each product of eigenstates of the single-qubit Paulis in `bases`, and checks that the joint
        // measurement returns exactly the parity of the eigenvalues, is repeatable, and leaves both qubits in the
        // same eigenstates.
        let paulis = match basis {
            JointMeasurementBasis::XX => [MeasurementBasis::X, MeasurementBasis::X],
            JointMeasurementBasis::YY => [MeasurementBasis::Y, MeasurementBasis::Y],
            JointMeasurementBasis::ZZ => [MeasurementBasis::Z, MeasurementBasis::Z],
            JointMeasurementBasis::ZY => [MeasurementBasis::Z, MeasurementBasis::Y],
        };

        for bits in ALL_BITS {
            let mut sim = new_sim(2, 2, 0);
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
            let parity = match bits[0] ^ bits[1] {
                false => MeasurementResult::Zero,
                true => MeasurementResult::One,
            };
            assert_eq!(sim.measurements(), [parity, parity], "{basis:?}, {bits:?}");
        }
    }
}
