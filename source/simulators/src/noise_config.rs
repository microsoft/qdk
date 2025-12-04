// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// Noise description for each operation.
///
/// This is the format in which the user config files are
/// written.
#[derive(Clone, Debug)]
pub struct NoiseConfig {
    pub i: NoiseTable,
    pub x: NoiseTable,
    pub y: NoiseTable,
    pub z: NoiseTable,
    pub h: NoiseTable,
    pub s: NoiseTable,
    pub s_adj: NoiseTable,
    pub t: NoiseTable,
    pub t_adj: NoiseTable,
    pub sx: NoiseTable,
    pub sx_adj: NoiseTable,
    pub rx: NoiseTable,
    pub ry: NoiseTable,
    pub rz: NoiseTable,
    pub cx: NoiseTable,
    pub cz: NoiseTable,
    pub rxx: NoiseTable,
    pub ryy: NoiseTable,
    pub rzz: NoiseTable,
    pub swap: NoiseTable,
    pub mov: NoiseTable,
    pub mresetz: NoiseTable,
    pub idle: IdleNoiseParams,
}

impl NoiseConfig {
    pub const NOISELESS: Self = Self {
        i: NoiseTable::noiseless(1),
        x: NoiseTable::noiseless(1),
        y: NoiseTable::noiseless(1),
        z: NoiseTable::noiseless(1),
        h: NoiseTable::noiseless(1),
        s: NoiseTable::noiseless(1),
        s_adj: NoiseTable::noiseless(1),
        t: NoiseTable::noiseless(1),
        t_adj: NoiseTable::noiseless(1),
        sx: NoiseTable::noiseless(1),
        sx_adj: NoiseTable::noiseless(1),
        rx: NoiseTable::noiseless(1),
        ry: NoiseTable::noiseless(1),
        rz: NoiseTable::noiseless(1),
        cx: NoiseTable::noiseless(2),
        cz: NoiseTable::noiseless(2),
        rxx: NoiseTable::noiseless(2),
        ryy: NoiseTable::noiseless(2),
        rzz: NoiseTable::noiseless(2),
        swap: NoiseTable::noiseless(2),
        mov: NoiseTable::noiseless(1),
        mresetz: NoiseTable::noiseless(1),
        idle: IdleNoiseParams::NOISELESS,
    };
}

/// The probability of idle noise is computed using the equation:
///   `idle_noise_prob(steps) = (s_probability + 1.0).pow(step) - 1.0`
///
/// Where:
///  - `s_probability`: is the probability of an `S` happening during
///    an idle a step, and is in the range `[0, 1]`.
///
/// This structure allows the user to paremetrize the equation.
#[derive(Clone, Copy, Debug)]
pub struct IdleNoiseParams {
    pub s_probability: f32,
}

impl IdleNoiseParams {
    pub const NOISELESS: Self = Self { s_probability: 0.0 };

    #[must_use]
    pub fn s_probability(self, steps: u32) -> f32 {
        (self.s_probability + 1.0).powi(i32::try_from(steps).expect("steps should fit in 31 bits"))
            - 1.0
    }
}

/// Noise description for an operation.
///
/// `pauli_strings[i]` contains the ith Pauli string
/// specified by the user, which we need to apply
/// with the probability `probabilities[i]`. All pauli
/// strings are mutually exclusive. Therefore, their probabilities
/// must add up to a number less or equal than `1.0`.
#[derive(Clone, Debug)]
pub struct NoiseTable {
    pub qubits: u32,
    pub pauli_strings: Vec<String>,
    pub probabilities: Vec<f32>,
    pub loss: f32,
}

impl NoiseTable {
    #[must_use]
    pub const fn noiseless(qubits: u32) -> Self {
        Self {
            qubits,
            pauli_strings: Vec::new(),
            probabilities: Vec::new(),
            loss: 0.0,
        }
    }

    #[must_use]
    pub fn is_noiseless(&self) -> bool {
        self.probabilities.iter().all(|p| *p == 0.0) && self.loss == 0.0
    }

    #[must_use]
    pub fn has_pauli_noise(&self) -> bool {
        self.probabilities.iter().any(|p| *p > 0.0)
    }
}
