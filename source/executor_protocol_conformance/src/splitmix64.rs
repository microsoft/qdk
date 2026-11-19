// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SplitMix64`], the generator of the kit's draws: the values the
//! wrapper prefills, those the kit's executor writes, the shot a run holds
//! that its script never names, and the patterns of each drawn trial (the
//! shots' ids and order, their batches and groups, where a run fails, the
//! conditions raised), each a fixed function of its seed, so that a check
//! gives the same result each time.

/// The `SplitMix64` generator: a stream of draws from a seed.
pub(crate) struct SplitMix64(u64);

impl SplitMix64 {
    /// The stream seeded with `seed`.
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next draw.
    pub(crate) fn next_u64(&mut self) -> u64 {
        let draw = mix(self.0);
        self.0 = self.0.wrapping_add(GOLDEN_GAMMA);
        draw
    }
}

/// `SplitMix64`'s step: 2⁶⁴ over the golden ratio, rounded down, as the
/// reference generator (Java's `SplittableRandom`) has it.
const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// `SplitMix64`'s output for the state `z`: a well-mixed function of it.
pub(crate) fn mix(z: u64) -> u64 {
    let z = z.wrapping_add(GOLDEN_GAMMA);
    // The reference generator's shifts and multipliers.
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
