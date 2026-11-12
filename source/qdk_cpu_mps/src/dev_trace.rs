// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only (not for review): per-thread counters of
//! the splits, to see where the capacity binds. They count every split of
//! every state on the calling thread since the last [`reset`]; they change
//! no value the engine computes.

use std::cell::Cell;

/// What the splits on this thread did since the last [`reset`].
#[derive(Clone, Copy, Debug, Default)]
pub struct SplitTrace {
    /// Every split.
    pub splits: u64,
    /// The splits where the capacity kept fewer values than the cutoff
    /// would: the capacity bound.
    pub capacity_bound: u64,
    /// The sum, over the capacity-bound splits, of the fraction of the
    /// block's squared norm the capacity dropped (beyond the cutoff's).
    pub capacity_lost: f64,
    /// The largest such fraction at one split.
    pub capacity_lost_max: f64,
    /// The largest bond the cutoff alone would keep, over all splits.
    pub largest_need: usize,
    /// The largest bond the cutoff alone would keep at a capacity-bound
    /// split, and the capacity's bond there.
    pub capacity_bound_need: usize,
    pub capacity_bound_cap: usize,
    /// The cut of the first capacity-bound split, and the split's index.
    pub first_capacity_bound_cut: Option<usize>,
    pub first_capacity_bound_split: Option<u64>,
}

thread_local! {
    static TRACE: Cell<SplitTrace> = Cell::new(SplitTrace::default());
}

/// The counters on this thread.
#[must_use]
pub fn get() -> SplitTrace {
    TRACE.with(Cell::get)
}

/// Zeroes the counters on this thread.
pub fn reset() {
    TRACE.with(|trace| trace.set(SplitTrace::default()));
}

/// Records a split of `values` (nonincreasing) that kept `bond` of them,
/// where the cutoff alone would keep `need`, under the capacity `cap`.
pub(crate) fn record(values: &[f64], need: usize, bond: usize, cap: usize, cut: usize) {
    TRACE.with(|cell| {
        let mut trace = cell.get();
        trace.splits += 1;
        trace.largest_need = trace.largest_need.max(need);
        if bond < need {
            let largest = values[0];
            let square = |s: &f64| (s / largest) * (s / largest);
            let total: f64 = values.iter().map(square).sum();
            let dropped: f64 = values[bond..need].iter().map(square).sum();
            let fraction = dropped / total;
            trace.capacity_bound += 1;
            trace.capacity_lost += fraction;
            trace.capacity_lost_max = trace.capacity_lost_max.max(fraction);
            if need > trace.capacity_bound_need {
                trace.capacity_bound_need = need;
                trace.capacity_bound_cap = cap;
            }
            if trace.first_capacity_bound_cut.is_none() {
                trace.first_capacity_bound_cut = Some(cut);
                trace.first_capacity_bound_split = Some(trace.splits);
            }
        }
        cell.set(trace);
    });
}
