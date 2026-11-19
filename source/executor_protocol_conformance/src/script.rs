// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a check of an executor or an adapter runs: a [`Script`], the
//! `execute`s the kit's driver sends, in order, and the variants
//! of it the checks derive, such as one shot's batches alone.

use executor_protocol::{Instruction, InstructionSet, ShotId, ShotSet};
use rustc_hash::FxHashSet;

use crate::splitmix64::SplitMix64;

/// The `execute`s the kit's driver sends the part under test, in
/// order: each a list of batches, each batch its shots and its
/// instructions, written in the instruction set the part accepts. The kit
/// sends each batch checked, stating its readout slots as that set counts
/// them, except in run 1 of
/// [`states_readout_slots`](crate::adapter::states_readout_slots),
/// which sends them unchecked.
///
/// A run's shots are every shot the script names, in the order they first
/// appear, then one the script never names, which the kit's driver never
/// sends: an id above the largest named, drawn from the run's seed. So the
/// runner's end of the run's shots ends a shot the part never held, unless
/// the check withholds the run's ends to drop the part. A run holds the
/// named shots alone where there is no such id (the script names shot
/// `u32::MAX`) or the chain refuses a run of one more shot than the script
/// names (its `max_shots`, the fewest any part offers, is the script's
/// count), so that the shot never sent never keeps the check from judging
/// the part. The check then, in trial 0, writes to the standard error that
/// those runs did not exercise an end of a shot never held. A script
/// naming fewer shots than the chain's `max_shots`, none of them
/// `u32::MAX`, gives each run that shot. A check refuses, as
/// `CannotJudge`, before any run, a script that names no shot, puts a shot in
/// two batches of an `execute` (the protocol asks a caller not to), or
/// holds an instruction outside the set the part accepts.
#[derive(Clone, Debug, Default)]
pub struct Script {
    executes: Vec<Vec<ScriptBatch>>,
}

/// A batch of a script: its shots, and its instructions.
pub(crate) type ScriptBatch = (ShotSet, Vec<Instruction>);

impl Script {
    /// A script that sends nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// This script, then one `execute` of `batches`.
    #[must_use]
    pub fn execute(
        mut self,
        batches: impl IntoIterator<Item = (ShotSet, Vec<Instruction>)>,
    ) -> Self {
        self.executes.push(batches.into_iter().collect());
        self
    }

    /// The `execute`s, in order.
    pub(crate) fn executes(&self) -> &[Vec<ScriptBatch>] {
        &self.executes
    }

    /// The shots of a run of the script with `seed`: every shot the script
    /// names, in the order they first appear, then the shot it never sends
    /// ([`never_sent`]).
    pub(crate) fn run_shots(&self, seed: u64) -> ShotSet {
        let named = self.shots();
        let unsent = never_sent(&named, seed);
        ShotSet::ids(named.iter().chain(&unsent).map(|shot| shot.get()))
    }

    /// The shots of a run of the script without the shot it never sends:
    /// every shot it names, in the order they first appear.
    pub(crate) fn named_shots(&self) -> ShotSet {
        ShotSet::ids(self.shots().iter().map(|shot| shot.get()))
    }

    /// Every shot the script names, in the order they first appear.
    pub(crate) fn shots(&self) -> Vec<ShotId> {
        let mut shots = Vec::new();
        let mut seen = FxHashSet::default();
        for (batch_shots, _) in self.executes.iter().flatten() {
            for shot in batch_shots.iter() {
                if seen.insert(shot) {
                    shots.push(shot);
                }
            }
        }
        shots
    }

    /// Why the kit cannot send this script to a part that accepts `set`, if
    /// it cannot: it names no shot, puts a shot in two batches of an
    /// `execute`, or holds an instruction outside `set`.
    pub(crate) fn cannot_judge_with(&self, set: &InstructionSet) -> Option<String> {
        if self.shots().is_empty() {
            return Some("the script names no shot".to_owned());
        }
        for (execute, batches) in self.executes.iter().enumerate() {
            let mut seen = FxHashSet::default();
            for (batch, (shots, instructions)) in batches.iter().enumerate() {
                if let Some(shot) = shots.iter().find(|shot| seen.contains(shot)) {
                    return Some(format!(
                        "execute {execute} puts shot {shot} in two batches; a caller puts a \
                         shot in at most one"
                    ));
                }
                seen.extend(shots.iter());
                if set.readout_slots_of(instructions).is_none() {
                    return Some(format!(
                        "execute {execute}, batch {batch} holds an instruction outside the set \
                         the part accepts"
                    ));
                }
            }
        }
        None
    }

    /// Only `shot`'s batches, each with `shot` alone, and only the
    /// `execute`s that hold one.
    pub(crate) fn alone(&self, shot: ShotId) -> Self {
        self.restricted(&[shot])
    }

    /// Each `execute` as two, the first on the shots of `groups[0]`, the
    /// second on those of `groups[1]`, each batch's shots in the group's
    /// order; an `execute` or batch left with no shot is dropped.
    pub(crate) fn split(&self, groups: &[Vec<ShotId>; 2]) -> Self {
        let executes = self
            .executes
            .iter()
            .flat_map(|batches| {
                groups.iter().filter_map(move |group| {
                    let restricted = restrict(batches, group);
                    (!restricted.is_empty()).then_some(restricted)
                })
            })
            .collect();
        Self { executes }
    }

    /// The `execute`s, each batch on the shots of `shots` it holds, in the
    /// order of `shots`; an `execute` or batch left with no shot is
    /// dropped.
    pub(crate) fn restricted(&self, shots: &[ShotId]) -> Self {
        self.split(&[shots.to_vec(), Vec::new()])
    }

    /// Each `execute` sent twice in a row.
    pub(crate) fn doubled(&self) -> Self {
        let executes = self
            .executes
            .iter()
            .flat_map(|batches| [batches.clone(), batches.clone()])
            .collect();
        Self { executes }
    }
}

/// The shot a run holds beside those the script names ([`Script`] says
/// why): an id above the largest of `named`, by 1 to [`ABOVE`] and at
/// most `u32::MAX`, drawn from `seed`. None when `named` is empty or its
/// largest id is `u32::MAX`.
fn never_sent(named: &[ShotId], seed: u64) -> Option<ShotId> {
    let largest = named.iter().map(|shot| shot.get()).max()?;
    // Below `u32::MAX - ABOVE`, the room is `ABOVE` and the draw is as if
    // there were no bound.
    let room = (u32::MAX - largest).min(ABOVE);
    if room == 0 {
        return None;
    }
    let step = u32::try_from(SplitMix64::new(seed).next_u64() % u64::from(room))
        .expect("below a u32 bound");
    Some(ShotId::new(largest + 1 + step))
}

/// How far above the largest id of a script the shot a run never sends
/// lies, at most.
const ABOVE: u32 = 64;

/// `batches`, each on the shots of `group` it holds, in `group`'s order; a
/// batch left with no shot is dropped.
fn restrict(batches: &[ScriptBatch], group: &[ShotId]) -> Vec<ScriptBatch> {
    batches
        .iter()
        .filter_map(|(shots, instructions)| {
            let kept: Vec<u32> = group
                .iter()
                .filter(|shot| shots.iter().any(|held| held == **shot))
                .map(|shot| shot.get())
                .collect();
            (!kept.is_empty()).then(|| (ShotSet::ids(kept), instructions.clone()))
        })
        .collect()
}
