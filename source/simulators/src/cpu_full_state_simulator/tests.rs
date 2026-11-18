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
