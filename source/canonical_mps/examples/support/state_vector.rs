// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The exact state vector the examples compare the engine with.

use super::{
    C, c,
    circuit::{Circuit, Op},
    gates::{pauli_x, pauli_y, pauli_z},
};
use qdk_simulators::SparseStateSim;
use tensornet::Pauli;

/// A state vector, qubit `q` as bit `q` of the index.
pub struct StateVector {
    pub psi: Vec<C>,
}

impl StateVector {
    /// `circuit`'s exact state, from `|0…0⟩`.
    pub fn run(circuit: &Circuit) -> Self {
        let mut psi = vec![c(0.0, 0.0); 1 << circuit.qubits];
        psi[0] = c(1.0, 0.0);
        let mut state = Self { psi };
        for op in &circuit.ops {
            match op {
                Op::Gate { qubits, matrix, .. } => state.apply(qubits, matrix),
                Op::Project { .. } => unimplemented!("a unitary circuit"),
            }
        }
        state
    }

    /// The sparse simulator's state, normalized.
    pub fn of_sparse(sim: &mut SparseStateSim) -> Self {
        let (entries, qubits) = sim.get_state();
        let mut psi = vec![c(0.0, 0.0); 1 << qubits];
        for (index, amplitude) in entries {
            let index = index.iter_u64_digits().next().unwrap_or(0);
            psi[usize::try_from(index).expect("a small index")] = amplitude;
        }
        let norm = psi.iter().map(C::norm_sqr).sum::<f64>().sqrt();
        for x in &mut psi {
            *x /= norm;
        }
        Self { psi }
    }

    fn apply(&mut self, qubits: &[usize], m: &[C]) {
        match *qubits {
            [q] => {
                let s = 1 << q;
                for i in (0..self.psi.len()).filter(|i| i & s == 0) {
                    let (v0, v1) = (self.psi[i], self.psi[i | s]);
                    self.psi[i] = m[0] * v0 + m[1] * v1;
                    self.psi[i | s] = m[2] * v0 + m[3] * v1;
                }
            }
            [a, b] => {
                let (sa, sb) = (1 << a, 1 << b);
                for i in (0..self.psi.len()).filter(|i| i & (sa | sb) == 0) {
                    let index = [i, i | sb, i | sa, i | sa | sb];
                    let v = index.map(|k| self.psi[k]);
                    for (row, &k) in index.iter().enumerate() {
                        self.psi[k] = (0..4).map(|col| m[4 * row + col] * v[col]).sum();
                    }
                }
            }
            _ => unimplemented!("gates on one or two qubits"),
        }
    }

    /// `⟨ψ|P|ψ⟩` for the Pauli string `factors` on qubits.
    pub fn pauli_expectation(&self, factors: &[(usize, Pauli)]) -> f64 {
        let mut copy = Self {
            psi: self.psi.clone(),
        };
        for &(q, p) in factors {
            let m = match p {
                Pauli::X => pauli_x(),
                Pauli::Y => pauli_y(),
                Pauli::Z => pauli_z(),
            };
            copy.apply(&[q], &m);
        }
        self.psi
            .iter()
            .zip(&copy.psi)
            .map(|(a, b)| (a.conj() * b).re)
            .sum()
    }

    pub fn probability_of_one(&self, q: usize) -> f64 {
        self.psi
            .iter()
            .enumerate()
            .filter(|(i, _)| i >> q & 1 == 1)
            .map(|(_, v)| v.norm_sqr())
            .sum()
    }
}
