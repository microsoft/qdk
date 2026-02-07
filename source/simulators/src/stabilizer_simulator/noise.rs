// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::noise_config::{self, CumulativeNoiseConfig, decode_pauli, is_pauli_identity};
use paulimer::quantum_core::{self, PauliObservable};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Fault {
    /// No fault occurred.
    #[default]
    None,
    /// A Pauli fault.
    Pauli(Vec<quantum_core::PauliObservable>),
    /// A gradual dephasing fault. Qubits are always slowly
    /// rotating along the Z-axis with an unknown rate,
    /// eventually resulting in an `S` gate.
    S,
    /// The qubit was lost.
    Loss,
}

impl noise_config::Fault for Fault {
    fn none() -> Self {
        Self::None
    }

    fn loss() -> Self {
        Self::Loss
    }
}

impl From<(u64, u32)> for Fault {
    fn from((pauli, qubits): (u64, u32)) -> Self {
        const MAP: [PauliObservable; 4] = [
            PauliObservable::PlusI,
            PauliObservable::PlusX,
            PauliObservable::PlusY,
            PauliObservable::PlusZ,
        ];
        assert!(
            !is_pauli_identity(pauli),
            "the NoiseTable input validation should ensure we don't insert the identity string"
        );
        let pauli_product = decode_pauli(pauli, qubits, &MAP);
        Self::Pauli(pauli_product)
    }
}

impl CumulativeNoiseConfig<Fault> {
    /// Samples a float in the range [0, 1] and picks one of the faults
    /// `X`, `Y`, `Z`, `S` based on the provided noise table.
    #[must_use]
    pub fn gen_idle_fault(&self, rng: &mut impl rand::Rng, idle_steps: u32) -> Fault {
        let sample: f32 = rng.gen_range(0.0..1.0);
        if sample < self.idle.s_probability(idle_steps) {
            Fault::S
        } else {
            Fault::None
        }
    }
}
