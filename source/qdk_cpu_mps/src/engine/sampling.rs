// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Samples: the values of some qubits over many shots, drawn from the state
//! without projecting it.
//!
//! A program whose measurements all come at its end needs only its final
//! state's distribution: a simulator can run it once and draw every shot from
//! that state. [`MpsEngine`]'s `sample` serves it. The draw keeps these rules,
//! stated for callers on [`CpuMpsEngine`]'s `sample`:
//!
//! - **Reproducible.** Each shot's values are drawn in order, One when a
//!   uniform from the `SplitMix64` stream seeded with `seed` is below P(One |
//!   the shot's earlier values), as `tensornet`'s shared sampler draws them,
//!   so the same state and seed give the same values.
//! - **Never impossible.** A value of probability exactly zero is never drawn:
//!   the other value's probability is then exactly one
//!   ([`reads`](super::reads)).
//! - **One read per prefix.** Shots that drew the same values so far share that
//!   probability, computed once per distinct prefix; each new nonempty prefix
//!   projects its values on a fork, a copy of the chain, so the state is not
//!   projected.
//! - **Memory.** It fails with [`CpuMpsError::TooManyValues`] when the values
//!   cannot be held, and returns at once when there are none.
//!
//! [`Engine::draw_shots`] keeps a map from each prefix drawn to its P(One) and
//! reads the conditional probability of each new one.
//!
//! [`MpsEngine`]: tensornet::MpsEngine
//! [`CpuMpsEngine`]: crate::CpuMpsEngine

use super::Engine;
use crate::{CanonicalMps, CpuMpsError, linear_algebra::LinearAlgebra};
use std::collections::BTreeMap;

impl<A: LinearAlgebra> Engine<A> {
    /// Draws `shots` shots of the values of `sites`, already checked, as
    /// [`CpuMpsEngine`]'s [`MpsEngine::sample`] documents.
    ///
    /// [`MpsEngine::sample`]: tensornet::MpsEngine::sample
    /// [`CpuMpsEngine`]: crate::CpuMpsEngine
    pub(super) fn draw_shots(
        &self,
        state: &CanonicalMps,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, CpuMpsError> {
        let mut rng = SplitMix64::new(seed);
        let mut p1_given_prefix: BTreeMap<Vec<(usize, bool)>, f64> = BTreeMap::new();
        let too_many = CpuMpsError::TooManyValues {
            shots,
            sites: sites.len(),
        };
        let Some(count) = shots.checked_mul(sites.len()) else {
            return Err(too_many);
        };
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut values = Vec::new();
        values.try_reserve_exact(count).map_err(|_| too_many)?;
        for _ in 0..shots {
            let mut prefix = Vec::with_capacity(sites.len());
            for &site in sites {
                let p1 = if let Some(&p1) = p1_given_prefix.get(&prefix) {
                    p1
                } else {
                    let p1 = self.conditional_probabilities(state, site, &prefix)?[1];
                    p1_given_prefix.insert(prefix.clone(), p1);
                    p1
                };
                let one = rng.next_uniform() < p1;
                values.push(one);
                prefix.push((site, one));
            }
        }
        Ok(values)
    }
}

/// The `SplitMix64` generator (Steele, Lea and Flood, 2014), whose uniforms
/// `tensornet`'s shared sampler draws from: `tensornet` keeps its own
/// internal.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next value's top 53 bits as a multiple of 2⁻⁵³, in [0, 1).
    fn next_uniform(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        #[expect(clippy::cast_precision_loss, reason = "53 bits fit an f64")]
        let uniform = (z >> 11) as f64 / (1u64 << 53) as f64;
        uniform
    }
}
