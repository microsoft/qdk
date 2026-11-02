// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Shots: their ids, and the set of shots a run or a batch runs on.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ops::{Deref, Range, RangeInclusive};

use crate::Rejected;

/// A shot's number within a run. Shots are numbered by the caller.
pub type ShotId = u32;

/// A set of shots, in the caller's order: the shots J of a run, or the
/// shots Jₖ ⊆ J a batch runs on. Never empty, and no shot is named
/// twice.
///
/// Needed because "run the instructions on each shot of Jₖ" has no answer for
/// a shot named twice (run it twice, or once?). A set by type is checked once,
/// where the shots are chosen, and never again by the executors downstream.
/// That matters most where J is split into parts, each run on its own
/// executor: a check in each executor sees only its part, while the parts of a
/// `ShotSet` cannot share a shot. An executor runs a batch's shots in the
/// set's order.
///
/// A set of one shot is held inline: made by [`ShotSet::one`], it
/// allocates nothing, so an executor that splits a call per shot, or a
/// driver of one shot, makes one per call for free. The other constructors
/// may allocate before they see that the set holds one shot.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ShotSet(Shots);

/// The shots of a set: one inline, or several on the heap. Never `Many`
/// of one shot, so that the derived `Eq` and `Hash` compare the shots.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Shots {
    One(ShotId),
    Many(Box<[ShotId]>),
}

impl From<Box<[ShotId]>> for Shots {
    /// `shots`, which is not empty.
    fn from(shots: Box<[ShotId]>) -> Self {
        match *shots {
            [one] => Self::One(one),
            _ => Self::Many(shots),
        }
    }
}

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
        Ok(Self(shots.into_boxed_slice().into()))
    }

    /// The set of one shot, `shot`. Allocates nothing.
    #[must_use]
    pub const fn one(shot: ShotId) -> Self {
        Self(Shots::One(shot))
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
        let part: Box<[ShotId]> = self.iter().copied().filter(|&j| keep(j)).collect();
        (!part.is_empty()).then(|| Self(part.into()))
    }

    /// The set of `part`, shots taken from one set in its order: distinct
    /// and not empty, so nothing is checked again.
    pub(crate) fn part(part: Vec<ShotId>) -> Self {
        debug_assert!(!part.is_empty(), "a part of a set is not empty");
        Self(part.into_boxed_slice().into())
    }

    /// Checks that the set is the one shot `held`, so that an executor of
    /// one shot, called first with this check, runs nothing for another.
    ///
    /// # Errors
    ///
    /// [`Rejected::ShotNotHeld`] for the first shot of the set other than
    /// `held`.
    pub fn check_only(&self, held: ShotId) -> Result<(), Rejected> {
        match self.iter().find(|&&j| j != held) {
            None => Ok(()),
            Some(&other) => Err(Rejected::ShotNotHeld(other)),
        }
    }

    /// The shots, in the order they were given.
    #[must_use]
    pub fn as_slice(&self) -> &[ShotId] {
        match &self.0 {
            Shots::One(shot) => std::slice::from_ref(shot),
            Shots::Many(shots) => shots,
        }
    }

    /// The shots in increasing order, to search them or to go through them
    /// beside a list by shot: the set itself when given so, as `1..=N` and
    /// its parts are, with no copy; else a sorted copy.
    pub(crate) fn sorted(&self) -> Cow<'_, [ShotId]> {
        let shots = self.as_slice();
        if shots.is_sorted() {
            Cow::Borrowed(shots)
        } else {
            let mut sorted = shots.to_vec();
            sorted.sort_unstable();
            Cow::Owned(sorted)
        }
    }
}

/// Read-only, so that a set cannot be changed into a list with a repeat.
impl Deref for ShotSet {
    type Target = [ShotId];

    fn deref(&self) -> &[ShotId] {
        self.as_slice()
    }
}

/// The shots, in order, as `ShotSet([3, 4])`, whether held inline or not.
impl std::fmt::Debug for ShotSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ShotSet").field(&self.as_slice()).finish()
    }
}

impl<'a> IntoIterator for &'a ShotSet {
    type Item = &'a ShotId;
    type IntoIter = std::slice::Iter<'a, ShotId>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
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

/// Shots as a caller writes them: a `Range` or a `RangeInclusive`; a
/// `Vec`, an array or a [`ShotSet`], owned or borrowed (as `IntoIterator`
/// takes a `Vec` or a `&Vec`); or a slice. They are checked as a set where
/// a job is given to a chain ([`Chain::run`](crate::Chain::run)), which
/// refuses shots that are not a set with the job's other faults. A lone
/// number is not shots, since it could name one shot or count them: `n`
/// shots are written `0..n`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be a job's shots",
    label = "not a job's shots",
    note = "a job's shots are a `Range` or a `RangeInclusive` of `ShotId`s; a `Vec`, an array or a \
            `ShotSet` of them, owned or borrowed; or a slice of them; collect an iterator into a `Vec` first",
    note = "for `n` shots, write `0..n`: a lone number is not shots, since it could name one shot or count them"
)]
pub trait IntoShotSet {
    /// These shots as a set.
    ///
    /// # Errors
    ///
    /// As [`ShotSet::new`]: [`Rejected::NoShots`] when there are none, and
    /// [`Rejected::RepeatedShot`] for the least shot named twice.
    fn into_shot_set(self) -> Result<ShotSet, Rejected>;
}

impl IntoShotSet for ShotSet {
    fn into_shot_set(self) -> Result<ShotSet, Rejected> {
        Ok(self)
    }
}

impl IntoShotSet for Vec<ShotId> {
    fn into_shot_set(self) -> Result<ShotSet, Rejected> {
        ShotSet::new(self)
    }
}

/// Copied, as [`ShotSet::try_from`] copies them.
impl IntoShotSet for &[ShotId] {
    fn into_shot_set(self) -> Result<ShotSet, Rejected> {
        ShotSet::new(self.to_vec())
    }
}

impl<const N: usize> IntoShotSet for [ShotId; N] {
    fn into_shot_set(self) -> Result<ShotSet, Rejected> {
        ShotSet::new(self.to_vec())
    }
}

/// Copied, as for a slice.
impl<const N: usize> IntoShotSet for &[ShotId; N] {
    fn into_shot_set(self) -> Result<ShotSet, Rejected> {
        ShotSet::new(self.to_vec())
    }
}

/// Copied, as for a slice.
impl IntoShotSet for &Vec<ShotId> {
    fn into_shot_set(self) -> Result<ShotSet, Rejected> {
        ShotSet::new(self.clone())
    }
}

/// Cloned, so that the caller keeps its set, to compare later runs on it.
impl IntoShotSet for &ShotSet {
    fn into_shot_set(self) -> Result<ShotSet, Rejected> {
        Ok(self.clone())
    }
}

/// `0..1000`: the shots `0` to `999`, as a sampler's thousand shots.
impl IntoShotSet for Range<ShotId> {
    fn into_shot_set(self) -> Result<ShotSet, Rejected> {
        ShotSet::new(self.collect())
    }
}

impl IntoShotSet for RangeInclusive<ShotId> {
    fn into_shot_set(self) -> Result<ShotSet, Rejected> {
        ShotSet::range(self)
    }
}

/// Where each shot of a set lives, in the order added. Shots are usually
/// added in order, `first..=last`, which needs no lookup; any other order
/// falls back to a map.
#[derive(Debug)]
pub(crate) enum ShotIndex {
    Range { first: ShotId, len: usize },
    Map(BTreeMap<ShotId, usize>),
}

impl Default for ShotIndex {
    fn default() -> Self {
        Self::Range { first: 0, len: 0 }
    }
}

impl ShotIndex {
    /// Adds `shot`, which it does not hold, after the shots it holds.
    pub(crate) fn push(&mut self, shot: ShotId) {
        if let Self::Range { first, len } = self {
            if *len == 0 {
                *first = shot;
            }
            if shot
                .checked_sub(*first)
                .and_then(|i| usize::try_from(i).ok())
                == Some(*len)
            {
                *len += 1;
                return;
            }
            let map = (0..*len).zip(*first..=ShotId::MAX).map(|(i, j)| (j, i));
            *self = Self::Map(map.collect());
        }
        if let Self::Map(map) = self {
            let at = map.len();
            map.insert(shot, at);
        }
    }

    /// Where shot `shot` lives, if it was added.
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
