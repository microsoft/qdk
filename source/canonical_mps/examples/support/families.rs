// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The circuit families the examples measure the engine on, each built
//! from its size and, as it needs them, a depth and a seed.

use super::{
    circuit::Circuit,
    gates::{H, controlled, pauli_x, pauli_y, phase, random_unitary, rx, rzz, u3},
    rng::Rng,
};
use std::f64::consts::PI;

/// Random `u3` on every qubit, then CNOT or CY on alternating neighbours:
/// a brickwork of random single-qubit rotations and entangling gates.
pub fn brickwork(n: usize, depth: usize, seed: u64) -> Circuit {
    let mut rng = Rng::new(seed);
    let mut circuit = Circuit::new(n);
    let tau = std::f64::consts::TAU;
    for layer in 0..depth {
        for q in 0..n {
            let (phi, theta, lambda) = (
                tau * rng.uniform(),
                tau * rng.uniform(),
                tau * rng.uniform(),
            );
            circuit.one(q, u3(phi, theta, lambda));
        }
        for a in (layer % 2..n.saturating_sub(1)).step_by(2) {
            if (a / 2 + layer) % 2 == 0 {
                circuit.two(a, a + 1, controlled(pauli_x()));
            } else {
                circuit.two(a + 1, a, controlled(pauli_y()));
            }
        }
    }
    circuit
}

/// A quench of the transverse-field Ising chain at its critical point from
/// `|0…0⟩`: per layer, one Trotter step of 0.2, `exp(i·0.2·ZZ)` on even
/// then odd neighbours, then `exp(i·0.2·X)` on every qubit.
pub fn ising(n: usize, depth: usize) -> Circuit {
    let dt = 0.2;
    let mut circuit = Circuit::new(n);
    for _ in 0..depth {
        for parity in 0..2 {
            for a in (parity..n.saturating_sub(1)).step_by(2) {
                circuit.two(a, a + 1, rzz(-2.0 * dt));
            }
        }
        for q in 0..n {
            circuit.one(q, rx(-2.0 * dt));
        }
    }
    circuit
}

/// A random product state (a random unitary on every qubit), then the
/// quantum Fourier transform without its final SWAPs: controlled phases on
/// every pair.
pub fn qft(n: usize, seed: u64) -> Circuit {
    let mut rng = Rng::new(seed);
    let mut circuit = Circuit::new(n);
    for q in 0..n {
        circuit.one(q, random_unitary::<4>(&mut rng, 2));
    }
    for j in 0..n {
        circuit.one(j, H);
        for k in j + 1..n {
            let theta = PI / f64::powi(2.0, i32::try_from(k - j).expect("a small distance"));
            circuit.two(k, j, controlled(phase(theta)));
        }
    }
    circuit
}
