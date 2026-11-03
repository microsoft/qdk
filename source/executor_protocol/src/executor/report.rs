// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How an executor is closed ([`Close`]), the report it returns
//! ([`ExecutorReport`], with [`LogP`]), and how reports fold ([`Fold`]).

use std::collections::BTreeMap;

#[cfg(doc)]
use crate::Capabilities;
use crate::ShotId;
#[cfg(doc)]
use crate::{Executor, Rejected};

/// A report that composes: an empty report, [`Default`], and a combination,
/// [`Fold::fold`].
///
/// Needed because each component of a chain keeps totals of its own (the
/// operations a simulator applied, the programs an adapter ran), and the
/// report of a chain must hold them all: an adapter's report holds its own
/// and the one of the executor it wraps, and the reports of parts of one run
/// (shots run separately) fold into the report of the whole.
///
/// So a fold must keep two rules, which the type system cannot check: the
/// default report is empty (folding it in changes nothing), and the order
/// and grouping of the reports folded do not change the total, since the
/// parts of a run may end in any order. Sums, maxima and unions of disjoint
/// parts keep them; a sum in floating point keeps the second up to
/// rounding only ([`LogP`]'s may differ in their last bits when regrouped).
/// The `conformance` module checks both (`default_is_empty`,
/// `folds_in_any_order`).
///
/// Integers fold by adding, and ordered maps value by value, so a report
/// that counts (per operation, per shot) is a map of integers and needs no
/// fold of its own. An integer's fold overflows as `+` does: it panics in
/// debug builds.
pub trait Fold: Default {
    /// Adds `other` into this report.
    fn fold(&mut self, other: Self);
}

impl Fold for () {
    fn fold(&mut self, (): Self) {}
}

// Integers fold by adding, since the reports that hold them count (the calls
// a component ran, the operations of each kind): the count of a whole is the
// sum of the counts of its parts. A report that keeps a maximum or a minimum
// (a simulator's peak bond dimension) folds its own way.
macro_rules! fold_by_adding {
    ($($integer:ty),*) => {
        $(
            impl Fold for $integer {
                fn fold(&mut self, other: Self) {
                    *self += other;
                }
            }
        )*
    };
}

fold_by_adding!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

/// Folds value by value: the values of a key in both maps fold, and a key in
/// one map keeps its value.
impl<K: Ord, V: Fold> Fold for BTreeMap<K, V> {
    fn fold(&mut self, other: Self) {
        for (key, value) in other {
            self.entry(key).or_default().fold(value);
        }
    }
}

/// Folds part by part: an adapter's report beside the one below it
/// ([`AdapterFn::state_report`](crate::AdapterFn::state_report)).
impl<A: Fold, B: Fold> Fold for (A, B) {
    fn fold(&mut self, (a, b): Self) {
        self.0.fold(a);
        self.1.fold(b);
    }
}

/// Ends an executor's work and returns its report. A driver never closes the
/// executor it calls: the run does, once, when the driver's future has ended
/// ([`Run`](crate::Run)).
///
/// The rules of `close` (later calls rejected, a second report empty) are
/// not checked by the type system. To keep them, an executor that passes
/// calls on to an executor downstream of it closes that executor, and keeps
/// them as that executor does; one that runs the calls itself closes its
/// [`ExecutorLifecycle`](crate::ExecutorLifecycle) (or keeps a flag that it
/// is closed), and takes its totals with [`std::mem::take`]; one made with
/// [`executor_fn`](fn@crate::executor_fn) keeps both. The `conformance`
/// module checks both rules
/// (`rejects_calls_once_closed`, `reports_once`).
pub trait Close {
    /// The executor's totals.
    type Report: Fold;

    /// Ends the executor's work: later calls are rejected with
    /// [`Rejected::Closed`], except by the executor an adapter written by
    /// hand gives, whose composed executor rejects them for it
    /// ([`Executor`]'s contract). A second `close` returns an empty report.
    ///
    /// A reply dropped before its end does not stop its call's work, and what
    /// `close` does with that work is the executor's own: one made with
    /// [`executor_fn`](fn@crate::executor_fn) and not placed ran it inside
    /// `call`, so none is left; a placed one
    /// ([`PlacedExecutor`](crate::PlacedExecutor), and
    /// [`OnePerShotPlacedExecutor`](crate::OnePerShotPlacedExecutor) through
    /// each shot's `PlacedExecutor`) waits for the calls queued, those whose
    /// replies were dropped included. Known limit, open, which no test pins:
    /// stopping that work goes with the planned cancellation.
    fn close(&mut self) -> Self::Report;
}

/// What an executor ran, returned when it closes, with its states' report,
/// of type `R` ([`states`](Self::states)).
///
/// Reports fold. When executors that split the shots run the same calls,
/// their folded `log_p`, and state reports that count per shot, are those of
/// one executor over all the shots; `calls` counts the calls each executor
/// accepted, so it adds up across the executors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExecutorReport<R> {
    /// The calls it accepted, those that failed later included, and those
    /// that panicked inside `call` once accepted (an `executor_fn`'s
    /// program), which returned no reply; refused ones are not counted, nor
    /// those that panicked before they were accepted (its check, or a state
    /// closure). A [`PlacedExecutor`](crate::PlacedExecutor)
    /// reports those of the executor it places: the calls that reached
    /// it, not those queued behind a failure, and none once its placement
    /// stopped (a call panicked, or a job was dropped unrun). Through
    /// adapters, the last executor's, passed up as it is.
    pub calls: u64,
    /// Per shot, the log probability of every draw made for it, where it is
    /// known.
    pub log_p: LogP,
    /// The reports of its states, folded: each state's, made as it closes
    /// (`ExecutorFn::state_report`).
    pub states: R,
}

impl<R: Default> ExecutorReport<R> {
    /// The report of an executor that accepted `calls` calls, before its
    /// states are read into it. `R::default` is the implementer's code, so
    /// it may panic.
    pub(crate) fn of_calls(calls: u64) -> Self {
        ExecutorReport {
            calls,
            ..ExecutorReport::default()
        }
    }
}

impl<R: Fold> Fold for ExecutorReport<R> {
    fn fold(&mut self, other: Self) {
        self.calls += other.calls;
        self.log_p.fold(other.log_p);
        self.states.fold(other.states);
    }
}

/// Per shot, the sum of ln p over the draws made for that shot, added as the
/// executor closes, so folding two is adding shot by shot.
///
/// Absent, not 0, for a shot where no component knows it
/// ([`Capabilities::log_probability`]).
///
/// It is per shot, and reported once, at close, because its consumers
/// weigh whole shots (importance sampling of logical error rates, a
/// statistical test of a sampler, symbolic path weights, postselection
/// rates), and because draws that make no event count too (noise, a bare
/// reset, draws after the last measurement). So events carry no
/// probability: a component that knows an outcome's probability adds it to
/// its shot's sum instead.
///
/// Two extensions are planned, neither built: a probability per event,
/// opted into as a need of its own, for a caller that weighs outcomes one
/// by one; and the running log probability of a shot in the middle of a
/// run, for a caller that stops a shot whose weight is too small.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LogP(BTreeMap<ShotId, f64>);

impl LogP {
    /// Adds `ln_p` to shot `shot`'s sum.
    pub fn add(&mut self, shot: ShotId, ln_p: f64) {
        *self.0.entry(shot).or_insert(0.0) += ln_p;
    }

    /// Shot `shot`'s sum, if any component reported one.
    #[must_use]
    pub fn get(&self, shot: ShotId) -> Option<f64> {
        self.0.get(&shot).copied()
    }

    /// Whether no component reported any.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Fold for LogP {
    fn fold(&mut self, other: Self) {
        for (shot, ln_p) in other.0 {
            self.add(shot, ln_p);
        }
    }
}
