// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How an [`adapter_fn`](fn@crate::adapter_fn) keeps a shot's conditions
//! ([`ShotConditions`]): the conditions raised on shots in a call
//! ([`Raised`]), each shot's conditions so far over its calls
//! ([`ConditionsSoFar`]), and those of a call's shots that mark the
//! outcomes it gives ([`Marks`]).

use std::collections::BTreeMap;

use crate::{ShotConditions, ShotId, ShotOutcome};

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
