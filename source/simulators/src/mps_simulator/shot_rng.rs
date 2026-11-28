// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The random number generator (RNG) each shot of an [`MpsResumer`] draws
//! the uniforms of its measurements and resets from, fixed by the run's
//! seed and the shot's id ([`MpsResumer`] states the promise this keeps).
//! Another runner that seeds each shot's RNG by the same rule draws the
//! same values.
//!
//! [`shot_rng`] seeds it, and `tensornet`'s [`SplitMix64`] generates its
//! values (G. L. Steele Jr., D. Lea and C. H. Flood, "Fast
//! splittable pseudorandom number generators", OOPSLA 2014).

use tensornet::sampling::SplitMix64;

use crate::qir::QirShotId;

#[cfg(doc)]
use super::MpsResumer;

/// The RNG shot `shot` draws from under the run seed `seed`. It is
/// `SplitMix64` seeded with `mix(mix(seed) ^ shot)`, where `mix(x)` is the
/// first value of `SplitMix64::new(x)`. So shots with close ids, or close
/// seeds, draw unrelated values.
pub(crate) fn shot_rng(seed: u64, shot: QirShotId) -> SplitMix64 {
    /// The first value of `SplitMix64::new(x)`: a scramble of `x`.
    fn mix(x: u64) -> u64 {
        SplitMix64::new(x).next_u64()
    }
    SplitMix64::new(mix(mix(seed) ^ u64::from(shot.0)))
}

#[cfg(test)]
mod tests;
