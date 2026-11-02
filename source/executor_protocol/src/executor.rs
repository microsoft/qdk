// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Executors, and how they are closed: the report each returns, and how
//! reports compose.

use std::collections::BTreeMap;

#[cfg(doc)]
use crate::{Capabilities, Describe, InstructionBatch, ShotContext};
use crate::{Message, Rejected, Reply, ShotId};

/// An executor of messages of kind `M`: one that runs them, or the executor an
/// adapter gives upstream of another. The crate's
/// `docs/internals/composition.md` lists the roles.
///
/// Its reply carries `M`'s events, so an executor cannot reply with another
/// message kind's events. The reply's type is the executor's own, not a boxed
/// one, so chaining executors adds no boxing or dynamic dispatch per component;
/// what a reply allocates is its own choice
/// ([`IncrementalReply`](crate::IncrementalReply) shares a buffer with its
/// producer, [`ImmediateReply`](crate::ImmediateReply) holds its events).
///
/// # Contract
///
/// Every executor a caller reaches, the last one of a chain or one an adapter
/// gives, keeps these rules, which the type system cannot check and
/// composition relies on:
///
/// - One reply per call: [`Executor::call`] returns the reply of this
///   message, or refuses it.
/// - A refusal means nothing ran: no state of the executor, nor of any
///   shot it runs, moved, so the call can be corrected and sent again
///   (the `conformance` module checks that the executor then takes a call,
///   `accepts_calls_after_a_refusal`). The exception is a refusal of class
///   [`RetryClass::Resend`](crate::RetryClass::Resend), such as
///   [`Rejected::NoReply`]: no verdict arrived over a link, so work may
///   have run ([`Executor::call`]).
/// - A failure once work has started fails the reply, not the call: the reply
///   ends with [`ReplyStatus::Failed`](crate::ReplyStatus::Failed). An executor
///   whose state carries over from one call to the next (a shot's state, or the
///   states of its shots) then refuses its later calls
///   ([`Rejected::EarlierFailure`]), since that state is no longer the one its
///   caller expects. A later call in flight as the failure happens ends with it
///   too once the failure is recorded as the executor's (on an
///   [`ExecutorLifecycle`](crate::ExecutorLifecycle): with `fail` when it
///   failed inside `call`, or by a reply bound with `bind` as that reply is
///   polled): refused, or failed with no more events, since it would run on
///   that state; a poll that runs as the failure is recorded may still leave
///   the reply pending and hand over that poll's own events, and the next poll
///   ends it (a known issue, noted at
///   [`LifecycleBoundReply`](crate::LifecycleBoundReply)'s source). Refusing a
///   new call ends no call already in flight: placed per shot, the executor
///   refuses a call once a shot's state failed, yet a call in flight on other
///   shots may still end done. Until the failure is recorded, such a call may
///   end as its own work does: an executor of several shots may have handed
///   over the events of shots that ran before, and a call on other shots may
///   end done (a design question, open, recorded at
///   [`ShotReplies`](crate::ShotReplies)'s source). The `conformance` check
///   `ends_calls_in_flight_at_an_earlier_failure` polls the failing call's
///   reply first.
/// - Once closed ([`Close`]), it refuses every call.
/// - A wrapper never forgets a refusal. A wrapper is an executor that
///   wraps another, or is made of one, and continues it, as `.shots(..)`
///   and `.run_on(..)` put one on an executor that
///   [`executor_fn`](fn@crate::executor_fn) makes (given a state per shot,
///   placed on a thread); it never forgets that the executor it continues
///   is closed or failed: it keeps refusing its calls, since it continues
///   what it wraps, failures included, as iterator adapters and Tower's
///   `Buffer` do. Known issue, open (blocks neither this commit nor a later
///   one): the `conformance` module has no check of this rule, which would
///   need a function that wraps the executor under test; the crate's own
///   wrappers are tested.
///
/// How an executor keeps the failure and close rules, which the
/// `conformance` module checks (`refuses_calls_after_a_failure` and
/// `ends_calls_in_flight_at_an_earlier_failure` on an executor whose state
/// carries over, and `rejects_calls_once_closed`):
///
/// - an executor with state of its own can keep them on an
///   [`ExecutorLifecycle`](crate::ExecutorLifecycle), or be built by
///   [`executor_fn`](fn@crate::executor_fn), which keeps them;
/// - one that passes its calls on (an adapter's executor) keeps them as the
///   executor downstream of it does;
/// - one with no state between calls has no state a failure leaves
///   part-way, so it needs only a flag that it is closed.
///
/// The executor an adapter written by hand gives ([`Apply`](crate::Apply))
/// is reached only through the executor composed with it, which keeps the
/// refusals after a failure, a panic and close for it, so it need not: its
/// own `call` only runs or refuses its batch.
///
/// # The last executor of a chain
///
/// The executor at the end of a chain (a simulator, a machine) is called
/// with a batch on a set of shots ([`InstructionBatch`]), describes what it
/// offers ([`Describe`]) and is closed once, with a report ([`Close`]):
/// [`compose`](fn@crate::compose) requires the three of any executor on its
/// right, this one or adapters composed with it. A driver calls the chain,
/// and its run closes it.
///
/// It holds its shots' states its own way, and states what that allows in
/// its capabilities, which [`compose`](fn@crate::compose) checks against a
/// driver's needs before anything runs: how many states it holds at once
/// ([`Capabilities::states`]), whether a shot continues from one call to the
/// next (`feedback`), whether a shot replays from the seed (`replayable`).
/// So a new way of holding shots is a new executor, and its callers do not
/// change:
///
/// | Executor                       | How it holds the shots                                                   | Replayable          |
/// | ------------------------------ | ------------------------------------------------------------------------ | ------------------- |
/// | a simulator, a device          | each in its own state, made at the shot's first call                     | as it offers        |
/// | an MPS ensemble (other crates) | together, from one state and one seed; a shot in one call                | no                  |
/// | grouped (planned)              | on shared states, split where their values differ                        | yes                 |
/// | sharded (planned)              | the shots split over several executors, their replies merged             | as the inner ones   |
///
/// Grouped and sharded executors are planned, with no milestone yet: a
/// grouped one for many shots of one program on a state vector, whose
/// common work then runs once; a sharded one for a run of many shots that
/// should use every core.
///
/// If it draws random numbers, it takes the run's seed when it is built, and
/// shot `j`'s components draw from the seeds of
/// [`ShotContext::new`]`(seed, j)`, each under its own id (an adapter's
/// from [`AdapterSeeds::shot`](crate::AdapterSeeds::shot)), so one number
/// replays the whole run; hardware takes none. Only an
/// [`executor_fn`](fn@crate::executor_fn) passes that seed up to the
/// adapters above it today ([`ExecutorFn::seed`](crate::ExecutorFn::seed));
/// one written by hand cannot state it yet (planned), so an adapter that
/// draws is refused over it
/// ([`BuildError::NoRunSeed`](crate::BuildError::NoRunSeed)). Each shot's
/// state carries over from one call to the next, until the executor is
/// closed.
pub trait Executor<M: Message> {
    /// The reply to one call, which delivers `M`'s events.
    type Reply: Reply<Item = M::Event>;

    /// Starts the work of `message` and returns its reply.
    ///
    /// # Errors
    ///
    /// [`Rejected`] when the message is refused. An executor's refusal means
    /// that no work was started, so the caller may correct the message and
    /// send it again without undoing anything. A rejection of class
    /// [`RetryClass::Resend`](crate::RetryClass::Resend) (such as
    /// [`Rejected::NoReply`]) instead means that no verdict arrived over a
    /// link, so work may have run. A run that starts and cannot finish is
    /// reported through the reply instead.
    fn call(&mut self, message: M::Ref<'_>) -> Result<Self::Reply, Rejected>;
}

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
/// parts keep them. The `conformance` module checks both
/// (`default_is_empty`, `folds_in_any_order`).
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
/// chain it calls: its run does, once, when the program's future has ended
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
/// ([`Capabilities::probabilities`]).
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
/// run, for a driver that stops a shot whose weight is too small.
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
