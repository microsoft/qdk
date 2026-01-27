// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::noise_config::{self, CumulativeNoiseConfig, is_pauli_identity};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PauliFault {
    I,
    X,
    Y,
    Z,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Fault {
    /// No fault occurred.
    #[default]
    None,
    /// A Pauli fault.
    Pauli(Vec<PauliFault>),
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

impl<S: AsRef<str>> From<S> for Fault {
    fn from(pauli_string: S) -> Self {
        let pauli_string: &str = pauli_string.as_ref();
        assert!(
            !is_pauli_identity(pauli_string),
            "the NoiseTable input validation should ensure we don't insert the identity string"
        );

        let pauli_product = pauli_string
            .chars()
            .map(|c| match c {
                'I' => PauliFault::I,
                'X' => PauliFault::X,
                'Y' => PauliFault::Y,
                'Z' => PauliFault::Z,
                _ => panic!("invalid pauli string character: {c}"),
            })
            .collect();

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
