// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The census: the shots' traces replayed breadth-first, batch by batch,
//! as a lockstep resumer would advance them.
//!
//! - **Round:** a batch index; every live shot runs its batch of that index.
//! - **Group:** the shots with one history (every earlier batch's calls and
//!   draws equal), which the grouped resumer runs on one state.
//! - **Unit:** the shots that share one state at a given split of the
//!   round: one group, its exact batch, and the values of its draws so
//!   far in the batch (a draw splits a group).
//! - **Cohort:** the units of a round whose batches are equal, exactly
//!   (sites, kinds and matrices) or by shape (sites and kinds), or every
//!   unit of the round (any site: a kernel that takes one block per
//!   member, wherever it stands). A cohort runs each of its splits as one
//!   launch of K updates, K its units then: the j-th launch takes each
//!   member's j-th split.
//! - **η:** at one launch, Σ b³ / (K · `b_max`³), b a unit's larger side of
//!   the split block.

use rustc_hash::FxHashMap;
use std::fmt::Write as _;

use crate::trace::{BatchTrace, ShotTrace};

/// Which batches make one cohort.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CohortKey {
    Exact,
    Shape,
    AnySite,
}

impl CohortKey {
    pub const ALL: [Self; 3] = [Self::Exact, Self::Shape, Self::AnySite];

    pub fn name(self) -> &'static str {
        match self {
            Self::Exact => "EXACT",
            Self::Shape => "SHAPE",
            Self::AnySite => "ANY_SITE",
        }
    }

    fn of(self, batch: &BatchTrace) -> u64 {
        match self {
            Self::Exact => batch.exact,
            Self::Shape => batch.shape,
            Self::AnySite => 0,
        }
    }
}

/// What the shots' splits and draws cost, run per cohort.
#[derive(Default, Clone)]
pub struct CohortCost {
    /// One per split of each cohort: the launches.
    pub launches: u64,
    /// Σ ⌈K / wave⌉ over the launches: the waves of `wave` updates.
    pub waves: u64,
    /// Σ K over the launches: the updates of distinct states.
    pub updates: u64,
    /// Σ shots over the launches.
    pub shot_updates: u64,
    /// Σ b³ and Σ K · `b_max`³ over the launches.
    pub cubes: f64,
    pub padded_cubes: f64,
    /// Σ b³ by tenth of η (the last bin holds η = 1).
    pub eta_bins: [f64; 11],
    /// One per draw of each cohort: the draws' launches (one sync each).
    pub draw_launches: u64,
    /// The launches where a cohort's members split different cuts, and the
    /// rounds where their split or draw counts differ.
    pub cut_mismatches: u64,
    pub count_mismatches: u64,
    /// The largest K of a launch.
    pub largest_k: usize,
}

/// What one round holds.
pub struct Round {
    pub live: usize,
    pub groups: usize,
    pub shot_splits: u64,
    /// By [`CohortKey::ALL`]: the cohorts, the largest one's shots, and
    /// their cost.
    pub cohorts: [usize; 3],
    pub largest: [usize; 3],
    pub cost: [CohortCost; 3],
}

/// The census of a set of shots.
pub struct Census {
    pub shots: usize,
    pub rounds: Vec<Round>,
    /// Σ splits and Σ draws over the shots, each shot alone.
    pub shot_splits: u64,
    pub shot_draws: u64,
    /// Σ distinct states over the splits and the draws: the grouped
    /// resumer's updates and draws.
    pub group_splits: u64,
    pub group_draws: u64,
    /// By [`CohortKey::ALL`].
    pub cost: [CohortCost; 3],
}

const START: u64 = 0xcbf2_9ce4_8422_2325;

fn mix(hash: u64, value: u64) -> u64 {
    let mut hash = hash;
    for byte in value.to_le_bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// The census of `traces`, launches counted in waves of at most `wave`.
pub fn census(traces: &[ShotTrace], wave: usize) -> Census {
    let rounds = traces.iter().map(|t| t.batches.len()).max().unwrap_or(0);
    let mut history = vec![START; traces.len()];
    let mut census = Census {
        shots: traces.len(),
        rounds: Vec::with_capacity(rounds),
        shot_splits: 0,
        shot_draws: 0,
        group_splits: 0,
        group_draws: 0,
        cost: Default::default(),
    };
    for round in 0..rounds {
        let live: Vec<usize> = (0..traces.len())
            .filter(|&s| round < traces[s].batches.len())
            .collect();
        let batch = |s: usize| &traces[s].batches[round];
        let groups = distinct(live.iter().map(|&s| history[s]));
        let mut row = Round {
            live: live.len(),
            groups,
            shot_splits: live.iter().map(|&s| batch(s).splits.len() as u64).sum(),
            cohorts: [0; 3],
            largest: [0; 3],
            cost: Default::default(),
        };
        for (k, key) in CohortKey::ALL.into_iter().enumerate() {
            let mut cohorts: FxHashMap<u64, Vec<usize>> = FxHashMap::default();
            for &s in &live {
                cohorts.entry(key.of(batch(s))).or_default().push(s);
            }
            row.largest[k] = cohorts.values().map(Vec::len).max().unwrap_or(0);
            row.cohorts[k] = cohorts.len();
            for members in cohorts.values() {
                run_cohort(traces, round, &history, members, wave, &mut row.cost[k]);
            }
            add(&mut census.cost[k], &row.cost[k]);
        }
        census.shot_splits += row.shot_splits;
        census.shot_draws += live
            .iter()
            .map(|&s| batch(s).draws.len() as u64)
            .sum::<u64>();
        census.group_splits += row.cost[0].updates;
        census.group_draws += group_draws(traces, round, &history, &live);
        for &s in &live {
            let batch = batch(s);
            let mut next = mix(history[s], batch.exact);
            for draw in &batch.draws {
                next = mix(next, u64::from(draw.one));
            }
            history[s] = next;
        }
        census.rounds.push(row);
    }
    census
}

/// The state a shot is in at a point of the round: its group, its exact
/// batch, and its draws' values so far.
fn unit_start(history: u64, batch: &BatchTrace) -> u64 {
    mix(history, batch.exact)
}

/// Runs `members`' batch of `round` as one cohort, adding its cost.
fn run_cohort(
    traces: &[ShotTrace],
    round: usize,
    history: &[u64],
    members: &[usize],
    wave: usize,
    cost: &mut CohortCost,
) {
    let batches: Vec<&BatchTrace> = members.iter().map(|&s| &traces[s].batches[round]).collect();
    let splits = batches.iter().map(|b| b.splits.len()).max().unwrap_or(0);
    let draws = batches.iter().map(|b| b.draws.len()).max().unwrap_or(0);
    if batches
        .iter()
        .any(|b| b.splits.len() != splits || b.draws.len() != draws)
    {
        cost.count_mismatches += 1;
    }
    cost.draw_launches += draws as u64;
    let mut unit: Vec<u64> = members
        .iter()
        .zip(&batches)
        .map(|(&s, b)| unit_start(history[s], b))
        .collect();
    let mut next_draw = vec![0usize; members.len()];
    let mut states: Vec<(u64, u16)> = Vec::with_capacity(members.len());
    for j in 0..splits {
        states.clear();
        let mut shots = 0u64;
        let mut cut = None;
        let mut mismatch = false;
        for (m, batch) in batches.iter().enumerate() {
            let Some(split) = batch.splits.get(j) else {
                continue;
            };
            while let Some(draw) = batch.draws.get(next_draw[m])
                && draw.after_splits as usize <= j
            {
                unit[m] = mix(unit[m], u64::from(draw.one) + 2);
                next_draw[m] += 1;
            }
            shots += 1;
            mismatch |= *cut.get_or_insert(split.cut) != split.cut;
            states.push((unit[m], split.left.max(split.right)));
        }
        states.sort_unstable();
        states.dedup_by_key(|state| state.0);
        let k = states.len();
        let b_max = states.iter().map(|s| f64::from(s.1)).fold(0.0, f64::max);
        let cubes: f64 = states.iter().map(|s| f64::from(s.1).powi(3)).sum();
        let padded = k as f64 * b_max.powi(3);
        let eta = cubes / padded;
        cost.launches += 1;
        cost.waves += k.div_ceil(wave) as u64;
        cost.updates += k as u64;
        cost.shot_updates += shots;
        cost.cubes += cubes;
        cost.padded_cubes += padded;
        cost.eta_bins[((eta * 10.0).floor() as usize).min(10)] += cubes;
        cost.cut_mismatches += u64::from(mismatch);
        cost.largest_k = cost.largest_k.max(k);
    }
}

/// The draws of the round's distinct states: each group's draws, a group
/// split by a draw drawing apart from then on.
fn group_draws(traces: &[ShotTrace], round: usize, history: &[u64], live: &[usize]) -> u64 {
    let mut states: Vec<(usize, u64)> = Vec::new();
    for &s in live {
        let batch = &traces[s].batches[round];
        let mut unit = unit_start(history[s], batch);
        for (d, draw) in batch.draws.iter().enumerate() {
            states.push((d, unit));
            unit = mix(unit, u64::from(draw.one) + 2);
        }
    }
    distinct(states.into_iter().map(|(d, unit)| mix(unit, d as u64))) as u64
}

fn distinct(values: impl Iterator<Item = u64>) -> usize {
    let mut values: Vec<u64> = values.collect();
    values.sort_unstable();
    values.dedup();
    values.len()
}

fn add(total: &mut CohortCost, part: &CohortCost) {
    total.launches += part.launches;
    total.waves += part.waves;
    total.updates += part.updates;
    total.shot_updates += part.shot_updates;
    total.cubes += part.cubes;
    total.padded_cubes += part.padded_cubes;
    for (bin, add) in total.eta_bins.iter_mut().zip(part.eta_bins) {
        *bin += add;
    }
    total.draw_launches += part.draw_launches;
    total.cut_mismatches += part.cut_mismatches;
    total.count_mismatches += part.count_mismatches;
    total.largest_k = total.largest_k.max(part.largest_k);
}

impl Census {
    /// The per-round table, as CSV.
    pub fn rounds_csv(&self) -> String {
        let mut csv = String::from(
            "round,live,groups,exact_cohorts,shape_cohorts,largest_exact,largest_shape,\
             shot_splits,group_splits,exact_launches,shape_launches,exact_k,shape_k,\
             shape_eta,draw_launches_shape,any_site_launches,any_site_k,any_site_eta\n",
        );
        for (index, r) in self.rounds.iter().enumerate() {
            let ratio = |a: u64, b: u64| if b == 0 { 0.0 } else { a as f64 / b as f64 };
            let eta = |c: &CohortCost| {
                if c.padded_cubes == 0.0 {
                    1.0
                } else {
                    c.cubes / c.padded_cubes
                }
            };
            let [exact, shape, any] = &r.cost;
            writeln!(
                csv,
                "{index},{},{},{},{},{},{},{},{},{},{},{:.2},{:.2},{:.3},{},{},{:.2},{:.3}",
                r.live,
                r.groups,
                r.cohorts[0],
                r.cohorts[1],
                r.largest[0],
                r.largest[1],
                r.shot_splits,
                exact.updates,
                exact.launches,
                shape.launches,
                ratio(exact.updates, exact.launches),
                ratio(shape.updates, shape.launches),
                eta(shape),
                shape.draw_launches,
                any.launches,
                ratio(any.updates, any.launches),
                eta(any),
            )
            .expect("a String takes any text");
        }
        csv
    }
}
