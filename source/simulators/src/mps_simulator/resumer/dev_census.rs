// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only (not for review): a census of what a
//! grouped run did, one record per segment, and the reset handling a run
//! may try instead of splitting at a reset.
//!
//! A segment is what one group ran of one batch: from the batch's first
//! step, or from the step after the draw that split the group off, to the
//! batch's end. The census is kept on the thread that runs, as the
//! routing's trace is, and only while it is switched on
//! ([`start_census`]).

use std::cell::RefCell;

use super::lowering::{DevWideOperator, Operator, Step};

/// EXPERIMENTAL, development only: what a group did with a reset whose
/// shots drew both values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DevResetHandling {
    /// Splits the group, as every draw does (the resumer's behaviour).
    #[default]
    Split,
    /// Keeps every shot in the group, on the more probable value, when
    /// every site is in a known basis state once the run of resets that
    /// holds this one ends: then each value leaves the same product state,
    /// and a reset answers no result, so the shots' records keep their
    /// distribution. Otherwise splits.
    Merge,
    /// As `Merge`, but at the first reset of such a run of resets, replaces
    /// the state with a fresh product state of the known values, and skips
    /// the run's resets (each shot still takes one uniform per reset, so its
    /// stream stays aligned with `Split`'s).
    Fresh,
}

/// EXPERIMENTAL, development only: one segment of a grouped run.
#[derive(Clone, Debug, Default)]
pub struct SegmentCensus {
    /// The results the group had drawn before the segment.
    pub results_before: usize,
    /// The batches the group's history had resumed before this one.
    pub batch: usize,
    /// The step of the batch it starts at: 0 at a batch's start, more for a
    /// part split off.
    pub start: usize,
    /// The batch's steps.
    pub batch_steps: usize,
    /// The group's shots at the segment's start and end.
    pub shots: usize,
    pub shots_at_end: usize,
    /// Whether the group's state was prepared at the segment's start.
    pub had_state: bool,
    pub applies: u64,
    pub two_site: u64,
    pub measures: u64,
    pub resets: u64,
    /// Draws whose shots drew both values, by kind, and what came of them.
    pub measure_splits: u64,
    pub reset_splits: u64,
    pub reset_merges: u64,
    /// Runs of resets replaced by a fresh state, and the resets they
    /// skipped.
    pub fresh_windows: u64,
    pub fresh_skipped: u64,
    /// Time in `apply`; in draws (P(One), projection, the X of a reset, a
    /// fork); in a fresh state's making; in the bond reads; and in resuming
    /// the machine, lowering and rewriting the next batch.
    pub apply_ns: u64,
    pub draw_ns: u64,
    pub fork_ns: u64,
    pub fresh_ns: u64,
    pub bond_ns: u64,
    pub host_ns: u64,
    /// Reads that give the host a number from the state: P(One), a
    /// projection's probability, a shape, a retained weight.
    pub probability_reads: u64,
    pub project_reads: u64,
    pub shape_reads: u64,
    pub weight_reads: u64,
    /// The engine's calls that change a state, and its forks.
    pub forks: u64,
    /// Hashes of the segment's operators, all and two-site only, in order.
    pub hash_all: u64,
    pub hash_two_site: u64,
    /// Whether the group's context ended with this segment, and its
    /// retained weight then.
    pub ended: bool,
    pub retained_weight: Option<f64>,
}

thread_local! {
    static CENSUS: RefCell<Option<Vec<SegmentCensus>>> = const { RefCell::new(None) };
}

/// EXPERIMENTAL, development only: switches the census on for this thread,
/// dropping what it held.
pub fn start_census() {
    CENSUS.with(|census| *census.borrow_mut() = Some(Vec::new()));
}

/// EXPERIMENTAL, development only: the segments recorded on this thread
/// since [`start_census`], switching the census off.
#[must_use]
pub fn take_census() -> Vec<SegmentCensus> {
    CENSUS.with(|census| census.borrow_mut().take().unwrap_or_default())
}

pub(super) fn census_on() -> bool {
    CENSUS.with(|census| census.borrow().is_some())
}

pub(super) fn record(segment: SegmentCensus) {
    CENSUS.with(|census| {
        if let Some(segments) = census.borrow_mut().as_mut() {
            segments.push(segment);
        }
    });
}

/// Mixes `operator` into `hash` (FNV-1a over its sites and matrix bits).
pub(super) fn mix(hash: u64, operator: &Operator) -> u64 {
    const PRIME: u64 = 0x0100_0000_01b3;
    let mut hash = hash;
    let mut eat = |word: u64| {
        hash ^= word;
        hash = hash.wrapping_mul(PRIME);
    };
    for &site in operator.sites() {
        eat(site as u64);
    }
    for value in operator.matrix.row_major() {
        eat(value.re.to_bits());
        eat(value.im.to_bits());
    }
    hash
}

/// Mixes a dev wide operator into `hash`, as [`mix`] does an operator.
pub(super) fn mix_wide(hash: u64, wide: &DevWideOperator) -> u64 {
    const PRIME: u64 = 0x0100_0000_01b3;
    let mut hash = hash;
    let mut eat = |word: u64| {
        hash ^= word;
        hash = hash.wrapping_mul(PRIME);
    };
    for &site in &wide.sites {
        eat(site as u64);
    }
    for value in &wide.matrix {
        eat(value.re.to_bits());
        eat(value.im.to_bits());
    }
    hash
}

pub(super) const HASH_START: u64 = 0xcbf2_9ce4_8422_2325;

/// Whether every site is in a known basis state once the run of resets
/// that holds the reset of `site` just taken, and goes on at
/// `steps[next]`, ends, given each site's known value (`None` when
/// unknown) before it; the resets set theirs to Zero.
pub(super) fn run_of_resets_settles(
    known: &[Option<bool>],
    site: usize,
    steps: &[Step],
    next: usize,
) -> bool {
    let mut known = known.to_vec();
    known[site] = Some(false);
    for step in &steps[next.min(steps.len())..] {
        match *step {
            Step::Reset { site } => known[site] = Some(false),
            _ => break,
        }
    }
    known.iter().all(Option::is_some)
}
