// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Which shots a run or a batch runs on: a [`ShotSet`], the shots in the
//! caller's order, never empty and none twice, each named by its
//! [`ShotId`].
//!
//! A batch runs its instructions on each of its shots, which has no answer
//! for a shot named twice (run it twice, or once?), nor for no shot at all.
//! So a set is checked once, where it is made, and no part that receives it
//! checks it again; a part that splits a set ([`ShotSet::select`]) gets sets
//! that are sound by construction. A set keeps the caller's order, because a
//! batch's readouts come back as a row per shot, in that order.
//!
//! Adaptive programs send many batches of one shot, so a set of one shot is
//! held inline and allocates nothing, however it is made.

mod shot_id;

pub use shot_id::ShotId;

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use smallvec::SmallVec;

/// Shots in the caller's order, never empty, none twice: the shots of a
/// run, or of a batch.
///
/// Made by [`ShotSet::ids`], or [`ShotSet::try_ids`], which returns the fault
/// instead of panicking; a part splits one with [`ShotSet::select`]. Two sets
/// are equal when they hold the same shots in the same order.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ShotSet(SmallVec<[ShotId; 1]>);

impl ShotSet {
    /// The shots `ids`, in their order: `ids(0..4)`, `ids([2, 5, 6, 11])`,
    /// or two ranges joined, `ids((0..4).chain(8..10))`.
    ///
    /// Linear in the number of shots, with no allocation beyond the set's
    /// own, when the ids increase, as ranges do.
    ///
    /// # Panics
    ///
    /// When `ids` is empty or names a shot twice, with the fault
    /// [`try_ids`](Self::try_ids) returns.
    #[must_use]
    #[track_caller]
    pub fn ids(ids: impl IntoIterator<Item = u32>) -> Self {
        match Self::try_ids(ids) {
            Ok(set) => set,
            Err(fault) => panic!("{fault}"),
        }
    }

    /// The shots `ids`, in their order, or why they are not a set.
    ///
    /// # Errors
    ///
    /// [`ShotSetFault::NoShots`] when `ids` is empty, and
    /// [`ShotSetFault::RepeatedShot`] for the first shot, in their order,
    /// that an earlier one names already.
    pub fn try_ids(ids: impl IntoIterator<Item = u32>) -> Result<Self, ShotSetFault> {
        let shots: SmallVec<[ShotId; 1]> = ids.into_iter().map(ShotId::new).collect();
        if shots.is_empty() {
            return Err(ShotSetFault::NoShots);
        }
        match first_repeat(&shots) {
            Some(shot) => Err(ShotSetFault::RepeatedShot(shot)),
            None => Ok(Self(shots)),
        }
    }

    /// How many shots the set holds: at least one.
    #[must_use]
    #[expect(
        clippy::len_without_is_empty,
        reason = "a shot set is never empty, so `is_empty` would always be false"
    )]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The shots, in the caller's order.
    pub fn iter(&self) -> impl Iterator<Item = ShotId> + '_ {
        self.0.iter().copied()
    }

    /// The shots at `positions` in this set, in the order of `positions`,
    /// or `None` when `positions` is empty, since a set is never empty. A
    /// driver splits a batch's shots by their readouts this way; the part is
    /// a set by construction, so nothing is checked again but the positions.
    ///
    /// # Panics
    ///
    /// When a position is not below [`len`](Self::len), or is given twice,
    /// which would name its shot twice: both are the caller's bug, as an
    /// index out of a slice's bounds is.
    #[must_use]
    #[track_caller]
    pub fn select(&self, positions: &[usize]) -> Option<Self> {
        if positions.is_empty() {
            return None;
        }
        if let Some(twice) = first_repeat(positions) {
            panic!("position {twice} is selected twice, which would name its shot twice");
        }
        for &position in positions {
            assert!(
                position < self.len(),
                "position {position} is beyond the set's {} shots",
                self.len()
            );
        }
        Some(Self(
            positions.iter().map(|&position| self.0[position]).collect(),
        ))
    }
}

/// The shots, in their order, as `ShotSet([2, 5])`, so a failing test shows
/// the shots it named.
impl fmt::Debug for ShotSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ShotSet(")?;
        f.debug_list()
            .entries(self.0.iter().map(|shot| shot.get()))
            .finish()?;
        f.write_str(")")
    }
}

/// Why shots given to [`ShotSet::try_ids`] are not a set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShotSetFault {
    /// No shot was given.
    NoShots,
    /// This shot was given twice: the first, in the order given, that an
    /// earlier one names already.
    RepeatedShot(ShotId),
}

impl fmt::Display for ShotSetFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoShots => f.write_str("a shot set needs at least one shot"),
            Self::RepeatedShot(shot) => write!(f, "shot {shot} is named twice in a shot set"),
        }
    }
}

impl Error for ShotSetFault {}

/// The first item of `items`, in their order, that an earlier one equals.
/// Linear, with no allocation, when the items increase, as ranges and the
/// positions a filter keeps do.
fn first_repeat<T: Ord + Copy>(items: &[T]) -> Option<T> {
    if items.is_sorted_by(|a, b| a < b) {
        return None;
    }
    let mut seen = BTreeSet::new();
    items.iter().copied().find(|&item| !seen.insert(item))
}
