// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;
pub(crate) mod uq1_63;

use num_traits::{ConstZero, Float};
use rustc_hash::FxHashMap;
use std::hash::BuildHasherDefault;

pub(crate) type IntrinsicID = u32;

pub trait Fault {
    fn none() -> Self;
    fn loss() -> Self;
}

/// Noise description for each operation.
///
/// This is the format in which the user config files are
/// written.
#[derive(Clone, Debug)]
pub struct NoiseConfig<T: Float, Q: Float> {
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
    pub mov: NoiseTable<T>,
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

impl<T: Float + ConstZero, Q: Float> NoiseConfig<T, Q> {
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
        mov: NoiseTable::<T>::noiseless(1),
        mresetz: NoiseTable::<T>::noiseless(1),
        idle: IdleNoiseParams::NOISELESS,
        intrinsics: const_empty_hash_map(),
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
pub struct NoiseTable<T: Float> {
    pub qubits: u32,
    pub pauli_strings: Vec<String>,
    pub probabilities: Vec<T>,
    pub loss: T,
}

impl<T: Float + ConstZero> NoiseTable<T> {
    #[must_use]
    pub const fn noiseless(qubits: u32) -> Self {
        Self {
            qubits,
            pauli_strings: Vec::new(),
            probabilities: Vec::new(),
            loss: num_traits::ConstZero::ZERO,
        }
    }
}

impl<T: Float> NoiseTable<T> {
    #[must_use]
    pub fn is_noiseless(&self) -> bool {
        self.probabilities.is_empty() && self.loss == T::zero()
    }

    #[must_use]
    pub fn has_pauli_noise(&self) -> bool {
        self.probabilities.iter().any(|p| *p > T::zero())
    }
}

/// Describes the noise configuration for each operation.
///
/// This is the internal format used by the simulator.
pub struct CumulativeNoiseConfig<T> {
    pub i: CumulativeNoiseTable<T>,
    pub x: CumulativeNoiseTable<T>,
    pub y: CumulativeNoiseTable<T>,
    pub z: CumulativeNoiseTable<T>,
    pub h: CumulativeNoiseTable<T>,
    pub s: CumulativeNoiseTable<T>,
    pub s_adj: CumulativeNoiseTable<T>,
    pub t: CumulativeNoiseTable<T>,
    pub t_adj: CumulativeNoiseTable<T>,
    pub sx: CumulativeNoiseTable<T>,
    pub sx_adj: CumulativeNoiseTable<T>,
    pub rx: CumulativeNoiseTable<T>,
    pub ry: CumulativeNoiseTable<T>,
    pub rz: CumulativeNoiseTable<T>,
    pub cx: CumulativeNoiseTable<T>,
    pub cy: CumulativeNoiseTable<T>,
    pub cz: CumulativeNoiseTable<T>,
    pub rxx: CumulativeNoiseTable<T>,
    pub ryy: CumulativeNoiseTable<T>,
    pub rzz: CumulativeNoiseTable<T>,
    pub swap: CumulativeNoiseTable<T>,
    pub mov: CumulativeNoiseTable<T>,
    pub mresetz: CumulativeNoiseTable<T>,
    pub idle: IdleNoiseParams,
    pub intrinsics: FxHashMap<IntrinsicID, CorrelatedNoiseSampler<T>>,
}

impl<F> From<NoiseConfig<f64, f64>> for CumulativeNoiseConfig<F>
where
    F: Fault + Clone + for<'s> From<&'s str>,
{
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
            mov: value.mov.into(),
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
pub struct CumulativeNoiseTable<T> {
    pub sampler: CorrelatedNoiseSampler<T>,
    pub loss: f64,
}

impl<F> From<NoiseTable<f64>> for CumulativeNoiseTable<F>
where
    F: Fault + Clone + for<'s> From<&'s str>,
{
    fn from(value: NoiseTable<f64>) -> Self {
        let choices = value.pauli_strings.into_iter().map(|p| F::from(&p));
        let probs = value.probabilities.into_iter().map(uq1_63::from_prob);
        Self {
            sampler: CorrelatedNoiseSampler::new(choices, probs),
            loss: value.loss,
        }
    }
}

impl<F> CumulativeNoiseTable<F>
where
    F: Fault + Clone,
{
    /// Samples a float in the range [0, 1] and picks one of the faults
    /// `X`, `Y`, `Z`, `Loss` based on the provided noise table.
    #[must_use]
    pub fn gen_operation_fault(&self, rng: &mut impl rand::Rng) -> F {
        let sample: f64 = rng.gen_range(0.0..1.0);
        if sample < self.loss {
            return F::loss();
        }
        self.sampler.sample(rng)
    }
}

pub struct CorrelatedNoiseSampler<T> {
    /// The total probability of any noise.
    noise_probability: u64,
    /// The errors to choose from.
    choices: Vec<T>,
    /// Cumulative probabilities in the [`uq1_63`] format.
    cumulative_probabilities: Vec<u64>,
}

impl<F> From<NoiseTable<f64>> for CorrelatedNoiseSampler<F>
where
    F: Fault + Clone + for<'a> From<&'a str>,
{
    fn from(value: NoiseTable<f64>) -> Self {
        assert!(
            !value.pauli_strings.is_empty(),
            "there should be at least one pauli_string"
        );
        let choices = value.pauli_strings.iter().map(|p| F::from(p.as_str()));
        let probs = value.probabilities.into_iter().map(uq1_63::from_prob);
        Self::new(choices, probs)
    }
}

impl<F: Fault + Clone> CorrelatedNoiseSampler<F> {
    #[must_use]
    pub fn new<Choices, Probabilities>(choices: Choices, probs: Probabilities) -> Self
    where
        Choices: IntoIterator<Item = F>,
        Probabilities: IntoIterator<Item = u64>,
    {
        let probs = probs.into_iter();
        let mut cumulative_probabilities: Vec<u64> = Vec::with_capacity(probs.size_hint().0);
        let mut noise_probability: u64 = 0;

        for p in probs {
            noise_probability += p;
            assert!(
                noise_probability <= uq1_63::ONE,
                "total probability should not exceed 1.0"
            );
            cumulative_probabilities.push(noise_probability);
        }

        Self {
            noise_probability,
            choices: choices.into_iter().collect(),
            cumulative_probabilities,
        }
    }

    #[must_use]
    pub fn sample(&self, rng: &mut impl rand::Rng) -> F {
        let distr = rand::distributions::Uniform::new(0, uq1_63::ONE);
        let random_sample: u64 = rng.sample(distr);
        self.sample_with_value(random_sample)
    }

    /// Samples a fault given a pre-generated random value in the range `[0, uq1_63::ONE)`.
    /// This is useful for testing purposes.
    #[must_use]
    pub fn sample_with_value(&self, random_sample: u64) -> F {
        // This codepath will be taken > 99.9% of times, since the total error probability
        // is usually very low.
        if random_sample >= self.noise_probability {
            return F::none();
        }
        // Find the index of the first cumulative probability greater than the chosen sample.
        let idx = self
            .cumulative_probabilities
            .partition_point(|p| *p <= random_sample);

        self.choices[idx].clone()
    }
}

/// Checks if a pauli string is the identity.
#[must_use]
pub fn is_pauli_identity(pauli_string: &str) -> bool {
    pauli_string.chars().all(|c| c == 'I')
}
