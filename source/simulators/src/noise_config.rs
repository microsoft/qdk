// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;
pub(crate) mod uq1_63;

use num_traits::ConstZero;
use qsc_data_structures::display::{write_field, writeln_field, writeln_header};
use rand::RngExt;
use rustc_hash::FxHashMap;
use std::{fmt::Display, hash::BuildHasherDefault};

use crate::noise_config::uq1_63::UQ1_63;

pub(crate) type IntrinsicID = u32;

/// A [`u64`] where every 3-bits encodes one of I, X, Y, Z, and L
/// where IXYZ represent Pauli terms, and L represents loss.
pub type PauliAndLossString = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FaultTerm {
    /// An `I` Pauli.
    I,
    /// An `X` Pauli.
    X,
    /// A `Y` Pauli.
    Y,
    /// A `Z` Pauli.
    Z,
    /// The qubit was lost.
    Loss,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// A string of [`FaultTerm`].
pub struct Fault(pub Vec<FaultTerm>);

impl From<(PauliAndLossString, u32)> for Fault {
    fn from((pauli, qubits): (PauliAndLossString, u32)) -> Self {
        const MAP: [FaultTerm; 5] = [
            FaultTerm::I,
            FaultTerm::X,
            FaultTerm::Z,
            FaultTerm::Y,
            FaultTerm::Loss,
        ];
        assert!(
            !is_pauli_identity(pauli),
            "the NoiseTable input validation should ensure we don't insert the identity string"
        );
        let fault_string = decode_pauli(pauli, qubits, &MAP);
        Self(fault_string)
    }
}

impl CumulativeNoiseConfig {
    /// Returns true if an idle fault has triggered.
    #[must_use]
    pub fn gen_idle_fault(&self, rng: &mut impl rand::Rng, idle_steps: u32) -> bool {
        let sample: f32 = rng.random_range(0.0..1.0);
        sample < self.idle.s_probability(idle_steps)
    }
}

/// Noise description for each operation.
///
/// This is the format in which the user config files are
/// written.
#[derive(Clone, Debug)]
pub struct NoiseConfig<T, Q> {
    pub i: NoiseTable<T>,
    pub x: NoiseTable<T>,
    pub y: NoiseTable<T>,
    pub z: NoiseTable<T>,
    pub h: NoiseTable<T>,
    pub s: NoiseTable<T>,
    pub s_adj: NoiseTable<T>,
    pub t: NoiseTable<T>,
    pub t_adj: NoiseTable<T>,
    pub sx: NoiseTable<T>,
    pub sx_adj: NoiseTable<T>,
    pub rx: NoiseTable<T>,
    pub ry: NoiseTable<T>,
    pub rz: NoiseTable<T>,
    pub cx: NoiseTable<T>,
    pub cy: NoiseTable<T>,
    pub cz: NoiseTable<T>,
    pub rxx: NoiseTable<T>,
    pub ryy: NoiseTable<T>,
    pub rzz: NoiseTable<T>,
    pub swap: NoiseTable<T>,
    pub ccx: NoiseTable<T>,
    pub mov: NoiseTable<T>,
    pub mz: NoiseTable<T>,
    pub mresetz: NoiseTable<T>,
    pub idle: IdleNoiseParams,
    pub intrinsics: FxHashMap<IntrinsicID, NoiseTable<Q>>,
}

#[must_use]
pub const fn const_empty_hash_map<K, V>() -> FxHashMap<K, V> {
    const HASH_BUILDER: BuildHasherDefault<rustc_hash::FxHasher> = BuildHasherDefault::new();
    #[allow(clippy::disallowed_types, reason = "we are using FxHasher here")]
    const {
        std::collections::HashMap::with_hasher(HASH_BUILDER)
    }
}

impl<T, Q> NoiseConfig<T, Q> {
    pub const NOISELESS: Self = Self {
        i: NoiseTable::<T>::noiseless(1),
        x: NoiseTable::<T>::noiseless(1),
        y: NoiseTable::<T>::noiseless(1),
        z: NoiseTable::<T>::noiseless(1),
        h: NoiseTable::<T>::noiseless(1),
        s: NoiseTable::<T>::noiseless(1),
        s_adj: NoiseTable::<T>::noiseless(1),
        t: NoiseTable::<T>::noiseless(1),
        t_adj: NoiseTable::<T>::noiseless(1),
        sx: NoiseTable::<T>::noiseless(1),
        sx_adj: NoiseTable::<T>::noiseless(1),
        rx: NoiseTable::<T>::noiseless(1),
        ry: NoiseTable::<T>::noiseless(1),
        rz: NoiseTable::<T>::noiseless(1),
        cx: NoiseTable::<T>::noiseless(2),
        cy: NoiseTable::<T>::noiseless(2),
        cz: NoiseTable::<T>::noiseless(2),
        rxx: NoiseTable::<T>::noiseless(2),
        ryy: NoiseTable::<T>::noiseless(2),
        rzz: NoiseTable::<T>::noiseless(2),
        swap: NoiseTable::<T>::noiseless(2),
        ccx: NoiseTable::<T>::noiseless(3),
        mov: NoiseTable::<T>::noiseless(1),
        mz: NoiseTable::<T>::noiseless(1),
        mresetz: NoiseTable::<T>::noiseless(1),
        idle: IdleNoiseParams::NOISELESS,
        intrinsics: const_empty_hash_map(),
    };
}

impl<T: Display, Q: Display> Display for NoiseConfig<T, Q> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        /// Macro to avoid repeating the field names twice as in:
        /// ```ignore
        ///     writeln_field(f, "mresetz", &self.mresetz)?;
        /// ```
        macro_rules! write_if_noisy {
            ($field:ident) => {
                if !self.$field.is_noiseless() {
                    writeln_field(f, stringify!($field), &self.$field)?;
                }
            };
        }

        // Write stdgates with noise.
        write_if_noisy!(i);
        write_if_noisy!(x);
        write_if_noisy!(y);
        write_if_noisy!(z);
        write_if_noisy!(h);
        write_if_noisy!(s);
        write_if_noisy!(s_adj);
        write_if_noisy!(t);
        write_if_noisy!(t_adj);
        write_if_noisy!(sx);
        write_if_noisy!(sx_adj);
        write_if_noisy!(rx);
        write_if_noisy!(ry);
        write_if_noisy!(rz);
        write_if_noisy!(cx);
        write_if_noisy!(cy);
        write_if_noisy!(cz);
        write_if_noisy!(rxx);
        write_if_noisy!(ryy);
        write_if_noisy!(rzz);
        write_if_noisy!(swap);
        write_if_noisy!(ccx);
        write_if_noisy!(mov);
        write_if_noisy!(mz);
        write_if_noisy!(mresetz);

        // Write IdleNoiseParams.
        writeln_field(f, "idle", &self.idle)?;

        // Write intrinsics.
        for (id, table) in &self.intrinsics {
            writeln_field(f, &id.to_string(), table)?;
        }

        Ok(())
    }
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

impl Display for IdleNoiseParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln_header(f, "IdleNoiseParams")?;
        write_field(f, "s_probability", &self.s_probability)
    }
}

impl Default for IdleNoiseParams {
    fn default() -> Self {
        Self::NOISELESS
    }
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
pub struct NoiseTable<T> {
    pub qubits: u32,
    pub pauli_strings: Vec<PauliAndLossString>,
    pub probabilities: Vec<T>,
}

impl<T> NoiseTable<T> {
    #[must_use]
    pub const fn noiseless(qubits: u32) -> Self {
        Self {
            qubits,
            pauli_strings: Vec::new(),
            probabilities: Vec::new(),
        }
    }

    #[must_use]
    pub const fn is_noiseless(&self) -> bool {
        self.probabilities.is_empty()
    }
}

impl<T: ConstZero + PartialOrd> NoiseTable<T> {
    #[must_use]
    pub fn has_pauli_noise(&self) -> bool {
        self.probabilities.iter().any(|p| *p > T::ZERO)
    }
}

impl<T: Display> Display for NoiseTable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const MAP: [char; 5] = ['I', 'X', 'Z', 'Y', 'L'];
        writeln_header(f, "NoiseTable")?;
        writeln_field(f, "qubit", &self.qubits)?;
        for (encoded_pauli, probability) in self.pauli_strings.iter().zip(&self.probabilities) {
            let fault_string: String = decode_pauli(*encoded_pauli, self.qubits, &MAP)
                .into_iter()
                .collect();
            writeln_field(f, &fault_string, &probability)?;
        }
        Ok(())
    }
}

/// Describes the noise configuration for each operation.
///
/// This is the internal format used by the simulator.
#[derive(Default)]
pub struct CumulativeNoiseConfig {
    pub i: CumulativeNoiseTable,
    pub x: CumulativeNoiseTable,
    pub y: CumulativeNoiseTable,
    pub z: CumulativeNoiseTable,
    pub h: CumulativeNoiseTable,
    pub s: CumulativeNoiseTable,
    pub s_adj: CumulativeNoiseTable,
    pub t: CumulativeNoiseTable,
    pub t_adj: CumulativeNoiseTable,
    pub sx: CumulativeNoiseTable,
    pub sx_adj: CumulativeNoiseTable,
    pub rx: CumulativeNoiseTable,
    pub ry: CumulativeNoiseTable,
    pub rz: CumulativeNoiseTable,
    pub cx: CumulativeNoiseTable,
    pub cy: CumulativeNoiseTable,
    pub cz: CumulativeNoiseTable,
    pub rxx: CumulativeNoiseTable,
    pub ryy: CumulativeNoiseTable,
    pub rzz: CumulativeNoiseTable,
    pub swap: CumulativeNoiseTable,
    pub ccx: CumulativeNoiseTable,
    pub mov: CumulativeNoiseTable,
    pub mz: CumulativeNoiseTable,
    pub mresetz: CumulativeNoiseTable,
    pub idle: IdleNoiseParams,
    pub intrinsics: FxHashMap<IntrinsicID, Sampler>,
}

impl From<NoiseConfig<f64, f64>> for CumulativeNoiseConfig {
    fn from(value: NoiseConfig<f64, f64>) -> Self {
        let intrinsics = value
            .intrinsics
            .into_iter()
            .map(|(k, v)| (k, v.into()))
            .collect::<FxHashMap<_, _>>();

        Self {
            i: value.i.into(),
            x: value.x.into(),
            y: value.y.into(),
            z: value.z.into(),
            h: value.h.into(),
            s: value.s.into(),
            s_adj: value.s_adj.into(),
            t: value.t.into(),
            t_adj: value.t_adj.into(),
            sx: value.sx.into(),
            sx_adj: value.sx_adj.into(),
            rx: value.rx.into(),
            ry: value.ry.into(),
            rz: value.rz.into(),
            cx: value.cx.into(),
            cy: value.cy.into(),
            cz: value.cz.into(),
            rxx: value.rxx.into(),
            ryy: value.ryy.into(),
            rzz: value.rzz.into(),
            swap: value.swap.into(),
            ccx: value.ccx.into(),
            mov: value.mov.into(),
            mz: value.mz.into(),
            mresetz: value.mresetz.into(),
            idle: value.idle,
            intrinsics,
        }
    }
}

/// A cumulative representation of the `NoiseTable` to make
/// computation more efficient.
///
/// This is the internal format used by the simulator.
#[derive(Default)]
pub struct CumulativeNoiseTable {
    pub sampler: Sampler,
}

impl From<NoiseTable<f64>> for CumulativeNoiseTable {
    fn from(value: NoiseTable<f64>) -> Self {
        let qubits = value.qubits;
        let choices = value
            .pauli_strings
            .into_iter()
            .map(|p| Fault::from((p, qubits)));
        let probs = value.probabilities.into_iter().map(uq1_63::from_prob);
        Self {
            sampler: Sampler::new(choices, probs),
        }
    }
}

impl CumulativeNoiseTable {
    /// Samples loss using the noise probabilities in the noise table.
    #[must_use]
    pub fn sample_noise(&self, rng: &mut impl rand::Rng) -> Option<Fault> {
        self.sampler.sample(rng).cloned()
    }
}

pub struct Sampler<T = Fault> {
    /// The total probability of any choice.
    total_probability: UQ1_63,
    /// The values to choose from.
    choices: Vec<T>,
    /// Cumulative probabilities in the [`UQ1_63`] format.
    cumulative_probabilities: Vec<UQ1_63>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            total_probability: Default::default(),
            choices: Default::default(),
            cumulative_probabilities: Default::default(),
        }
    }
}

impl From<NoiseTable<f64>> for Sampler<Fault> {
    fn from(value: NoiseTable<f64>) -> Self {
        assert!(
            !value.pauli_strings.is_empty(),
            "there should be at least one pauli_string"
        );
        let qubits = value.qubits;
        let choices = value
            .pauli_strings
            .into_iter()
            .map(|p| Fault::from((p, qubits)));
        let probs = value.probabilities.into_iter().map(uq1_63::from_prob);
        Self::new(choices, probs)
    }
}

impl<T> Sampler<T> {
    #[must_use]
    pub fn new<Choices, Probabilities>(choices: Choices, probs: Probabilities) -> Self
    where
        Choices: IntoIterator<Item = T>,
        Probabilities: IntoIterator<Item = u64>,
    {
        let probs = probs.into_iter();
        let mut cumulative_probabilities: Vec<u64> = Vec::with_capacity(probs.size_hint().0);
        let mut total_probability: u64 = 0;

        for p in probs {
            total_probability += p;
            assert!(
                total_probability <= uq1_63::ONE,
                "total probability should not exceed 1.0"
            );
            cumulative_probabilities.push(total_probability);
        }

        Self {
            total_probability,
            choices: choices.into_iter().collect(),
            cumulative_probabilities,
        }
    }

    #[must_use]
    pub fn sample(&self, rng: &mut impl rand::Rng) -> Option<&T> {
        let distr = rand::distr::Uniform::new(0, uq1_63::ONE).expect("valid range");
        let random_sample: u64 = rng.sample(distr);
        self.sample_with_value(random_sample)
    }

    /// Samples a fault given a pre-generated random value in the range `[0, uq1_63::ONE)`.
    /// This is useful for testing purposes.
    #[must_use]
    pub fn sample_with_value(&self, random_sample: u64) -> Option<&T> {
        // This codepath will be taken > 99.9% of times, since the total error probability
        // is usually very low.
        if random_sample >= self.total_probability {
            return None;
        }
        // Find the index of the first cumulative probability greater than the chosen sample.
        let idx = self
            .cumulative_probabilities
            .partition_point(|p| *p <= random_sample);

        Some(&self.choices[idx])
    }
}

/// Checks if a pauli string is the identity.
#[must_use]
pub fn is_pauli_identity(pauli_string: PauliAndLossString) -> bool {
    pauli_string == 0
}

/// Encode a validated Pauli string as a `u128` using 3 bits per character
/// (I=0, X=1, Z=2, Y=3, L=4). Supports up to 42 qubits.
#[must_use]
pub fn encode_pauli(pauli: &str) -> PauliAndLossString {
    let pauli = pauli.as_bytes();
    debug_assert!(pauli.len() <= 42);
    let mut result: PauliAndLossString = 0;
    for &b in pauli {
        let bits = match b {
            b'I' => 0,
            b'X' => 1,
            b'Z' => 2,
            b'Y' => 3,
            b'L' => 4,
            _ => unreachable!("pauli bytes must be validated before encoding"),
        };
        result = (result << 3) | bits;
    }
    result
}

/// Decode a `u64`-encoded Pauli string back into a list of T,
/// using the given `map` for decoding.
/// (I=0, X=1, Z=2, Y=3, L=4). Supports up to 21 qubits.
#[must_use]
pub fn decode_pauli<T: Clone>(mut pauli: PauliAndLossString, qubits: u32, map: &[T; 5]) -> Vec<T> {
    let n = qubits as usize;
    let mut buf = vec![map[0].clone(); n];
    for i in (0..n).rev() {
        buf[i] = map[(pauli & 0b111) as usize].clone();
        pauli >>= 3;
    }
    buf
}
