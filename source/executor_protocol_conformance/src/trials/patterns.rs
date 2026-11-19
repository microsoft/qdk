// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`TrialPatterns`]: the run patterns of one trial of a check, which the
//! check asks for each choice it makes. In trial 0 each is the fixed
//! pattern the check's documentation states; in a drawn trial each is
//! drawn from the trial's seed and index, within what the protocol leaves
//! free, so that a part fitted to the fixed patterns shows.

use executor_protocol::{Instruction, ShotId, ShotSet};
use rustc_hash::{FxHashMap, FxHashSet};

use super::Trial;
use crate::nonconformance::{Findings, Nonconformance};
use crate::script::{Script, ScriptBatch};
use crate::shared_run::{Raised, raising_halves};
use crate::splitmix64::SplitMix64;

/// The run patterns of one trial: fixed in trial 0, drawn in the others.
pub(crate) struct TrialPatterns {
    trial: Trial,
    /// The trial's draws, or `None` in trial 0.
    draws: Option<SplitMix64>,
}

/// The small ids a drawn trial gives `count` shots are below this many per
/// shot, so that shots' ids lie close enough for a part keyed to their
/// order, their position or their density to show.
const IDS_PER_SHOT: u32 = 64;

/// The large ids a drawn trial gives shots are below this bound, or below
/// [`IDS_PER_SHOT`] per shot if higher, so that a part keeping its shots in
/// a table sized for small ids (a `u64` bit mask, an array of 4,096) shows.
const LARGE_IDS: u32 = 1 << 16;

/// The first id a `u16` cannot hold: a drawn trial gives ids on each side
/// of it, and pairs of ids that agree on their low 16 bits, so that a part
/// storing ids as `u16`, or converting them so, shows.
const U16_EDGE: u32 = 1 << u16::BITS;

/// How many ids a drawn trial gives at each edge: the ids just below
/// [`U16_EDGE`], those from it up, and those up to `u32::MAX`, the top of
/// the ids' range, so that a part computing an id's successor or a count
/// of ids in a `u32` (which overflows there) shows.
const NEAR_AN_EDGE: u32 = 64;

impl TrialPatterns {
    /// The patterns of `trial`.
    pub(crate) fn new(trial: Trial) -> Self {
        Self {
            trial,
            draws: (!trial.is_fixed()).then(|| SplitMix64::new(trial.draws_seed())),
        }
    }

    /// Whether the trial runs the fixed patterns: trial 0.
    pub(crate) fn is_fixed(&self) -> bool {
        self.draws.is_none()
    }

    /// What a check of this trial found, so far nothing.
    pub(crate) fn findings(&self) -> Findings {
        Findings::new(self.trial)
    }

    /// The result of a check that cannot judge the part in this trial, for
    /// `why`, before any run.
    pub(crate) fn cannot_judge(&self, why: impl Into<String>) -> Nonconformance {
        Nonconformance::CannotJudge {
            trial: self.trial,
            why: why.into(),
        }
    }

    /// `script`, in trial 0; in a drawn trial, the same script for each
    /// shot, sent otherwise: the shots given new ids; each batch's shots
    /// in a new order, cut into one batch or more; each `execute`'s
    /// batches in a new order; and some `execute`s split in two, at a
    /// batch. So each shot gets the same instructions, `execute` after
    /// `execute`, as in `script`, in other company.
    pub(crate) fn script(&mut self, script: &Script) -> Script {
        let Some(draws) = &mut self.draws else {
            return script.clone();
        };
        let ids = new_ids(draws, &script.shots());
        let mut reshaped = Script::new();
        for batches in script.executes() {
            let mut cut: Vec<ScriptBatch> = Vec::new();
            for (shots, instructions) in batches {
                let mut shots: Vec<ShotId> = shots.iter().map(&ids).collect();
                shuffle(draws, &mut shots);
                for part in cut_up(draws, &shots) {
                    cut.push((set(part), instructions.clone()));
                }
            }
            shuffle(draws, &mut cut);
            if cut.len() > 1 && below(draws, 3) == 0 {
                let at = 1 + below(draws, cut.len() - 1);
                let second = cut.split_off(at);
                reshaped = reshaped.execute(cut).execute(second);
            } else {
                reshaped = reshaped.execute(cut);
            }
        }
        reshaped
    }

    /// `shots`, in trial 0; in a drawn trial, as many shots with new ids,
    /// in a new order: [`new_ids`] draws the ids in an order drawn too, so
    /// the renamed shots need no shuffle of their own.
    pub(crate) fn shots(&mut self, shots: &ShotSet) -> ShotSet {
        let Some(draws) = &mut self.draws else {
            return shots.clone();
        };
        let given: Vec<ShotId> = shots.iter().collect();
        let ids = new_ids(draws, &given);
        let renamed: Vec<ShotId> = given.into_iter().map(ids).collect();
        set(&renamed)
    }

    /// `shots` in one batch, in trial 0; in a drawn trial, `shots` in a new
    /// order, cut into one batch or more, in a new order. A check draws one
    /// cut of a group of shots per trial and sends it in every run it
    /// compares, so that each shot sits in the same batch in each.
    pub(crate) fn cut(&mut self, shots: &ShotSet) -> Cut {
        let Some(draws) = &mut self.draws else {
            return Cut(vec![shots.clone()]);
        };
        let mut shots: Vec<ShotId> = shots.iter().collect();
        shuffle(draws, &mut shots);
        let mut parts: Vec<ShotSet> = cut_up(draws, &shots).into_iter().map(set).collect();
        shuffle(draws, &mut parts);
        Cut(parts)
    }

    /// A cut of `held`, and one of `all`, which is `held` and `new`: in
    /// trial 0, one batch each; in a drawn trial, `held`'s cut, and that cut
    /// followed by one of `new`. So each shot of `held` sits in the same
    /// batch in both cuts, and a check can send `held` alone in one run and
    /// all the shots in another.
    pub(crate) fn held_and_all_cuts(
        &mut self,
        held: &ShotSet,
        new: Option<&ShotSet>,
        all: &ShotSet,
    ) -> [Cut; 2] {
        let held_cut = self.cut(held);
        if self.is_fixed() {
            return [held_cut, Cut(vec![all.clone()])];
        }
        let mut parts = held_cut.0.clone();
        if let Some(new) = new {
            parts.extend(self.cut(new).0);
        }
        [held_cut, Cut(parts)]
    }

    /// `shots` in two groups, each in its order: in trial 0, those at even
    /// positions and those at odd ones, which lie between each other; in
    /// a drawn trial, each shot in a group drawn for it, both groups
    /// holding one shot or more, each in a new order. `shots` holds two
    /// shots or more.
    pub(crate) fn two_groups(&mut self, shots: &[ShotId]) -> [Vec<ShotId>; 2] {
        debug_assert!(shots.len() > 1, "two groups need two shots");
        let Some(draws) = &mut self.draws else {
            return [
                shots.iter().step_by(2).copied().collect(),
                shots.iter().skip(1).step_by(2).copied().collect(),
            ];
        };
        loop {
            let mut groups = [Vec::new(), Vec::new()];
            for &shot in shots {
                groups[below(draws, 2)].push(shot);
            }
            if groups.iter().all(|group| !group.is_empty()) {
                for group in &mut groups {
                    shuffle(draws, group);
                }
                return groups;
            }
        }
    }

    /// The positions, among `count` shots, that a check raises conditions
    /// on in each of its raising runs: [`raising_halves`] of the positions
    /// in trial 0; in a drawn trial, of the positions in a new order. So
    /// any two positions are apart in some run either way.
    pub(crate) fn raising_halves(&mut self, count: usize) -> Vec<Vec<usize>> {
        let halves = raising_halves(count);
        let Some(draws) = &mut self.draws else {
            return halves;
        };
        let mut order: Vec<usize> = (0..count).collect();
        shuffle(draws, &mut order);
        halves
            .into_iter()
            .map(|half| half.into_iter().map(|position| order[position]).collect())
            .collect()
    }

    /// What a raising run raises on a shot, `fixed` in trial 0; in a
    /// drawn trial, one kind or more drawn for each `execute`, call and
    /// shot ([`Raised::Drawn`]), with a key drawn for each call of this.
    pub(crate) fn raised(&mut self, fixed: Raised) -> Raised {
        match &mut self.draws {
            None => fixed,
            Some(draws) => Raised::Drawn {
                key: draws.next_u64(),
            },
        }
    }

    /// The `execute`, among `executes`, after which the kit's driver fails
    /// a run: the first in trial 0, any in a drawn trial.
    pub(crate) fn failing_execute(&mut self, executes: usize) -> usize {
        self.draws
            .as_mut()
            .map_or(0, |draws| below(draws, executes.max(1)))
    }
}

/// Shots cut into batches, in the order the batches are sent: one cut per
/// trial and group of shots, which a check sends with each list of
/// instructions ([`TrialPatterns::cut`]).
pub(crate) struct Cut(Vec<ShotSet>);

impl Cut {
    /// One batch of `instructions` per part of the cut, in its order.
    pub(crate) fn batches(&self, instructions: &[Instruction]) -> Vec<ScriptBatch> {
        self.0
            .iter()
            .map(|part| (part.clone(), instructions.to_vec()))
            .collect()
    }
}

/// A draw below `bound`, which is not 0.
fn below(draws: &mut SplitMix64, bound: usize) -> usize {
    let bound = u64::try_from(bound).expect("a bound fits in u64");
    usize::try_from(draws.next_u64() % bound).expect("a draw below a usize fits in one")
}

/// `items` in an order drawn uniformly (Fisher–Yates).
fn shuffle<T>(draws: &mut SplitMix64, items: &mut [T]) {
    for last in (1..items.len()).rev() {
        items.swap(last, below(draws, last + 1));
    }
}

/// `shots` cut into one part or more, in their order: each gap between two
/// neighbours cut with odds of one in three.
fn cut_up<'a>(draws: &mut SplitMix64, shots: &'a [ShotId]) -> Vec<&'a [ShotId]> {
    let mut parts = Vec::new();
    let mut start = 0;
    for gap in 1..shots.len() {
        if below(draws, 3) == 0 {
            parts.push(&shots[start..gap]);
            start = gap;
        }
    }
    parts.push(&shots[start..]);
    parts
}

/// A new id for each of `shots`, all distinct: with odds of one in two,
/// the ids from 0 up, in an order drawn for them, as most callers number
/// their shots, so that a part keyed to a small id shows; otherwise ids
/// spread, so that a part keyed to a shot's position, or to the ids being
/// dense, shows, each with odds of three in eight small, below
/// [`IDS_PER_SHOT`] per shot, of two in eight large, below [`LARGE_IDS`],
/// and of three in eight at an edge ([`edge_id`]), so that a part fitted
/// to small ids, or to ids a `u16` holds, shows too. The map from each
/// shot to its new id.
///
/// # Panics
///
/// If `shots` holds more than `u32::MAX / IDS_PER_SHOT` shots (67,108,863),
/// whose ids could not be spread so.
fn new_ids(draws: &mut SplitMix64, shots: &[ShotId]) -> impl Fn(ShotId) -> ShotId + use<> {
    let bound = u32::try_from(shots.len())
        .ok()
        .and_then(|count| count.max(1).checked_mul(IDS_PER_SHOT))
        .unwrap_or_else(|| {
            panic!(
                "the drawn trials give new ids to at most {} shots, not {}",
                u32::MAX / IDS_PER_SHOT,
                shots.len()
            )
        });
    let count = u32::try_from(shots.len()).expect("fewer shots than the bound");
    let mut taken = Vec::with_capacity(shots.len());
    if below(draws, 2) == 0 {
        taken.extend(0..count);
        shuffle(draws, &mut taken);
    } else {
        let large = usize::try_from(bound.max(LARGE_IDS)).expect("a u32 fits in usize");
        let small = usize::try_from(bound).expect("a u32 fits in usize");
        let mut used = FxHashSet::default();
        while taken.len() < shots.len() {
            let id = match below(draws, 8) {
                0..=2 => u32::try_from(below(draws, small)).expect("below a u32 bound"),
                3 | 4 => u32::try_from(below(draws, large)).expect("below a u32 bound"),
                _ => edge_id(draws, &taken),
            };
            if used.insert(id) {
                taken.push(id);
            }
        }
    }
    let map: FxHashMap<ShotId, ShotId> = shots
        .iter()
        .copied()
        .zip(taken.into_iter().map(ShotId::new))
        .collect();
    move |shot| {
        *map.get(&shot)
            .expect("the map holds each shot `new_ids` was given, and only those are renamed")
    }
}

/// An id at an edge of what ids a part may store: with odds of two in
/// three, one that agrees with an id of `taken`, drawn, on its low 16
/// bits, as `k` and `k + 2¹⁶` do (or `2¹⁶` and above if `taken` is empty);
/// otherwise, with odds of one in three each, one of the [`NEAR_AN_EDGE`]
/// ids just below [`U16_EDGE`], of those from it up, or of those up to
/// `u32::MAX`.
fn edge_id(draws: &mut SplitMix64, taken: &[u32]) -> u32 {
    let near = u32::try_from(below(draws, NEAR_AN_EDGE as usize)).expect("below a u32 bound");
    let low_bits = u32::from(u16::MAX);
    if below(draws, 3) < 2 {
        return match taken.get(below(draws, taken.len().max(1))) {
            Some(&id) if id <= low_bits => id + U16_EDGE,
            Some(&id) => id & low_bits,
            None => U16_EDGE + near,
        };
    }
    match below(draws, 3) {
        0 => U16_EDGE - 1 - near,
        1 => U16_EDGE + near,
        _ => u32::MAX - near,
    }
}

/// `shots`, as a set, in their order.
fn set(shots: &[ShotId]) -> ShotSet {
    ShotSet::ids(shots.iter().map(|shot| shot.get()))
}
