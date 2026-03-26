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
    let q = sim.qubit_allocate();
    for _ in 0..100 {
        sim.x(q);
        let res1 = sim.m(q).unwrap_bool();
        assert!(res1, "Expected True without noise.");
        sim.x(q);
        let res2 = sim.m(q).unwrap_bool();
        assert!(!res2, "Expected False without noise.");
    }
    assert!(
        sim.qubit_release(q),
        "Expected correct qubit state on release."
    );
}

#[test]
fn bitflip_measurement() {
    let noise = PauliNoise::from_probabilities(1.0, 0.0, 0.0)
        .expect("bit flip noise with probability 100% should be constructable.");
    let mut sim = SparseSim::new_with_noise(&noise);
    assert!(!sim.is_noiseless(), "Expected noisy simulator.");
    let q = sim.qubit_allocate(); // Allocation is noiseless even with noise.
    for _ in 0..100 {
        let res1 = sim.m(q).unwrap_bool();
        assert!(res1, "Expected True for 100% bit flip noise.");
        let res2 = sim.m(q).unwrap_bool();
        assert!(!res2, "Expected False for 100% bit flip noise.");
    }
    assert!(
        sim.qubit_release(q),
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
        let q = sim.qubit_allocate(); // Allocation is noiseless even with noise.
        // sim.m sometimes applies X before measuring
        if sim.m(q).unwrap_bool() {
            true_count += 1;
        }
        sim.qubit_release(q);
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
    let state = sim.capture_quantum_state();
    expected.assert_eq(&state_to_string(&state));
}

#[test]
fn noisy_via_x() {
    let noise = PauliNoise::from_probabilities(1.0, 0.0, 0.0)
        .expect("bit flip noise with probability 100% should be constructable.");
    let mut sim = SparseSim::new_with_noise(&noise);
    assert!(!sim.is_noiseless(), "Expected noisy simulator.");
    let q = sim.qubit_allocate(); // Allocation is noiseless even with noise.
    check_state(&mut sim, &expect!["|0⟩: 1.0000+0.0000𝑖 "]);
    sim.x(q); // Followed by X. So, no op.
    check_state(&mut sim, &expect!["|0⟩: 1.0000+0.0000𝑖 "]);
    sim.y(q); // Followed by X.
    check_state(&mut sim, &expect!["|0⟩: 0.0000+1.0000𝑖 "]);
    sim.z(q); // Followed by X.
    check_state(&mut sim, &expect!["|1⟩: 0.0000+1.0000𝑖 "]);
}

#[test]
fn noisy_via_y() {
    let noise = PauliNoise::from_probabilities(0.0, 1.0, 0.0)
        .expect("0.0, 1.0, 0.0 Pauli noise should be constructable.");
    let mut sim = SparseSim::new_with_noise(&noise);
    assert!(!sim.is_noiseless(), "Expected noisy simulator.");
    let q = sim.qubit_allocate(); // Allocation is noiseless even with noise.
    check_state(&mut sim, &expect!["|0⟩: 1.0000+0.0000𝑖 "]);
    sim.x(q); // Followed by Y.
    check_state(&mut sim, &expect!["|0⟩: 0.0000−1.0000𝑖 "]);
    sim.y(q); // Followed by Y. So, no op.
    check_state(&mut sim, &expect!["|0⟩: 0.0000−1.0000𝑖 "]);
    sim.z(q); // Followed by Y.
    check_state(&mut sim, &expect!["|1⟩: 1.0000+0.0000𝑖 "]);
}

#[test]
fn noisy_via_z() {
    let noise = PauliNoise::from_probabilities(0.0, 0.0, 1.0)
        .expect("phase flip noise with probability 100% should be constructable.");
    let mut sim = SparseSim::new_with_noise(&noise);
    assert!(!sim.is_noiseless(), "Expected noisy simulator.");
    let q = sim.qubit_allocate(); // Allocation is noiseless even with noise.
    check_state(&mut sim, &expect!["|0⟩: 1.0000+0.0000𝑖 "]);
    sim.x(q); // Followed by Z.
    check_state(&mut sim, &expect!["|1⟩: −1.0000+0.0000𝑖 "]);
    sim.y(q); // Followed by Z.
    check_state(&mut sim, &expect!["|0⟩: 0.0000+1.0000𝑖 "]);
    sim.z(q); // Followed by Z. So, no op.
    check_state(&mut sim, &expect!["|0⟩: 0.0000+1.0000𝑖 "]);
}

#[test]
fn measure_without_loss_returns_value() {
    let mut sim = SparseSim::new();
    let q = sim.qubit_allocate();
    let res = sim.m(q);
    assert!(
        matches!(res, val::Result::Val(_)),
        "Expected measurement to return a result"
    );
}

#[test]
fn measure_with_loss_returns_loss() {
    let mut sim = SparseSim::new();
    sim.set_loss(1.0); // Set loss probability to 100%
    let q = sim.qubit_allocate();
    let res = sim.m(q);
    assert_eq!(
        res,
        val::Result::Loss,
        "Expected measurement with loss to return None"
    );
}

#[test]
fn new_with_seed_produces_deterministic_measurements() {
    // Two simulators constructed with the same seed should produce identical measurement sequences.
    let mut sim1 = SparseSim::new_with_seed(Some(42));
    let mut sim2 = SparseSim::new_with_seed(Some(42));
    for _ in 0..64 {
        let q1 = sim1.qubit_allocate();
        sim1.h(q1);
        let r1 = sim1.m(q1).unwrap_bool();
        sim1.qubit_release(q1);

        let q2 = sim2.qubit_allocate();
        sim2.h(q2);
        let r2 = sim2.m(q2).unwrap_bool();
        sim2.qubit_release(q2);

        assert_eq!(
            r1, r2,
            "Simulators with same seed should produce identical measurements"
        );
    }
}

#[test]
fn new_with_seed_none_creates_valid_simulator() {
    // Passing None should work the same as SparseSim::new().
    let mut sim = SparseSim::new_with_seed(None);
    let q = sim.qubit_allocate();
    sim.h(q);
    let res = sim.m(q);
    assert!(
        matches!(res, val::Result::Val(_)),
        "Expected measurement to return a result"
    );
    sim.qubit_release(q);
}
