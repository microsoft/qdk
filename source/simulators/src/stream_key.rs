// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Derivation of the random number streams used during simulation.
//!
//! A simulation makes several kinds of random choices (measurement outcomes, noise faults,
//! and classical draws such as `DrawRandomInt`) that must be statistically independent.
//! Each kind of choice gets its own generator, keyed by the seed, the shot (or session),
//! and the kind of choice. Because the generators are keyed independently, rather than
//! seeded from the same value or drawn in sequence from a shared generator, they never
//! share values, and the number of values drawn from one never shifts another.

use rand::{Rng as _, SeedableRng as _, rngs::StdRng};

#[cfg(test)]
mod tests;

/// The kind of random choice a stream is used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    /// Measurement outcomes and any other sampling from the quantum state.
    Measurement,
    /// Noise faults, including qubit loss and readout errors.
    Noise,
    /// Classical random values requested by the program, such as `DrawRandomInt`.
    Classical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    /// A long-lived simulation, such as an interactive interpreter session.
    Session,
    /// One shot of a multi-shot run.
    Shot(u64),
}

/// Identifies a family of independent random streams, one per [`Stream`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamKey {
    seed: u64,
    scope: Scope,
}

impl StreamKey {
    /// The streams for shot `shot` of a run seeded with `seed`.
    #[must_use]
    pub fn shot(seed: u64, shot: u64) -> Self {
        Self {
            seed,
            scope: Scope::Shot(shot),
        }
    }

    /// The streams for a long-lived simulation seeded with `seed`.
    #[must_use]
    pub fn session(seed: u64) -> Self {
        Self {
            seed,
            scope: Scope::Session,
        }
    }

    /// Streams seeded from the operating system's entropy source.
    #[must_use]
    pub fn from_entropy() -> Self {
        Self::session(rand::rng().next_u64())
    }

    /// Creates the generator for `stream`.
    #[must_use]
    pub fn rng(self, stream: Stream) -> StdRng {
        // The ChaCha key is an injective encoding of (seed, scope, stream), so every stream
        // has a distinct key, and ChaCha's outputs for distinct keys are independent.
        const DOMAIN: u64 = u64::from_le_bytes(*b"qdk-rng1");
        let (scope, shot) = match self.scope {
            Scope::Session => (0, 0),
            Scope::Shot(shot) => (1, shot),
        };
        let stream = match stream {
            Stream::Measurement => 0,
            Stream::Noise => 1,
            Stream::Classical => 2,
        };
        let mut key = [0; 32];
        for (chunk, word) in
            key.chunks_exact_mut(8)
                .zip([DOMAIN, self.seed, shot, (scope << 8) | stream])
        {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        StdRng::from_seed(key)
    }
}
