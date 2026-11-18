// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The stream each shot of an [`MpsResumer`] draws its measurements and
//! resets from: [`shot_stream`] seeds it from the run's seed and the
//! shot's id, and [`SplitMix64`] generates it.
//!
//! A shot's records must depend only on the program, the seed, the shot's
//! id and the settings, not on the shots run beside it (to what precision:
//! [`MpsResumer`]'s doc): so each shot needs a stream of uniforms of its
//! own, fixed by the run's seed and its id.
//! The tests need streams that are the same on every run too, and use the
//! generator alone.
//!
//! The generator is `SplitMix64` (Steele, Lea and Flood, 2014), written
//! here rather than taken from a dependency (`rand_xoshiro` has one, for
//! instance, which the workspace does not use): small, and fixed by this
//! crate, so a seed's values do not change with a dependency's version.
//! `tensornet` has its own, internal to its sampler; this one gives the
//! same values.
//!
//! **Known issue:** the two copies are one generator written twice. One
//! copy needs `tensornet` to make its generator public; until then, a test
//! here pins the published values, which both give. It blocks nothing.
//!
//! Each step adds the odd constant `0x9E37_79B9_7F4A_7C15` (2⁶⁴ over the
//! golden ratio) to the state, wrapping, and returns the state mixed by a
//! finalizer of two multiplications and three shifts: the values of the
//! published algorithm. A uniform is the top 53 bits of the next value, as
//! a multiple of 2⁻⁵³, in [0, 1). It is no cryptographic generator.

use crate::qir::QirShotId;

#[cfg(doc)]
use super::MpsResumer;

/// The stream shot `shot` draws from under the run seed `seed`:
/// `SplitMix64` seeded with `mix(mix(seed) ^ shot)`, where `mix(x)` is the
/// first value of `SplitMix64::new(x)`. So a shot's stream is fixed by the
/// seed and its id alone, and shots with close ids, or close seeds, draw
/// unrelated values.
pub(crate) fn shot_stream(seed: u64, shot: QirShotId) -> SplitMix64 {
    fn mix(x: u64) -> u64 {
        SplitMix64::new(x).next_u64()
    }
    SplitMix64::new(mix(mix(seed) ^ u64::from(shot.0)))
}

/// The `SplitMix64` generator: the stream of 64-bit values, and of uniforms
/// in [0, 1), that a seed gives.
pub(crate) struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// The stream that `seed` gives; any `u64` is a seed.
    #[must_use]
    pub(crate) const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next value of the stream.
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// The next value's top 53 bits as a multiple of 2⁻⁵³: a uniform in
    /// [0, 1), whose every value an `f64` holds exactly, 0 and 1 − 2⁻⁵³
    /// included. So a shot that reads One when its uniform is below P(One)
    /// never reads One at P(One) = 0, and always reads it at P(One) = 1.
    pub(crate) fn next_uniform(&mut self) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "53 bits fit an f64")]
        let uniform = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        uniform
    }
}
