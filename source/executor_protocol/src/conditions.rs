// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot's conditions: what a component found wrong with a shot without
//! ending it, such as a code's runtime that rejects a shot, finds its
//! checks inconsistent, or cannot determine a value. They are per shot and
//! merge by union: once raised, a condition stays raised for the shot. Each
//! outcome carries its shot's conditions so far
//! ([`ShotOutcome::conditions`]), as a code's runtime marks each result
//! with its status (`docs/replies.md`, §Shot conditions). It also holds
//! how an [`adapter_fn`](fn@crate::adapter_fn) keeps them: the conditions
//! raised on shots in a call ([`Raised`]), each shot's conditions so far
//! over its calls ([`ConditionsSoFar`]), and those of a call's shots that
//! mark the outcomes it gives ([`Marks`]).

use std::collections::BTreeMap;
use std::fmt;
use std::ops::{BitOr, BitOrAssign};

use crate::{ShotId, ShotOutcome};

/// The conditions raised on a shot, a set: each says that the shot's
/// results are suspect in its own way, while the shot runs on and its
/// readouts stand. A component raises them on the outcomes it gives; an
/// [`adapter_fn`](fn@crate::adapter_fn) above merges those it reads below
/// with its own, by union ([`BitOr`]), and marks its outcomes with them, so
/// that the caller sees each shot's conditions on its outcomes whatever
/// layer raised them.
///
/// A set of flags in one byte, so that an outcome stays as small as it was
/// without them, with room for eight; its field is private, so that adding
/// a condition breaks no caller. Its `Debug` names the flags, as in
/// `ShotConditions(REJECTED | UNRESOLVED)`.
///
/// ```
/// use executor_protocol::ShotConditions;
///
/// let conditions = ShotConditions::REJECTED | ShotConditions::UNRESOLVED;
/// assert!(conditions.contains(ShotConditions::REJECTED));
/// assert!(!conditions.contains(ShotConditions::INCONSISTENT));
/// assert!(conditions.contains(ShotConditions::NONE));
/// ```
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ShotConditions(u8);

impl ShotConditions {
    /// No condition: a clear shot.
    pub const NONE: Self = Self(0);
    /// The shot was flagged for rejection: a check it was asked to pass
    /// failed (a post-selection, a flag). The caller chooses what to do
    /// with it: a caller that post-selects drops the shot.
    pub const REJECTED: Self = Self(1);
    /// The shot's checks disagree with each other (a parity that cannot
    /// hold), so its corrected results may be wrong.
    pub const INCONSISTENT: Self = Self(1 << 1);
    /// A value the shot depends on could not be determined (a readout lost
    /// or never read, a check that could not be decided).
    pub const UNRESOLVED: Self = Self(1 << 2);

    /// The named conditions, in the order `Debug` lists them.
    const NAMED: [(Self, &'static str); 3] = [
        (Self::REJECTED, "REJECTED"),
        (Self::INCONSISTENT, "INCONSISTENT"),
        (Self::UNRESOLVED, "UNRESOLVED"),
    ];

    /// Whether no condition is raised.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every condition of `other` is raised in this set: always,
    /// for [`NONE`](Self::NONE).
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for ShotConditions {
    type Output = Self;

    /// The union: the conditions raised in either.
    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitOrAssign for ShotConditions {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl fmt::Debug for ShotConditions {
    /// `ShotConditions(NONE)`, or its flags joined by ` | `.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ShotConditions(")?;
        if self.is_empty() {
            f.write_str("NONE")?;
        }
        let mut sep = "";
        for (flag, name) in Self::NAMED {
            if self.contains(flag) {
                write!(f, "{sep}{name}")?;
                sep = " | ";
            }
        }
        f.write_str(")")
    }
}

/// The conditions of each shot an adapter served, merged by union over its
/// calls, so that each outcome it gives carries its shot's conditions so
/// far. Only shots with a condition are held: a run that raises none holds
/// nothing, and costs no lookup.
//
// Known issue, open (blocks neither this commit nor a later one): a shot's
// entry is held until the adapter is dropped, since nothing tells an
// adapter that a shot has ended. It costs one map entry per flagged shot:
// a `BTreeMap<u32, u8>` entry, 14.3 bytes each, nodes included, as
// measured with shots flagged in ascending order (which leaves each leaf
// about half full), so a run of a million shots that flags them all holds
// about 14 MB per adapter, freed with it
// (`benches/shot_conditions.rs` measures it). A shot-end signal would
// free them as shots end: none exists in the protocol.
#[derive(Debug, Default)]
pub(crate) struct ConditionsSoFar(BTreeMap<ShotId, ShotConditions>);

impl ConditionsSoFar {
    /// Raises `conditions` on `shot`.
    pub(crate) fn raise(&mut self, shot: ShotId, conditions: ShotConditions) {
        if !conditions.is_empty() {
            *self.0.entry(shot).or_default() |= conditions;
        }
    }

    /// The conditions so far of those of `shots` that have some, by shot in
    /// ascending order: none, with no lookup, while no shot has any; once
    /// one has, an allocation and a lookup of each of `shots`, flagged or
    /// not.
    #[inline]
    pub(crate) fn of_shots(&self, shots: &[ShotId]) -> Marks {
        if self.0.is_empty() {
            return Marks::default();
        }
        self.of_flagged_shots(shots)
    }

    /// [`of_shots`](Self::of_shots), once some shot has a condition.
    #[inline(never)]
    fn of_flagged_shots(&self, shots: &[ShotId]) -> Marks {
        let mut of_shots = Vec::with_capacity(shots.len().min(self.0.len()));
        of_shots.extend(
            (shots.iter())
                .filter_map(|&shot| self.0.get(&shot).map(|&conditions| (shot, conditions))),
        );
        of_shots.sort_unstable_by_key(|&(shot, _)| shot);
        Marks(of_shots)
    }
}

/// The conditions of some shots, by shot in ascending order, to mark their
/// outcomes with: none, when none of them has any.
#[derive(Debug, Default)]
pub(crate) struct Marks(Vec<(ShotId, ShotConditions)>);

impl Marks {
    /// Marks each of `outcomes` with the conditions held for its shot, if
    /// any.
    #[inline]
    pub(crate) fn mark(&self, outcomes: &mut [ShotOutcome]) {
        if !self.0.is_empty() {
            self.mark_held(outcomes);
        }
    }

    /// [`mark`](Self::mark), once it holds some.
    #[inline(never)]
    fn mark_held(&self, outcomes: &mut [ShotOutcome]) {
        match self.0.as_slice() {
            [] => {}
            &[(shot, conditions)] => {
                for outcome in outcomes.iter_mut().filter(|outcome| outcome.shot == shot) {
                    outcome.conditions |= conditions;
                }
            }
            marks => {
                // A shot's outcomes come in runs: one search per run.
                let mut last = None;
                for outcome in outcomes {
                    let conditions = match last {
                        Some((shot, conditions)) if shot == outcome.shot => conditions,
                        _ => {
                            let found = marks
                                .binary_search_by_key(&outcome.shot, |&(shot, _)| shot)
                                .map_or(ShotConditions::NONE, |at| marks[at].1);
                            last = Some((outcome.shot, found));
                            found
                        }
                    };
                    outcome.conditions |= conditions;
                }
            }
        }
    }
}

/// The conditions raised on shots in one call, one entry per run of
/// outcomes of a shot, merged into its adapter's [`ConditionsSoFar`] as the
/// call ends. Empty, with nothing to merge, while none is raised.
#[derive(Debug, Default)]
pub(crate) struct Raised(Vec<(ShotId, ShotConditions)>);

impl Raised {
    /// Raises `conditions` on `shot`, into the last entry when it is
    /// `shot`'s, so that a shot's outcomes in a row take one entry.
    pub(crate) fn raise(&mut self, shot: ShotId, conditions: ShotConditions) {
        if conditions.is_empty() {
            return;
        }
        match self.0.last_mut() {
            Some((last, so_far)) if *last == shot => *so_far |= conditions,
            _ => self.0.push((shot, conditions)),
        }
    }

    /// Raises the conditions each of `outcomes` carries on its shot: one
    /// pass that only reads them, and nothing more when none carries any,
    /// as in a run that raises none.
    #[inline]
    pub(crate) fn raise_carried(&mut self, outcomes: &[ShotOutcome]) {
        if !carried(outcomes).is_empty() {
            self.raise_each(outcomes);
        }
    }

    /// [`raise_carried`](Self::raise_carried), once one carries some.
    #[inline(never)]
    fn raise_each(&mut self, outcomes: &[ShotOutcome]) {
        // A shot's outcomes come in runs: each run's conditions are merged
        // here, then raised once.
        let mut run: Option<(ShotId, ShotConditions)> = None;
        for outcome in outcomes {
            match &mut run {
                Some((shot, conditions)) if *shot == outcome.shot => {
                    *conditions |= outcome.conditions;
                }
                _ => {
                    if let Some((shot, conditions)) = run {
                        self.raise(shot, conditions);
                    }
                    run = Some((outcome.shot, outcome.conditions));
                }
            }
        }
        if let Some((shot, conditions)) = run {
            self.raise(shot, conditions);
        }
    }

    /// Raises the conditions each of `outcomes` whose shot is in `shots`,
    /// in increasing order, carries on its shot, and none of the others':
    /// for what a call below on `shots` read before it failed, which may
    /// hold another shot's outcome. Nothing more than a pass that only
    /// reads them when none carries any; else a binary search of each in
    /// `shots`.
    pub(crate) fn raise_carried_on(&mut self, outcomes: &[ShotOutcome], shots: &[ShotId]) {
        debug_assert!(shots.is_sorted(), "a binary search needs them sorted");
        if carried(outcomes).is_empty() {
            return;
        }
        for outcome in outcomes {
            if shots.binary_search(&outcome.shot).is_ok() {
                self.raise(outcome.shot, outcome.conditions);
            }
        }
    }

    /// Merges what it holds into `so_far`.
    #[inline]
    pub(crate) fn into_so_far(self, so_far: &mut ConditionsSoFar) {
        if !self.0.is_empty() {
            self.merge_into(so_far);
        }
    }

    /// [`into_so_far`](Self::into_so_far), once it holds some.
    #[inline(never)]
    fn merge_into(self, so_far: &mut ConditionsSoFar) {
        for (shot, conditions) in self.0 {
            so_far.raise(shot, conditions);
        }
    }
}

/// The union of the conditions `outcomes` carry: one pass that only reads
/// them.
#[inline]
fn carried(outcomes: &[ShotOutcome]) -> ShotConditions {
    (outcomes.iter()).fold(ShotConditions::NONE, |carried, outcome| {
        carried | outcome.conditions
    })
}
