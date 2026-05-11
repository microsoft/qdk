// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    backend::{Backend, SparseSim},
    noise::PauliNoise,
    state::{fmt_complex, format_state_id},
    val,
};
use expect_test::{Expect, expect};
use num_bigint::BigUint;
use num_complex::Complex;
use qdk_simulators::noise_config::{NoiseConfig, NoiseTable, encode_pauli};
use std::fmt::Write;

#[test]
fn pauli_noise() {
    let noise = PauliNoise::from_probabilities(0.0, 0.0, 0.0);
    assert!(
        noise
            .expect("noiseless Pauli noise should be constructable.")
            .is_noiseless(),
        "Expected noiseless noise."
    );
    let noise = PauliNoise::from_probabilities(1e-5, 0.0, 0.0);
    assert!(
        !noise
            .expect("bit flip noise with probability 1e-5 should be constructable.")
            .is_noiseless(),
        "Expected noise to be noisy."
    );
    let noise = PauliNoise::from_probabilities(1.0, 0.0, 0.0);
    assert!(
        !noise
            .expect("bit flip noise with probability 1 should be constructable.")
            .is_noiseless(),
        "Expected noise to be noisy."
    );
    let noise = PauliNoise::from_probabilities(0.01, 0.01, 0.01)
        .expect("depolarizing noise with probability 0.01 should be constructable..");
    assert!(!noise.is_noiseless(), "Expected noise to be noisy.");
    assert!(
        0.0 <= noise.distribution[0]
            && noise.distribution[0] <= noise.distribution[1]
            && noise.distribution[1] <= noise.distribution[2]
            && noise.distribution[2] <= 1.1,
        "Expected non-decreasing noise distribution."
    );
    let _ = PauliNoise::from_probabilities(-1e-10, 0.1, 0.1)
        .expect_err("pauli noise with probabilities -1e-10, 0.1, 0.1 should result in error.");
    let _ = PauliNoise::from_probabilities(1.0 + -1e-10, 0.1, 0.1)
        .expect_err("pauli noise with probabilities 1.0+1e-10, 0.1, 0.1 should result in error.");
    let _ = PauliNoise::from_probabilities(0.3, 0.4, 0.5)
        .expect_err("pauli noise with probabilities 0.3, 0.4, 0.5 should result in error.");
}

#[test]
fn noisy_simulator() {
    let sim = SparseSim::new();
    assert!(sim.is_noiseless(), "Expected noiseless simulator.");

    let noise = PauliNoise::from_probabilities(0.0, 0.0, 0.0)
        .expect("noiseless Pauli noise should be constructable.");
    let sim = SparseSim::new_with_noise(&noise);
    assert!(sim.is_noiseless(), "Expected noiseless simulator.");

    let noise = PauliNoise::from_probabilities(1e-10, 0.0, 0.0)
        .expect("1e-10, 0.0, 0.0 Pauli noise should be constructable.");
    let sim = SparseSim::new_with_noise(&noise);
    assert!(!sim.is_noiseless(), "Expected noisy simulator.");

    let noise = PauliNoise::from_probabilities(0.0, 0.0, 1e-10)
        .expect("0.0, 0.0, 1e-10 Pauli noise should be constructable.");
    let sim = SparseSim::new_with_noise(&noise);
    assert!(!sim.is_noiseless(), "Expected noisy simulator.");
}

#[test]
fn noiseless_gate() {
    let noise = PauliNoise::from_probabilities(0.0, 0.0, 0.0)
        .expect("noiseless Pauli noise should be constructable.");
    let mut sim = SparseSim::new_with_noise(&noise);
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    for _ in 0..100 {
        let _ = sim.x(q);
        let res1 = sim
            .m(q)
            .expect("sparse simulator is infinite")
            .unwrap_bool();
        assert!(res1, "Expected True without noise.");
        let _ = sim.x(q);
        let res2 = sim
            .m(q)
            .expect("sparse simulator is infinite")
            .unwrap_bool();
        assert!(!res2, "Expected False without noise.");
    }
    assert!(
        sim.qubit_release(q).expect("sparse simulator is infinite"),
        "Expected correct qubit state on release."
    );
}

#[test]
fn bitflip_measurement() {
    let noise = PauliNoise::from_probabilities(1.0, 0.0, 0.0)
        .expect("bit flip noise with probability 100% should be constructable.");
    let mut sim = SparseSim::new_with_noise(&noise);
    assert!(!sim.is_noiseless(), "Expected noisy simulator.");
    let q = sim.qubit_allocate().expect("sparse simulator is infinite"); // Allocation is noiseless even with noise.
    for _ in 0..100 {
        let res1 = sim
            .m(q)
            .expect("sparse simulator is infinite")
            .unwrap_bool();
        assert!(res1, "Expected True for 100% bit flip noise.");
        let res2 = sim
            .m(q)
            .expect("sparse simulator is infinite")
            .unwrap_bool();
        assert!(!res2, "Expected False for 100% bit flip noise.");
    }
    assert!(
        sim.qubit_release(q).expect("sparse simulator is infinite"),
        "Expected correct qubit state on release."
    );
}

#[test]
fn noisy_measurement() {
    let noise = PauliNoise::from_probabilities(0.3, 0.0, 0.0)
        .expect("bit flip noise with probability 100% should be constructable.");
    let mut sim = SparseSim::new_with_noise(&noise);
    assert!(!sim.is_noiseless(), "Expected noisy simulator.");
    sim.set_seed(Some(0));
    let mut true_count = 0;
    for _ in 0..1000 {
        let q = sim.qubit_allocate().expect("sparse simulator is infinite"); // Allocation is noiseless even with noise.
        // sim.m sometimes applies X before measuring
        if sim
            .m(q)
            .expect("sparse simulator is infinite")
            .unwrap_bool()
        {
            true_count += 1;
        }
        sim.qubit_release(q).expect("sparse simulator is infinite");
    }
    assert!(
        true_count > 200 && true_count < 400,
        "Expected about 30% bit flip noise."
    );
}

pub fn state_to_string(input: &(Vec<(BigUint, Complex<f64>)>, usize)) -> String {
    input
        .0
        .iter()
        .fold(String::new(), |mut output, (id, state)| {
            let _ = write!(
                output,
                "{}: {} ",
                format_state_id(id, input.1),
                fmt_complex(state)
            );
            output
        })
        .clone()
}

fn check_state(sim: &mut SparseSim, expected: &Expect) {
    let state = sim
        .capture_quantum_state()
        .expect("sparse simulator is infinite");
    expected.assert_eq(&state_to_string(&state));
}

#[test]
fn noisy_via_x() {
    let noise = PauliNoise::from_probabilities(1.0, 0.0, 0.0)
        .expect("bit flip noise with probability 100% should be constructable.");
    let mut sim = SparseSim::new_with_noise(&noise);
    assert!(!sim.is_noiseless(), "Expected noisy simulator.");
    let q = sim.qubit_allocate().expect("sparse simulator is infinite"); // Allocation is noiseless even with noise.
    check_state(&mut sim, &expect!["|0⟩: 1.0000+0.0000𝑖 "]);
    let _ = sim.x(q); // Followed by X. So, no op.
    check_state(&mut sim, &expect!["|0⟩: 1.0000+0.0000𝑖 "]);
    let _ = sim.y(q); // Followed by X.
    check_state(&mut sim, &expect!["|0⟩: 0.0000+1.0000𝑖 "]);
    let _ = sim.z(q); // Followed by X.
    check_state(&mut sim, &expect!["|1⟩: 0.0000+1.0000𝑖 "]);
}

#[test]
fn noisy_via_y() {
    let noise = PauliNoise::from_probabilities(0.0, 1.0, 0.0)
        .expect("0.0, 1.0, 0.0 Pauli noise should be constructable.");
    let mut sim = SparseSim::new_with_noise(&noise);
    assert!(!sim.is_noiseless(), "Expected noisy simulator.");
    let q = sim.qubit_allocate().expect("sparse simulator is infinite"); // Allocation is noiseless even with noise.
    check_state(&mut sim, &expect!["|0⟩: 1.0000+0.0000𝑖 "]);
    let _ = sim.x(q); // Followed by Y.
    check_state(&mut sim, &expect!["|0⟩: 0.0000−1.0000𝑖 "]);
    let _ = sim.y(q); // Followed by Y. So, no op.
    check_state(&mut sim, &expect!["|0⟩: 0.0000−1.0000𝑖 "]);
    let _ = sim.z(q); // Followed by Y.
    check_state(&mut sim, &expect!["|1⟩: 1.0000+0.0000𝑖 "]);
}

#[test]
fn noisy_via_z() {
    let noise = PauliNoise::from_probabilities(0.0, 0.0, 1.0)
        .expect("phase flip noise with probability 100% should be constructable.");
    let mut sim = SparseSim::new_with_noise(&noise);
    assert!(!sim.is_noiseless(), "Expected noisy simulator.");
    let q = sim.qubit_allocate().expect("sparse simulator is infinite"); // Allocation is noiseless even with noise.
    check_state(&mut sim, &expect!["|0⟩: 1.0000+0.0000𝑖 "]);
    let _ = sim.x(q); // Followed by Z.
    check_state(&mut sim, &expect!["|1⟩: −1.0000+0.0000𝑖 "]);
    let _ = sim.y(q); // Followed by Z.
    check_state(&mut sim, &expect!["|0⟩: 0.0000+1.0000𝑖 "]);
    let _ = sim.z(q); // Followed by Z. So, no op.
    check_state(&mut sim, &expect!["|0⟩: 0.0000+1.0000𝑖 "]);
}

#[test]
fn measure_without_loss_returns_value() {
    let mut sim = SparseSim::new();
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let res = sim.m(q).expect("sparse simulator is infinite");
    assert!(
        matches!(res, val::Result::Val(_)),
        "Expected measurement to return a result"
    );
}

#[test]
fn measure_with_loss_returns_loss() {
    let mut sim = SparseSim::new();
    sim.set_loss(1.0); // Set loss probability to 100%
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let res = sim.m(q).expect("sparse simulator is infinite");
    assert_eq!(
        res,
        val::Result::Loss,
        "Expected measurement with loss to return None"
    );
}

/// Creates a `NoiseConfig` where the given gate's `NoiseTable` has 100% probability
/// of the specified single-qubit Pauli fault, and all other gates are noiseless.
fn noise_config_with_single_qubit_fault(
    set_gate: impl FnOnce(&mut NoiseConfig<f64, f64>, NoiseTable<f64>),
    pauli: &str,
) -> NoiseConfig<f64, f64> {
    let mut config = NoiseConfig::NOISELESS;
    let table = NoiseTable {
        qubits: 1,
        pauli_strings: vec![encode_pauli(pauli)],
        probabilities: vec![1.0],
        loss: 0.0,
    };
    set_gate(&mut config, table);
    config
}

/// Creates a `NoiseConfig` where the given gate's `NoiseTable` has 100% probability
/// of the specified two-qubit Pauli fault, and all other gates are noiseless.
fn noise_config_with_two_qubit_fault(
    set_gate: impl FnOnce(&mut NoiseConfig<f64, f64>, NoiseTable<f64>),
    pauli: &str,
) -> NoiseConfig<f64, f64> {
    let mut config = NoiseConfig::NOISELESS;
    let table = NoiseTable {
        qubits: 2,
        pauli_strings: vec![encode_pauli(pauli)],
        probabilities: vec![1.0],
        loss: 0.0,
    };
    set_gate(&mut config, table);
    config
}

// Tests for single-qubit gates with CumulativeNoiseConfig

#[test]
fn noise_config_x_gate_with_x_fault() {
    // X gate followed by 100% X fault = identity (X * X = I)
    let config = noise_config_with_single_qubit_fault(|c, t| c.x = t, "X");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.x(q); // X then X fault => |0⟩
    check_state(&mut sim, &expect!["|0⟩: 1.0000+0.0000𝑖 "]);
}

#[test]
fn noise_config_x_gate_with_z_fault() {
    // X gate followed by 100% Z fault = ZX|0⟩ = Z|1⟩ = -|1⟩
    let config = noise_config_with_single_qubit_fault(|c, t| c.x = t, "Z");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.x(q);
    check_state(&mut sim, &expect!["|1⟩: −1.0000+0.0000𝑖 "]);
}

#[test]
fn noise_config_x_gate_with_y_fault() {
    // X gate followed by 100% Y fault = YX|0⟩ = Y|1⟩ = -i|0⟩
    let config = noise_config_with_single_qubit_fault(|c, t| c.x = t, "Y");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.x(q);
    check_state(&mut sim, &expect!["|0⟩: 0.0000−1.0000𝑖 "]);
}

#[test]
fn noise_config_h_gate_with_y_fault() {
    // H|0⟩ = |+⟩, then Y|+⟩ = i|−⟩
    let config = noise_config_with_single_qubit_fault(|c, t| c.h = t, "Y");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.h(q);
    check_state(
        &mut sim,
        &expect!["|0⟩: 0.0000+0.7071𝑖 |1⟩: 0.0000−0.7071𝑖 "],
    );
}

#[test]
fn noise_config_h_gate_with_z_fault() {
    // H|0⟩ = |+⟩, then Z|+⟩ = |−⟩
    let config = noise_config_with_single_qubit_fault(|c, t| c.h = t, "Z");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.h(q);
    check_state(
        &mut sim,
        &expect!["|0⟩: 0.7071+0.0000𝑖 |1⟩: −0.7071+0.0000𝑖 "],
    );
}

#[test]
fn noise_config_y_gate_with_y_fault() {
    // Y gate followed by 100% Y fault = Y*Y = I
    let config = noise_config_with_single_qubit_fault(|c, t| c.y = t, "Y");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.y(q);
    check_state(&mut sim, &expect!["|0⟩: 1.0000+0.0000𝑖 "]);
}

#[test]
fn noise_config_z_gate_with_x_fault() {
    // Z|0⟩ = |0⟩, then X|0⟩ = |1⟩
    let config = noise_config_with_single_qubit_fault(|c, t| c.z = t, "X");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.z(q);
    check_state(&mut sim, &expect!["|1⟩: 1.0000+0.0000𝑖 "]);
}

#[test]
fn noise_config_s_gate_with_x_fault() {
    // S|0⟩ = |0⟩ (S only adds phase to |1⟩), then X|0⟩ = |1⟩
    let config = noise_config_with_single_qubit_fault(|c, t| c.s = t, "X");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.s(q);
    check_state(&mut sim, &expect!["|1⟩: 1.0000+0.0000𝑖 "]);
}

#[test]
fn noise_config_s_gate_with_y_fault() {
    // S|0⟩ = |0⟩, then Y|0⟩ = i|1⟩
    let config = noise_config_with_single_qubit_fault(|c, t| c.s = t, "Y");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.s(q);
    check_state(&mut sim, &expect!["|1⟩: 0.0000+1.0000𝑖 "]);
}

#[test]
fn noise_config_t_gate_with_x_fault() {
    // T|0⟩ = |0⟩ (T only adds phase to |1⟩), then X|0⟩ = |1⟩
    let config = noise_config_with_single_qubit_fault(|c, t| c.t = t, "X");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.t(q);
    check_state(&mut sim, &expect!["|1⟩: 1.0000+0.0000𝑖 "]);
}

#[test]
fn noise_config_sadj_gate_with_y_fault() {
    // Sadj|0⟩ = |0⟩, then Y|0⟩ = i|1⟩
    let config = noise_config_with_single_qubit_fault(|c, t| c.s_adj = t, "Y");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.sadj(q);
    check_state(&mut sim, &expect!["|1⟩: 0.0000+1.0000𝑖 "]);
}

#[test]
fn noise_config_tadj_gate_with_y_fault() {
    // Tadj|0⟩ = |0⟩, then Y|0⟩ = i|1⟩
    let config = noise_config_with_single_qubit_fault(|c, t| c.t_adj = t, "Y");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.tadj(q);
    check_state(&mut sim, &expect!["|1⟩: 0.0000+1.0000𝑖 "]);
}

#[test]
fn noise_config_mz_with_x_fault() {
    // Measurement with 100% X fault: qubit in |0⟩, X is applied before measurement,
    // so it measures as True (|1⟩).
    let config = noise_config_with_single_qubit_fault(|c, t| c.mz = t, "X");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let res = sim
        .m(q)
        .expect("sparse simulator is infinite")
        .unwrap_bool();
    assert!(
        res,
        "Expected True: X fault flips |0⟩ to |1⟩ before measurement."
    );
}

#[test]
fn noise_config_mz_with_z_fault() {
    // Measurement with 100% Z fault: Z|0⟩ = |0⟩, so measurement is still False.
    let config = noise_config_with_single_qubit_fault(|c, t| c.mz = t, "Z");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let res = sim
        .m(q)
        .expect("sparse simulator is infinite")
        .unwrap_bool();
    assert!(
        !res,
        "Expected False: Z fault on |0⟩ doesn't change measurement outcome."
    );
}

// Tests for two-qubit gates with CumulativeNoiseConfig

#[test]
fn noise_config_cx_gate_with_xi_fault() {
    // CX(ctl, tgt) on |00⟩ = |00⟩, then XI fault: X on control, I on target => |10⟩
    let config = noise_config_with_two_qubit_fault(|c, t| c.cx = t, "XI");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let ctl = sim.qubit_allocate().expect("sparse simulator is infinite");
    let tgt = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.cx(ctl, tgt);
    check_state(&mut sim, &expect!["|10⟩: 1.0000+0.0000𝑖 "]);
}

#[test]
fn noise_config_cx_gate_with_ix_fault() {
    // CX(ctl, tgt) on |00⟩ = |00⟩, then IX fault: I on control, X on target => |01⟩
    let config = noise_config_with_two_qubit_fault(|c, t| c.cx = t, "IX");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let ctl = sim.qubit_allocate().expect("sparse simulator is infinite");
    let tgt = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.cx(ctl, tgt);
    check_state(&mut sim, &expect!["|01⟩: 1.0000+0.0000𝑖 "]);
}

#[test]
fn noise_config_cx_gate_with_xx_fault() {
    // CX(ctl, tgt) on |00⟩ = |00⟩, then XX fault: X on both => |11⟩
    let config = noise_config_with_two_qubit_fault(|c, t| c.cx = t, "XX");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let ctl = sim.qubit_allocate().expect("sparse simulator is infinite");
    let tgt = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.cx(ctl, tgt);
    check_state(&mut sim, &expect!["|11⟩: 1.0000+0.0000𝑖 "]);
}

#[test]
fn noise_config_cz_gate_with_xy_fault() {
    // CZ on |00⟩ = |00⟩, then XY fault: X on first, Y on second
    // X|0⟩ = |1⟩, Y|0⟩ = i|1⟩ => |1i1⟩
    let config = noise_config_with_two_qubit_fault(|c, t| c.cz = t, "XY");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q0 = sim.qubit_allocate().expect("sparse simulator is infinite");
    let q1 = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.cz(q0, q1);
    check_state(&mut sim, &expect!["|11⟩: 0.0000+1.0000𝑖 "]);
}

#[test]
fn noise_config_swap_gate_with_xx_fault() {
    // Prepare |10⟩, SWAP => |01⟩, then XX fault => |10⟩ again
    let config = noise_config_with_two_qubit_fault(|c, t| c.swap = t, "XX");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q0 = sim.qubit_allocate().expect("sparse simulator is infinite");
    let q1 = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.x(q0); // |10⟩ (x gate has no noise configured)
    let _ = sim.swap(q0, q1); // SWAP => |01⟩, then XX => |10⟩
    check_state(&mut sim, &expect!["|10⟩: 1.0000+0.0000𝑖 "]);
}

// Test that noise is only applied to the configured gate, not others

#[test]
fn noise_config_only_affects_configured_gate() {
    // Configure X fault only on H gate; X gate should be noiseless.
    let config = noise_config_with_single_qubit_fault(|c, t| c.h = t, "X");
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    // X gate has no noise configured, so X|0⟩ = |1⟩ without any fault
    let _ = sim.x(q);
    check_state(&mut sim, &expect!["|1⟩: 1.0000+0.0000𝑖 "]);
    // Now apply H (which has 100% X fault): H|1⟩ = |−⟩, then X|−⟩ = −|−⟩
    let _ = sim.h(q);
    check_state(
        &mut sim,
        &expect!["|0⟩: −0.7071+0.0000𝑖 |1⟩: 0.7071+0.0000𝑖 "],
    );
}

// Test loss via noise config

#[test]
fn noise_config_mz_with_loss() {
    let mut config = NoiseConfig::NOISELESS;
    config.mz = NoiseTable {
        qubits: 1,
        pauli_strings: vec![],
        probabilities: vec![],
        loss: 1.0,
    };
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let res = sim.m(q).expect("sparse simulator is infinite");
    assert_eq!(
        res,
        val::Result::Loss,
        "Expected measurement with 100% loss to return Loss"
    );
}

#[test]
fn noise_config_gate_loss_causes_measurement_loss() {
    // Configure 100% loss on X gate. After X, qubit is lost.
    // Measurement of a lost qubit should return Loss.
    let mut config = NoiseConfig::NOISELESS;
    config.x = NoiseTable {
        qubits: 1,
        pauli_strings: vec![],
        probabilities: vec![],
        loss: 1.0,
    };
    let mut sim = SparseSim::new_with_noise_config(config.into());
    let q = sim.qubit_allocate().expect("sparse simulator is infinite");
    let _ = sim.x(q);
    let res = sim.m(q).expect("sparse simulator is infinite");
    assert_eq!(
        res,
        val::Result::Loss,
        "Expected measurement after gate loss to return Loss"
    );
}
