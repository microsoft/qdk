// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SplitMix64`]: a stream of 64-bit values, and of uniforms in [0, 1),
//! from a `u64` seed.
//!
//! The shared sampler
//! ([`sample_by_conditionals`](super::sample_by_conditionals)) draws its
//! uniforms from it, so a seed's sample is reproducible. Tests that need
//! random circuits draw them from it too, here and in the crates that
//! implement or use an engine.
//!
//! It is `SplitMix64` (Steele, Lea and Flood, 2014; Vigna's reference
//! implementation, `splitmix64.c`). It is written here, not taken from a
//! dependency such as `rand`: it is small, and a seed's values then never
//! change with a dependency's version.

/// The `SplitMix64` generator: the stream of 64-bit values, and of uniforms
/// in [0, 1), that a seed gives.
///
/// A seed's stream is fixed by this crate: the same on every run,
/// platform and release, and the same as the reference implementation's.
/// It is predictable from its values, so it is not for cryptography.
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// The start of the stream that `seed` gives; any `u64` is a seed.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64-bit value of the stream, any `u64`.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// The next value's top 53 bits as a multiple of 2⁻⁵³: a uniform in
    /// [0, 1), whose every value an `f64` holds exactly, 0 and 1 − 2⁻⁵³
    /// included. Like [`next_u64`](Self::next_u64), it takes one value of
    /// the stream.
    pub fn next_uniform(&mut self) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "53 bits fit an f64")]
        let uniform = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        uniform
    }
}
