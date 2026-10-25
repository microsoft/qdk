// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Shots: the set of shots a session holds or a submission runs on, each
//! shot's seed, and the seeds of the components that draw random numbers in
//! a shot.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::{Deref, RangeInclusive};

use crate::{Rejected, ShotId};

/// A set of shots, in the caller's order: the shots J a session holds, or the
/// shots Jₖ ⊆ J a submission runs on. Never empty, and no shot is named
/// twice.
///
/// Needed because "run the batch on each shot of Jₖ" has no answer for a shot
/// named twice (run it twice, or once?). A set by type is checked once, where
/// the shots are chosen, and never again by the executors below. That matters
/// most where J is split into parts, each opened on its own: a check in each
/// opener sees only its part, while the parts of a `ShotSet` cannot share a
/// shot. A per-shot session runs a submission's shots in the set's order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShotSet(Box<[ShotId]>);

impl ShotSet {
    /// `shots` as a set.
    ///
    /// Linear in the number of shots when they are increasing, as `1..=N`
    /// and its parts are.
    ///
    /// # Errors
    ///
    /// [`Rejected::NoShots`] when `shots` is empty, and
    /// [`Rejected::RepeatedShot`] for the least shot named twice.
    pub fn new(shots: Vec<ShotId>) -> Result<Self, Rejected> {
        if shots.is_empty() {
            return Err(Rejected::NoShots);
        }
        if !shots.windows(2).all(|pair| pair[0] < pair[1]) {
            let mut sorted = shots.clone();
            sorted.sort_unstable();
            if let Some(pair) = sorted.windows(2).find(|pair| pair[0] == pair[1]) {
                return Err(Rejected::RepeatedShot(pair[0]));
            }
        }
        Ok(Self(shots.into_boxed_slice()))
    }

    /// The set of one shot, `shot`.
    #[must_use]
    pub fn one(shot: ShotId) -> Self {
        Self(Box::new([shot]))
    }

    /// The shots of `range`, in order.
    ///
    /// # Errors
    ///
    /// [`Rejected::NoShots`] when `range` is empty.
    pub fn range(range: RangeInclusive<ShotId>) -> Result<Self, Rejected> {
        Self::new(range.collect())
    }

    /// The shots that `keep` keeps, in order, or `None` when it keeps none. A
    /// part of a set is a set, so nothing is checked again: this is how a
    /// driver splits Jₖ by outcome.
    pub fn subset(&self, mut keep: impl FnMut(ShotId) -> bool) -> Option<Self> {
        let part: Box<[ShotId]> = self.0.iter().copied().filter(|&j| keep(j)).collect();
        (!part.is_empty()).then_some(Self(part))
    }

    /// The shots, in the order they were given.
    #[must_use]
    pub fn as_slice(&self) -> &[ShotId] {
        &self.0
    }
}

/// Read-only, so that a set cannot be changed into a list with a repeat.
impl Deref for ShotSet {
    type Target = [ShotId];

    fn deref(&self) -> &[ShotId] {
        &self.0
    }
}

impl<'a> IntoIterator for &'a ShotSet {
    type Item = &'a ShotId;
    type IntoIter = std::slice::Iter<'a, ShotId>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl TryFrom<Vec<ShotId>> for ShotSet {
    type Error = Rejected;

    fn try_from(shots: Vec<ShotId>) -> Result<Self, Rejected> {
        Self::new(shots)
    }
}

impl TryFrom<&[ShotId]> for ShotSet {
    type Error = Rejected;

    fn try_from(shots: &[ShotId]) -> Result<Self, Rejected> {
        Self::new(shots.to_vec())
    }
}

/// Where each shot of a session lives. J is usually `first..=last`, which
/// needs no lookup; any other J falls back to a map.
pub(crate) enum ShotIndex {
    Range { first: ShotId, len: usize },
    Map(BTreeMap<ShotId, usize>),
}

impl ShotIndex {
    pub(crate) fn new(shots: &ShotSet) -> Self {
        let first = shots[0];
        if shots
            .iter()
            .zip(u64::from(first)..)
            .all(|(&j, expected)| u64::from(j) == expected)
        {
            return Self::Range {
                first,
                len: shots.len(),
            };
        }
        Self::Map(shots.iter().enumerate().map(|(i, &j)| (j, i)).collect())
    }

    /// Where shot `shot` lives, if the session holds it.
    pub(crate) fn get(&self, shot: ShotId) -> Option<usize> {
        match self {
            Self::Range { first, len } => {
                let i = usize::try_from(shot.checked_sub(*first)?).ok()?;
                (i < *len).then_some(i)
            }
            Self::Map(map) => map.get(&shot).copied(),
        }
    }
}

/// The seed of shot `j` of a session opened with `seed`: the same whatever
/// session or process runs the shot, so that any split of J gives each shot
/// the same draws.
///
/// It is one step of `SplitMix64` (Steele, Lea and Flood, "Fast splittable
/// pseudorandom number generators", OOPSLA 2014) applied to
/// `SplitMix64(seed) ^ j`:
///
/// - Two shots of a session never share a seed: a `SplitMix64` step and the
///   XOR with a constant are bijections of `u64`, so `j ↦ shot_seed(seed, j)`
///   is one-to-one. A hash, or a draw from a master generator, would only
///   make a collision unlikely, and two shots with one seed draw alike.
/// - The seeds do not change with a dependency's version: the constants are
///   `SplitMix64`'s own, fixed here.
///
/// It is the seed of the terminal's random numbers in the shot,
/// [`ShotContext::rng_seed`] of [`ComponentId::TERMINAL`].
#[must_use]
pub const fn shot_seed(seed: u64, j: ShotId) -> u64 {
    splitmix(splitmix(seed) ^ j as u64)
}

/// One step of `SplitMix64`, with its own constants: a bijection of `u64`.
const fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The name a component of a stack draws random numbers under: what selects
/// its seed in each shot.
///
/// Needed because with one generator per shot, an adapter that draws (noise)
/// would shift the terminal's draws, so a shot's outcomes would depend on
/// which adapters are in the stack. With a seed per component, component `c`
/// of shot `j` draws from its own seed, whatever else is in the stack, and
/// inserting an adapter that draws without effect changes no outcome.
///
/// Ids are stable names, not positions in the stack, so that adding a
/// component does not change the draws of the others. The terminal's id is
/// fixed; any other is a hash of a name the builder chooses
/// ([`ComponentId::named`]). It is 32 bits so that a (shot, component) pair
/// fits one `u64`, which keeps [`ShotContext::rng_seed`] one-to-one; the
/// price is that two names may share an id, which a stack refuses when it is
/// built ([`BuildError::RngInUse`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ComponentId(u32);

impl ComponentId {
    /// The terminal's: its seed in shot `j` is [`shot_seed`]`(seed, j)`.
    pub const TERMINAL: Self = Self(0);

    /// The id of the component named `name`: FNV-1a 64 (Fowler, Noll and Vo)
    /// over its UTF-8 bytes, XOR-folded to 32 bits. Its constants are fixed
    /// here, so an id, and the seeds it selects, do not change with a
    /// dependency's version. A name that folds to 0 gets 1, since 0 is the
    /// terminal's.
    #[must_use]
    pub const fn named(name: &str) -> Self {
        let bytes = name.as_bytes();
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
        Self(if folded == 0 { 1 } else { folded })
    }

    /// The id as a number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::TERMINAL {
            write!(f, "the terminal's random numbers")
        } else {
            write!(f, "component random numbers {:#010x}", self.0)
        }
    }
}

/// Shot `j`'s context: what each component of the shot draws from. Built
/// only from the session's seed and `j`, so it is the same whichever session
/// or process runs the shot.
///
/// Needed as a type of its own because a session's seed and a shot's seed
/// are both `u64`: a shot made from the session's seed instead of its own
/// compiles, and then every shot draws the same values. A `ShotContext`
/// names both.
///
/// It gives seeds, not generators, so that this crate depends on no random
/// number library: each component seeds the generator it uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShotContext {
    seed: u64,
    shot: ShotId,
}

impl ShotContext {
    /// Shot `shot` of a session opened with `seed`.
    #[must_use]
    pub const fn new(seed: u64, shot: ShotId) -> Self {
        Self { seed, shot }
    }

    /// The session's seed.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// The shot, `j`.
    #[must_use]
    pub const fn shot(&self) -> ShotId {
        self.shot
    }

    /// The seed of component `c` in this shot: one step of `SplitMix64`
    /// applied to `SplitMix64(seed) ^ (c · 2³² + j)`, the construction of
    /// [`shot_seed`] with the component in the high half.
    ///
    /// - The terminal's is [`shot_seed`]`(seed, j)` exactly, since its id is
    ///   0.
    /// - No two (shot, component) pairs of a session share a seed:
    ///   `c · 2³² + j` is one-to-one over 32-bit shots and ids, and the rest
    ///   is a bijection of `u64`.
    /// - Not guaranteed: seeds of different sessions may meet, as for
    ///   [`shot_seed`], and two names may share an id ([`ComponentId`]).
    #[must_use]
    pub const fn rng_seed(&self, c: ComponentId) -> u64 {
        splitmix(splitmix(self.seed) ^ ((c.0 as u64) << 32 | self.shot as u64))
    }
}

/// The ids of the components that draw random numbers at or below a
/// component of a stack ([`Capabilities::rngs`](crate::Capabilities::rngs)):
/// what a component that draws checks its own id against when it is built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentRngs(Vec<ComponentId>);

impl ComponentRngs {
    /// A terminal's: its own id only.
    #[must_use]
    pub fn terminal() -> Self {
        Self(vec![ComponentId::TERMINAL])
    }

    /// Whether a component at or below draws with `id`.
    #[must_use]
    pub fn contains(&self, id: ComponentId) -> bool {
        self.0.contains(&id)
    }

    /// These and `id`, for a component that draws with `id` above them.
    ///
    /// # Errors
    ///
    /// [`BuildError::RngInUse`] when a component below already draws with
    /// `id`: the two would draw the same numbers.
    pub fn with(mut self, id: ComponentId) -> Result<Self, BuildError> {
        if self.contains(id) {
            return Err(BuildError::RngInUse(id));
        }
        self.0.push(id);
        Ok(self)
    }

    /// The ids: the terminal's first, then from the bottom of the stack up.
    #[must_use]
    pub fn as_slice(&self) -> &[ComponentId] {
        &self.0
    }
}

impl Default for ComponentRngs {
    /// A terminal's.
    fn default() -> Self {
        Self::terminal()
    }
}

/// Why a component of a stack cannot be built over the components below it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BuildError {
    /// A component below already draws with this id, so the two would draw
    /// the same numbers in every shot.
    RngInUse(ComponentId),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::RngInUse(id) => write!(
                f,
                "{id} are already drawn from below: two components would share them"
            ),
        }
    }
}

impl std::error::Error for BuildError {}
