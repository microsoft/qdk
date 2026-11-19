// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How the kit tells an unwritten readout value from a written one: it
//! runs the part again and again with the same seed, prefilling each slot
//! with a value it draws before the part runs, and reports the slots that
//! equal their prefill in every run ([`ReadoutPrefills`]).
//!
//! A slot the part leaves unwritten equals its prefill in every run, so it
//! is never missed. A slot the part writes with a value drawn
//! independently of the prefill equals it with probability at most ½ in a
//! run: each prefill is `Zero` or `One`, drawn evenly, and a `Missing`
//! value never equals one. The second run prefills each slot with the
//! complement of the first run's prefill, so a slot written the same in
//! both runs, as a seeded part writes it, equals its prefill in at most
//! one of them; so a part that repeats its values for a seed is settled in
//! two runs. Further runs, each with fresh prefills, run only while some
//! slot has equalled its prefill in every run so far, up to
//! [`MAX_RUNS`] in all; so each slot written with values that change from
//! run to run (a part whose values move between runs, such as a GPU's sums
//! in another order) is reported with probability at most 2⁻²¹, and a
//! check over N such slots in each of T trials reports one with
//! probability at most N·T·2⁻²¹. Missed: a
//! part that reads its prefill before writing, and one that answers with
//! fresh readouts of the same shape, whose unwritten values read
//! `Missing`.

use rustc_hash::FxHashSet;

use executor_protocol::{BatchReadouts, ReadoutSlot, ReadoutValue};

use crate::nonconformance::Place;
use crate::shared_run::{RunLog, SharedRun};
use crate::splitmix64::mix;

/// The most runs of the check that a part writes every readout
/// (`writes_every_readout`): two that settle a part that repeats its
/// values, and twenty more.
pub(crate) const MAX_RUNS: u32 = 22;

/// The seed of the prefills, fixed so that a check gives the same result
/// each time.
const PREFILL_SEED: u64 = 0x5EED_C0FF_EE00_0F00;

/// The slots that equalled their prefill in every run so far, and how
/// many runs there were.
#[derive(Default)]
pub(crate) struct ReadoutPrefills {
    runs: u32,
    /// After the first run: the slots that equalled their prefill in every
    /// run.
    unchanged: FxHashSet<Place>,
}

impl ReadoutPrefills {
    /// The runs so far: the index of the next run.
    pub(crate) fn runs(&self) -> u32 {
        self.runs
    }

    /// Whether another run is needed: none ran yet, or some slot equalled
    /// its prefill in every run and fewer than [`MAX_RUNS`] ran.
    pub(crate) fn needs_another_run(&self) -> bool {
        self.runs == 0 || (!self.unchanged.is_empty() && self.runs < MAX_RUNS)
    }

    /// Takes a run's rows into account.
    pub(crate) fn record(&mut self, log: &RunLog) {
        let mut unchanged = FxHashSet::default();
        for row in &log.rows {
            for (slot, (value, prefill)) in row.values.iter().zip(&row.prefills).enumerate() {
                if value == prefill {
                    unchanged.insert(Place::Slot {
                        execute: row.execute,
                        batch: row.batch,
                        row: row.row,
                        shot: row.shot,
                        slot: ReadoutSlot::new(u32::try_from(slot).expect("a slot index fits u32")),
                    });
                }
            }
        }
        if self.runs == 0 {
            self.unchanged = unchanged;
        } else {
            self.unchanged.retain(|place| unchanged.contains(place));
        }
        self.runs += 1;
    }

    /// The slots that equalled their prefill in every run, which the part
    /// leaves unwritten unless the kit was unlucky as the module says, in
    /// the order of their places.
    pub(crate) fn unwritten(&self) -> Vec<Place> {
        let mut unwritten: Vec<Place> = self.unchanged.iter().cloned().collect();
        unwritten.sort_by_key(slot_key);
        unwritten
    }
}

/// The value the wrapper writes into a slot before the part runs, in run
/// `run` of a check: drawn from the run and the slot's place, the second
/// run's the complement of the first's.
pub(crate) fn prefill(
    run: u32,
    execute: usize,
    batch: usize,
    row: usize,
    slot: usize,
) -> ReadoutValue {
    let draw = |run: u32| {
        let key = [
            u64::from(run),
            execute as u64,
            batch as u64,
            row as u64,
            slot as u64,
        ]
        .iter()
        .fold(PREFILL_SEED, |key, part| mix(key ^ part));
        key & 1 == 1
    };
    match run {
        1 => (!draw(0)).into(),
        run => draw(run).into(),
    }
}

/// A slot's place, as a key that sorts slots by `execute`, batch, row and
/// slot.
fn slot_key(place: &Place) -> (usize, usize, usize, u32) {
    match place {
        Place::Slot {
            execute,
            batch,
            row,
            slot,
            ..
        } => (*execute, *batch, *row, slot.get()),
        // Unreached: `unchanged`, whose places these are, holds slots only.
        _ => (usize::MAX, 0, 0, 0),
    }
}

/// Writes the current prefill run's values into `readouts` before the part
/// runs, if the run is one: the values written, per batch and row.
pub(crate) fn prefilled(
    run: &SharedRun,
    execute: usize,
    readouts: &mut [BatchReadouts],
) -> Option<Vec<Vec<Vec<ReadoutValue>>>> {
    let prefill_run = run.setup().prefill_run?;
    let mut written = Vec::with_capacity(readouts.len());
    for (batch, readouts) in readouts.iter_mut().enumerate() {
        let mut rows = Vec::with_capacity(readouts.shots());
        for row in 0..readouts.shots() {
            let values = readouts.row_mut(row);
            for (slot, value) in values.iter_mut().enumerate() {
                *value = prefill(prefill_run, execute, batch, row, slot);
            }
            rows.push(values.to_vec());
        }
        written.push(rows);
    }
    Some(written)
}
