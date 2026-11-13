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
//! To avoid both problems, a component that owns a random number generator creates it with
//! [`stream_rng`], passing the seed it was given together with the [`Stream`] that names the
//! generator's role. Components pass the seed they were given on to their subcomponents unchanged;
//! only the generator's owner derives. A component that runs several shots or evaluations gives
//! each its own seed with [`derive_seed`].

use rand::{Rng as _, SeedableRng as _, rngs::StdRng};

#[cfg(test)]
mod tests;

/// The role of a random number generator seeded from a shared seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    /// The seed of the `n`th of several executions that share a seed, such as the shots of a run.
    Shot(u64),
    /// The seed of the `n`th evaluation in a long-lived session, such as successive `eval`
    /// calls on one interpreter. Distinct from [`Stream::Shot`], so evaluations and the shots
    /// of runs don't repeat each other's draws.
    Evaluation(u64),
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
            Stream::Evaluation(n) => (5, n),
        }
    }
}

/// What a key is used for: a generator, or a seed to pass to a subcomponent.
#[derive(Clone, Copy)]
enum Output {
    Generator = 0,
    Seed = 1,
}

/// An injective encoding of `(seed, stream, output)` as a generator key, so that every
/// generator and derived seed comes from a distinct key.
fn key(seed: u64, stream: Stream, output: Output) -> [u8; 32] {
    const DOMAIN: u64 = u64::from_le_bytes(*b"qdk-seed");
    let (kind, index) = stream.label();
    let mut key = [0; 32];
    for (chunk, word) in
        key.chunks_exact_mut(8)
            .zip([DOMAIN, seed, index, (kind << 8) | output as u64])
    {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    key
}

/// Derives a seed for a subcomponent, such as one shot of a run, from `seed`.
///
/// Distinct `(seed, stream)` pairs give unrelated seeds.
#[must_use]
pub fn derive_seed(seed: u64, stream: Stream) -> u64 {
    StdRng::from_seed(key(seed, stream, Output::Seed)).next_u64()
}

/// Creates the generator for `stream`, keyed by `seed`.
///
/// Distinct `(seed, stream)` pairs give generators with distinct keys, so no two generators
/// share a stream.
#[must_use]
pub fn stream_rng(seed: u64, stream: Stream) -> StdRng {
    StdRng::from_seed(key(seed, stream, Output::Generator))
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
