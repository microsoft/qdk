// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Where a run's random numbers come from: the [`Seed`] every part passes
//! down, from which each part that draws, the executor or another, takes
//! its streams.
//!
//! A replayable part ([`Offer::replayable`](crate::Offer::replayable))
//! draws each shot from streams of its own, so that, on a chain whose parts
//! are replayable, a shot's records depend only on the chain, the run's
//! seed and the shot's id: not on the shots run beside it, how its
//! instructions are put in batches, or which other parts draw. So no stream
//! is shared, and none is chosen by a part's place in the chain, where
//! inserting a part would shift the draws of those below it:
//!
//! - a replayable executor seeds each shot's stream with [`Seed::shot`],
//!   from the run's seed and the shot's id alone, not its place in a batch,
//!   at the shot's first batch, and keeps it until the shot ends;
//! - a replayable part that draws above it, such as a noise model, takes a
//!   seed of its own with [`Seed::derive`] and a key of its own, such as
//!   `"bit_flip"`, and seeds each shot's stream with that seed's
//!   [`shot`](Seed::shot), so it never draws the executor's numbers, and
//!   inserting it shifts no other part's.
//!
//! A part that is not replayable
//! ([`Offer::not_replayable`](crate::Offer::not_replayable)) still draws
//! from the run's seed, but may draw a run's shots from one stream, such as
//! a sampler that draws them in one call.
//!
//! A stream is a `u64` that the part seeds its own generator with, so this
//! crate depends on no random-number library. The streams are computed with
//! constants fixed here, `SplitMix64`'s and FNV-1a's, so a run keeps its
//! streams across platforms and the versions of Rust and of any dependency.

use crate::ShotId;

/// The seed an `execute` carries: the run's, which each part passes on,
/// unchanged, to every `execute` it makes below. A part that draws derives
/// its own from it ([`derive`](Self::derive)).
///
/// The runner makes one from the run's seed. A remote executor, one that
/// carries an `execute` to a chain in another process, sends its seed's
/// bits ([`to_bits`](Self::to_bits)), and that process rebuilds the seed
/// from those bits ([`from_bits`](Self::from_bits)), and from no other
/// number, so that every part of a run, in either process, draws each
/// shot's streams from the run's seed. It holds the run's seed after one
/// `SplitMix64` step, or, once derived, that value plus the key's id in
/// its high half.
#[derive(Clone, Copy, Debug)]
pub struct Seed(u64);

impl Seed {
    /// Shot `shot`'s stream: what a part seeds its generator for that shot
    /// with, at the shot's first batch, and keeps until the shot ends.
    ///
    /// It depends only on this seed and the shot's id, so a shot draws the
    /// same alone or with any other shots, in any batches and any order.
    /// Two shots never share a stream: it is one step of `SplitMix64`
    /// (Steele, Lea and Flood, "Fast splittable pseudorandom number
    /// generators", OOPSLA 2014) applied to this seed's value XOR the
    /// shot's id (for the run's seed `s`, `SplitMix64(s) ^ shot`), and both
    /// the step and the XOR are bijections of `u64`.
    ///
    /// For the run's seed, each shot's stream is a fixed function of the
    /// run's seed and the shot's id, the same on every platform, so a
    /// simulator that seeds its shots the same way gives the same records
    /// alone and as a chain's executor.
    #[must_use]
    pub const fn shot(self, shot: ShotId) -> u64 {
        splitmix(self.0 ^ shot.get() as u64)
    }

    /// The seed of a part that draws under `key`, a name of its own such as
    /// `"bit_flip"`: streams apart from the executor's and from every other
    /// key's.
    ///
    /// A part derives it from the seed its `execute` carries, and draws each
    /// shot's numbers from its [`shot`](Self::shot) stream. Its stream for a
    /// shot is never the run's stream for any shot, nor another key's for
    /// any shot, unless the two keys' 32-bit ids are equal: the id is
    /// FNV-1a 64 (Fowler, Noll and Vo) of the key's UTF-8 bytes, folded to
    /// 32 bits and never 0, and it is added to the high half of the seed,
    /// which [`shot`](Self::shot) leaves as it is. So two parts with one key
    /// share their streams, and k keys share an id with probability about
    /// k²/2³³. A part with several streams derives one key for each from the
    /// seed it got, rather than deriving a derived seed again: ids add, so
    /// a seed derived twice may be another key's, or the run's own.
    #[must_use]
    pub const fn derive(self, key: &str) -> Self {
        Self(self.0.wrapping_add((key_id(key) as u64) << 32))
    }

    /// The seed of a run with seed `seed`, as the runner gives it to the
    /// driver.
    pub(crate) const fn new(seed: u64) -> Self {
        Self(splitmix(seed))
    }

    /// This seed as a `u64`, for a remote executor to send, as [`Seed`]
    /// says.
    #[must_use]
    pub const fn to_bits(self) -> u64 {
        self.0
    }

    /// The seed whose [`to_bits`](Self::to_bits) is `bits`, for the process
    /// that receives a seed's bits from a remote executor, as [`Seed`]
    /// says. A
    /// seed made from any other number is not the run's, so a shot's
    /// records would no longer depend only on the run's seed.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }
}

/// The id of `key`: FNV-1a 64 of its UTF-8 bytes, XOR-folded to 32 bits, with
/// 1 for a key that folds to 0, whose streams would otherwise be the run's.
const fn key_id(key: &str) -> u32 {
    let bytes = key.as_bytes();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
        i += 1;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the XOR-fold keeps the low 32 bits on purpose"
    )]
    let folded = (hash ^ (hash >> 32)) as u32;
    if folded == 0 { 1 } else { folded }
}

/// One step of `SplitMix64`, with its published constants: a bijection of
/// `u64`.
const fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
