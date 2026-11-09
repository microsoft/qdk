// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SplitMix64`]: the generator each shot of an
//! [`MpsResumer`](super::MpsResumer) draws its measurements from, and the
//! hash that derives a shot's seed from the run's seed.
//!
//! A shot's records must depend only on the program, the seed, the shot's
//! id and the settings, and must match what the executor protocol
//! (planned) will draw for that shot: so the resumer needs a stream of
//! uniforms fixed by a seed, and a hash fixed by the protocol's seed
//! formula (`shot_stream`). The tests need streams that are the same on
//! every run too.
//!
//! It is `SplitMix64` (Steele, Lea and Flood, 2014), written here rather
//! than taken from a dependency: small, and fixed by this crate, so a
//! seed's values do not change with a dependency's version. `tensornet`
//! has its own, internal to its sampler; this one gives the same values.
//!
//! Each step adds the odd constant `0x9E37_79B9_7F4A_7C15` (2⁶⁴ over the
//! golden ratio) to the state, wrapping, and returns the state mixed by a
//! finalizer of two multiplications and three shifts: the values of the
//! published algorithm. A uniform is the top 53 bits of the next value, as
//! a multiple of 2⁻⁵³, in [0, 1). It is no cryptographic generator.

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
