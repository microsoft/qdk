// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SplitMix64`]: the generator the shared sampler
//! ([`sample_by_conditionals`](super::sample_by_conditionals))
//! draws its uniforms from, and the tests their streams.
//!
//! `sample` draws every shot from one `u64` seed, and the same seed gives
//! the same values on the same engine. So the sampler needs a stream of
//! uniforms from that seed, and the tests need streams of numbers that are
//! the same on every run.
//!
//! It is `SplitMix64` (Steele, Lea and Flood, 2014), written here rather
//! than taken from a dependency: small, and fixed by this crate, so a
//! seed's values do not change with a dependency's version. It is internal:
//! an engine reaches it only through the sampler.
//!
//! Each step adds the odd constant `0x9E37_79B9_7F4A_7C15` (2⁶⁴ over the
//! golden ratio) to the state, wrapping, and returns the state mixed by a
//! finalizer of two multiplications and three shifts: the values of the
//! published algorithm. A uniform is the top 53 bits of the next value, as
//! a multiple of 2⁻⁵³, in [0, 1). It is no cryptographic generator.

/// The `SplitMix64` generator: the stream of 64-bit values, and of uniforms
/// in [0, 1), that a seed gives.
#[derive(Clone, Debug)]
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
    /// included.
    pub(crate) fn next_uniform(&mut self) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "53 bits fit an f64")]
        let uniform = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        uniform
    }
}
