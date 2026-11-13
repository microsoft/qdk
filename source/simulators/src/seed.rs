// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Derives the seeds of independent random number generators from a single seed.
//!
//! A seeded run uses several random number generators: one per shot and, within a shot, one each
//! for measurement outcomes, noise faults, and classical draws such as `DrawRandomInt`. Seeding two
//! of them with the same value makes their streams identical, so draws that should be independent
//! are correlated (for example, a noise fault becomes a function of an earlier measurement
//! outcome). Seeding shot `k` with `seed + k` makes runs with nearby seeds share shots.
//!
//! To avoid both problems, a component that owns a random number generator seeds it with
//! [`derive_seed`] (or [`stream_rng`]), passing the seed it was given together with the
//! [`Stream`] that names the generator's role. Components pass the seed they were given on to
//! their subcomponents unchanged; only the generator's owner derives.

use rand::{Rng as _, SeedableRng as _, rngs::StdRng};

#[cfg(test)]
mod tests;

/// The role of a random number generator seeded from a shared seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    /// The seed of the `n`th of several executions that share a seed, such as the shots of a run.
    Shot(u64),
    /// Sampling of the quantum state, such as measurement outcomes.
    Measurement,
    /// Noise faults, including qubit loss.
    Noise,
    /// Classical random draws made by the program, such as `DrawRandomInt`.
    Classical,
}

impl Stream {
    fn label(self) -> (u64, u64) {
        match self {
            Stream::Shot(n) => (1, n),
            Stream::Measurement => (2, 0),
            Stream::Noise => (3, 0),
            Stream::Classical => (4, 0),
        }
    }
}

/// Derives the seed of the generator for `stream` from `seed`.
///
/// Distinct `(seed, stream)` pairs give unrelated seeds, so no two generators share a stream.
#[must_use]
pub fn derive_seed(seed: u64, stream: Stream) -> u64 {
    let (kind, index) = stream.label();
    let mut key = [0; 32];
    key[..8].copy_from_slice(&seed.to_le_bytes());
    key[8..16].copy_from_slice(&kind.to_le_bytes());
    key[16..24].copy_from_slice(&index.to_le_bytes());
    StdRng::from_seed(key).next_u64()
}

/// Creates the generator for `stream`, seeded from `seed`.
#[must_use]
pub fn stream_rng(seed: u64, stream: Stream) -> StdRng {
    StdRng::seed_from_u64(derive_seed(seed, stream))
}

/// Returns a seed drawn from system entropy, for runs that were not given a seed.
#[must_use]
pub fn entropy_seed() -> u64 {
    rand::rng().next_u64()
}

/// Returns whether any value appears twice across the first draws of `rngs`.
///
/// Generators with identical or overlapping streams share draws whatever their relative offset
/// (up to the window size), while independent generators share one with negligible probability,
/// whatever their seeds. Tests use this to check that generators are independent without
/// depending on how many draws any component makes.
#[doc(hidden)]
#[must_use]
pub fn share_draws<R: rand::Rng>(rngs: impl IntoIterator<Item = R>) -> bool {
    const DRAWS: usize = 64;
    let mut seen = rustc_hash::FxHashSet::default();
    rngs.into_iter()
        .any(|mut rng| (0..DRAWS).any(|_| !seen.insert(rng.next_u64())))
}
