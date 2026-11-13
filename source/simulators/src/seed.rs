// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Derives the seeds of independent random number generators from a single seed.
//!
//! A seeded run gives each shot its own seed. Within a shot, random number generators have distinct
//! roles: measurement outcomes, noise faults, and classical draws such as `DrawRandomInt`. Seeding
//! generators for different roles with the same value makes their draws identical, correlating
//! outcomes that should be independent (for example, a noise fault becomes a function of an earlier
//! measurement outcome). Seeding shot `k` with `seed + k` makes runs with nearby seeds share shots.
//!
//! To avoid both problems, a component that owns a random number generator creates it with
//! [`make_rng`], passing the seed it was given together with the [`RngRole`] that names the
//! generator's purpose. Components pass the seed they were given on to their subcomponents unchanged;
//! only the generator's owner derives. A component that runs several shots or evaluations gives
//! each its own seed with [`make_seed`], using [`SeedRole::Shot`] or [`SeedRole::Evaluation`].

use rand::{Rng as _, SeedableRng as _, rngs::StdRng};

#[cfg(test)]
mod tests;

/// The role of a seed derived for an execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedRole {
    /// The seed of the `n`th shot of a run.
    Shot(u64),
    /// The classical seed of the `n`th evaluation in a long-lived session, such as successive `eval`
    /// calls on one interpreter. Distinct from [`SeedRole::Shot`], so evaluations and the shots
    /// of runs don't repeat each other's draws.
    Evaluation(u64),
}

impl SeedRole {
    fn extract(self) -> (u64, u64) {
        match self {
            SeedRole::Shot(index) => (0, index),
            SeedRole::Evaluation(index) => (1, index),
        }
    }
}

/// The role of a random number generator within a seeded execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RngRole {
    /// Sampling of the quantum state, such as measurement outcomes.
    Measurement = 0,
    /// Noise faults, including qubit loss.
    Noise = 1,
    /// Classical random draws made by the program, such as `DrawRandomInt`.
    Classical = 2,
}

/// Separates generator and derived-seed keys, allowing their role labels to overlap.
#[derive(Clone, Copy)]
enum KeyKind {
    Generator = 0,
    Seed = 1,
}

/// Encodes the seed, role label, execution index, and key kind as a generator key.
///
/// The kind distinguishes the seed and generator namespaces, even when their role labels match.
///
/// Note: a key is really a kind of seed, but it helps to use a different word.
fn make_key(seed: u64, role: u64, index: u64, kind: KeyKind) -> [u8; 32] {
    const DOMAIN: u64 = u64::from_le_bytes(*b"qdk-seed");
    let mut key = [0; 32];
    for (chunk, word) in
        key.chunks_exact_mut(8)
            .zip([DOMAIN, seed, index, (role << 8) | kind as u64])
    {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    key
}

/// Derives a seed for `role`, such as one shot of a run, from `seed`.
///
/// Distinct `(seed, role)` pairs derive independently, though the resulting 64-bit seeds can collide.
#[must_use]
pub fn make_seed(seed: u64, role: SeedRole) -> u64 {
    let (role, index) = role.extract();
    StdRng::from_seed(make_key(seed, role, index, KeyKind::Seed)).next_u64()
}

/// Creates the generator for `role`, keyed by `seed`.
///
/// Distinct `(seed, role)` pairs give independently keyed generators.
#[must_use]
pub fn make_rng(seed: u64, role: RngRole) -> StdRng {
    StdRng::from_seed(make_key(seed, role as u64, 0, KeyKind::Generator))
}

/// Returns a seed drawn from system entropy, for runs that were not given a seed.
#[must_use]
pub fn make_system_seed() -> u64 {
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
