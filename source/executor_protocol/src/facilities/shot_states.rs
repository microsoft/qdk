// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The states of an executor's shots, one per shot ([`ShotStates`]), and the
//! reply to a batch run on them ([`ShotReplies`]): the rules of an
//! executor that runs each shot on its own state, written once.

use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::task::Waker;

use crate::facilities::executor_lifecycle::{
    ExecutorLifecycle, ExecutorLifecycleHandle, PartsFailure, failing_on_panic,
};
use crate::facilities::panics::end_each_raising_first;
use crate::facilities::teardown::{DroppedOneByOne, HeldStates};
use crate::shot_set::ShotIndex;
use crate::{
    Close, Executor, ExecutorReport, Fold, InstructionBatch, InstructionBatchRef, Rejected, Reply,
    ReplyStatus, ShotId, ShotOutcome, ShotSet,
};

/// The states of an executor's shots, one per shot, each an executor of its
/// shot alone (`S: Executor<InstructionBatch<I>>`, called with the set of one
/// shot `{j}`): shot `j`'s state is made at the shot's first call. A state is
/// kept only once its shot ran a call; until then each call makes it anew. A
/// kept state serves the shot's later calls until the executor closes.
///
/// It applies the protocol's rules for such an executor once: a closed or
/// failed executor says so whatever the batch, before the executor's own check
/// of it; a batch runs on each of its shots in its order; each outcome a
/// shot's state gives is named by that shot, whatever the state named it; a
/// state that cannot be made refuses the call, with the refusal of its
/// `make`, and the states made for this call are not kept; a rejection by
/// the first shot refuses the call, since nothing ran, and one for the
/// state's own earlier failure ([`Rejected::EarlierFailure`], a failure its
/// reply had not shown yet) also fails the executor; a rejection by a later
/// shot, or a failed batch, fails the reply, and every later call is refused
/// ([`Rejected::EarlierFailure`]) from the moment it is recorded as the
/// executor's (in `call`, or when the reply is polled), since the shots it
/// hit are part-way through their instructions, or through a batch that ran
/// on some of its shots only; one recorded in `call` stops the batch's later
/// shots. A failed batch hands over its outcomes, then its failure. A later
/// call's reply still in flight then ends with that failure, without more
/// events ([`ShotReplies`]; a poll during which the failure is recorded as
/// the executor's may still hand over its own, as at
/// [`LifecycleBoundReply`](crate::LifecycleBoundReply)). A refused call runs
/// no shot and is not counted.
/// States that record their failures as they happen, on threads of their
/// own, share their first failure with it ([`PartsFailure`], given to
/// `continuing`): each call is refused by close, then by the earliest of
/// that shared failure and a failure recorded as the executor's, before
/// `check` ([`ShotStates::call`]), so a state's failure refuses every call
/// that starts after it, whatever its shots; that refusal ends no call
/// already in flight.
///
/// It keeps the refusals after a failure and after close for the states it
/// holds, so a state need not keep them ([`Executor`]'s contract): its own
/// `call` only runs or refuses its batch.
///
/// A state made but not kept is dropped without a report. After a failure,
/// the kept states stay until [`ShotStates::close`], which closes and
/// reports those made before the failed call, and drops the others unread:
/// a failed call reports no shot of it, whether the shot ran or not, so
/// that every form of an executor reads and reports the same states. `get`
/// (for tests) reads them only before then. Dropping
/// them at the failure is planned, for a case that needs their memory back
/// sooner, such as a state of gigabytes held after its executor failed.
///
/// Dropped, it frees the states it keeps each in a catch of its own, and
/// absorbs their drops' panics ([`HeldStates`]), so that it adds no panic
/// to the `drop` of an executor that holds it; [`ShotStates::close`]
/// raises the first.
///
/// Until a reply keeps a part pending after its call, a reply shares nothing
/// with the executor: a call on kept states whose replies end inside it,
/// with no outcome, allocates nothing.
#[derive(Debug)]
pub(crate) struct ShotStates<S> {
    states: HeldStates<Kept<S>>,
    index: ShotIndex,
    calls: u64,
    lifecycle: ExecutorLifecycle,
    /// The first failure of any state, for states that record their
    /// failures as they happen, on threads of their own: read at the start
    /// of each call.
    parts_failure: Option<PartsFailure>,
    /// Set once a reply kept a part pending after its call: that part's
    /// failure may come after later calls, whose replies must then check
    /// for it.
    parts_may_fail_late: bool,
}

/// No state yet.
impl<S> Default for ShotStates<S> {
    fn default() -> Self {
        Self {
            states: HeldStates::default(),
            index: ShotIndex::default(),
            calls: 0,
            lifecycle: ExecutorLifecycle::default(),
            parts_failure: None,
            parts_may_fail_late: false,
        }
    }
}

impl<S> ShotStates<S> {
    /// The states of an executor made of another, which continues it: its
    /// refusals (`lifecycle`), the `calls` it accepted since its last report,
    /// and the state it `held`, kept for its shot; `parts_failure`, which
    /// each state's failures set as they happen ([`PartsFailure`]).
    pub(crate) fn continuing(
        lifecycle: ExecutorLifecycle,
        calls: u64,
        held: Option<(ShotId, S)>,
        parts_failure: PartsFailure,
    ) -> Self {
        let mut index = ShotIndex::default();
        if let Some((j, _)) = &held {
            index.push(*j);
        }
        Self {
            states: (held.into_iter())
                .map(|(shot, state)| Kept {
                    shot,
                    state,
                    made_in: 0,
                })
                .collect::<Vec<_>>()
                .into(),
            index,
            calls,
            lifecycle,
            parts_failure: Some(parts_failure),
            parts_may_fail_late: false,
        }
    }

    /// Runs `batch` on the state of each of its shots, in its order,
    /// making with `make` the state of each shot not called before; `make`
    /// gives its refusal, saying why and how soon to retry, if it cannot.
    /// `check` is the executor's check of the batch, of what does not
    /// depend on a shot's state: it runs once, before any state is made. A
    /// state made for the call and not kept (the call was refused, or
    /// failed) is freed by `free`, each in a catch of its own, and the first
    /// one's panic is raised again once all are freed: for a placed state,
    /// whose drop absorbs that panic.
    ///
    /// # Errors
    ///
    /// [`Rejected::Closed`] after [`ShotStates::close`], then
    /// [`Rejected::EarlierFailure`] after a call failed, or a state's
    /// failure was recorded in the states' [`PartsFailure`], with the
    /// earliest failed call's reason, in whichever order the two were
    /// recorded; then `check`'s rejection; then `make`'s refusal when it
    /// fails for a shot, retried at the next call; then the rejection of the
    /// first shot called. Nothing ran in each case. When the first shot's
    /// rejection is [`Rejected::EarlierFailure`], its state failed: every later
    /// call is refused with it too.
    ///
    /// A rejection by a later shot is not an error, since the shots before it
    /// ran: the reply hands over the outcomes of the shots that ran, then ends
    /// [`ReplyStatus::Failed`] with `"shot j: …"`, the rejection of shot `j`.
    /// It, or a batch that failed on a shot before its `call` returned, refuses
    /// every later call, even if the reply is never polled. Unless an earlier
    /// call's failure is recorded as the executor's before the reply ends:
    /// then the reply ends with that failure instead, without more events
    /// ([`ShotReplies`]).
    pub(crate) fn call<I>(
        &mut self,
        batch: InstructionBatchRef<'_, I>,
        check: impl FnOnce(&[I]) -> Result<(), Rejected>,
        make: impl FnMut(ShotId) -> Result<S, Rejected>,
        free: impl FnMut(S),
    ) -> Result<ShotReplies<S::Reply>, Rejected>
    where
        S: Executor<InstructionBatch<I>>,
    {
        failing_on_panic(
            self,
            |this| &this.lifecycle,
            |this| this.serve(batch, check, make, free),
        )
    }

    /// Serves `batch`: the steps of [`ShotStates::call`], which runs
    /// them failing the executor should they panic ([`failing_on_panic`]).
    fn serve<I>(
        &mut self,
        batch: InstructionBatchRef<'_, I>,
        check: impl FnOnce(&[I]) -> Result<(), Rejected>,
        mut make: impl FnMut(ShotId) -> Result<S, Rejected>,
        mut free: impl FnMut(S),
    ) -> Result<ShotReplies<S::Reply>, Rejected>
    where
        S: Executor<InstructionBatch<I>>,
    {
        let admitted = self.lifecycle.admit();
        // A state's failure, whether or not its reply showed it: the
        // executor failed, and refuses this call before anything runs. The
        // reason is the earliest failed call's, whether that failure was
        // recorded as the executor's (a reply polled) or by its states only.
        if let Some(parts_failure) = &self.parts_failure
            && let Some((call, reason)) = parts_failure.first()
        {
            let recorded_earlier =
                (self.lifecycle.first_failed_call()).is_some_and(|own| own <= call);
            if !recorded_earlier && !matches!(admitted, Err(Rejected::Closed)) {
                return Err(Rejected::EarlierFailure(reason));
            }
        }
        admitted?;
        if let Some(parts_failure) = &self.parts_failure {
            parts_failure.begin(self.lifecycle.call());
        }
        check(batch.instructions)?;
        // Pushed one by one, and each taken out once its shot keeps it, so
        // that a refusal, a failure or a panic frees the states made for
        // the call and not kept, by `free`, each in a catch of its own
        // (`DroppedOneByOne`).
        let mut new_states = DroppedOneByOne::freed_by(|made: Option<S>| {
            if let Some(state) = made {
                free(state);
            }
        });
        for &j in batch.shots {
            if self.index.get(j).is_none() {
                new_states.push(Some(make(j)?));
            }
        }
        let mut next_new = 0;
        let mut reply = ShotReplies {
            ready: Vec::new(),
            end: None,
            late: None,
        };
        let mut pending = Vec::new();
        // Known issue (blocks neither): a second `index` lookup per shot.
        for (n, &j) in batch.shots.iter().enumerate() {
            let mut new_state = None;
            let state = if let Some(i) = self.index.get(j) {
                &mut self.states[i].state
            } else {
                new_state = Some(next_new);
                next_new += 1;
                new_states[next_new - 1]
                    .as_mut()
                    .expect("made above, in the batch's order")
            };
            let shot = ShotSet::one(j);
            let alone = InstructionBatchRef {
                shots: &shot,
                instructions: batch.instructions,
            };
            match state.call(alone) {
                Ok(mut part) => {
                    if let Some(at) = new_state {
                        let state = new_states[at].take().expect("taken once, here");
                        self.index.push(j);
                        self.states.push(Kept {
                            shot: j,
                            state,
                            made_in: self.lifecycle.call(),
                        });
                    }
                    // Polled at once, so that a batch that failed inside
                    // `call` fails the executor even if no one polls.
                    let from = reply.ready.len();
                    let status = part.poll_events(&mut reply.ready, Waker::noop());
                    name_by_shot(&mut reply.ready[from..], j);
                    match status {
                        ReplyStatus::Pending => pending.push((j, part)),
                        ReplyStatus::Done => {}
                        end @ (ReplyStatus::Failed(_) | ReplyStatus::Cancelled) => {
                            reply.end = Some(end);
                            break;
                        }
                    }
                }
                // Nothing started yet: the batch is refused as a whole. A
                // shot whose state failed already, though the failure is
                // not recorded as the executor's yet (its reply was not
                // polled since; with a `PartsFailure`, it came after the read
                // above), fails the executor, which refuses this call and every
                // later one, whatever their shots.
                Err(rejected) if n == 0 => return Err(self.lifecycle.pass_up_refusal(rejected)),
                // Other shots already run it: too late to refuse, so the
                // batch fails, and the rest of its shots do not run.
                Err(rejected) => {
                    reply.end = Some(ReplyStatus::Failed(format!("shot {j}: {rejected}")));
                    break;
                }
            }
        }
        // A part cancelled inside `call` fails the executor too, as a
        // failure does: the executor holds its states, so the cancellation
        // came from below it.
        //
        // Known issue, open (blocks neither): a part cancelled after
        // `call` fails nothing (`poll_parts`), by a rule made for parts
        // dropped with the executor, though a part of a state that is not
        // placed may be cancelled from below it too.
        if let Some(reason) = reply.end.as_ref().and_then(ReplyStatus::failure) {
            self.lifecycle.fail(reason);
        }
        // Shared only where a failure can come after the call: a reply's
        // handle costs an allocation and two atomic adds per call, and two
        // atomic loads per poll, one for a poll its first check ends.
        self.parts_may_fail_late |= !pending.is_empty();
        if self.parts_may_fail_late {
            reply.late = Some(Box::new(LateParts {
                lifecycle: self.lifecycle.handle(),
                pending,
            }));
        }
        // Known issue, open (blocks neither this commit nor a later one): a
        // panic in the loop above (a shot's executor's `call`, such as its
        // placement's as it starts) leaves the call uncounted, though shots
        // before it may have run, where `OnePerShotExecutor` counts a call
        // whose logic panicked (`ExecutorReport::calls`).
        self.calls += 1;
        Ok(reply)
    }

    /// The states kept: those of the shots that ran a call.
    pub(crate) fn len(&self) -> usize {
        self.states.len()
    }

    /// Whether it keeps shot `j`'s state.
    pub(crate) fn holds(&self, j: ShotId) -> bool {
        self.index.get(j).is_some()
    }

    /// Shot `j`'s state, as its calls so far left it; `None` before the
    /// shot accepted a call, and after [`ShotStates::close`]. Read by the
    /// crate's test double.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn get(&self, j: ShotId) -> Option<&S> {
        let i = self.index.get(j)?;
        self.states.get(i).map(|kept| &kept.state)
    }

    /// The executor's report: the calls it accepted, each state's report
    /// folded, and each shot's `log_p` where it gives one, except those of
    /// the states made for a call that failed, or for a later one: a failed
    /// call reports no shot of it. Every later call is refused
    /// ([`Rejected::Closed`]).
    ///
    /// It first waits for every state's calls (`wait_for_calls`: a placed
    /// state's, whose failures are known only once they ended), so that a panic
    /// after the waits (the report's `Default` included) ends no call accepted;
    /// then makes the report; then closes the states made before the first
    /// failed call, in the order of their shots' first calls, and frees the
    /// others unread (`discard`: a placed state is dropped on its placement,
    /// and waited for), so that what they hold (a device, GPU memory) is free
    /// when `close` returns, as
    /// [`ExecutorPlacement`](crate::ExecutorPlacement)'s doc tells callers to
    /// rely on: neither `log_p` nor `close` runs on a state made for
    /// a failed call, so that every form of an executor reads the same states.
    /// When a state's `wait_for_calls`, `log_p`, `close` or `discard` panics,
    /// or the report's `Default` (a state's report's, the implementer's code),
    /// that first panic leaves `close` (outside an unwind: see the known issue
    /// in its code), and every state it still holds, closed or not, is freed
    /// unreported (`discard`), before it leaves rather than as it unwinds: a
    /// state whose drop panics too (a placed state whose placement panics at
    /// every hand-off, its drop's included, against
    /// [`ExecutorPlacement`](crate::ExecutorPlacement)'s rule) would otherwise
    /// abort the process. Such a drop's panic is left to the panic hook, which
    /// reports it, so that the first panic is the one raised. A placed state's
    /// drop runs on its placement (`discard` waits for it there): the executor
    /// it places absorbs a panic of that drop there, after the panic hook
    /// reported it, so `discard` returns and `close` goes on; where `discard`
    /// drops a state here, its drop's panic leaves `close` as the first panic.
    /// The states closed are dropped last, each in a catch of its own, and the
    /// first drop's panic then leaves `close`, once all are dropped.
    pub(crate) fn close(
        &mut self,
        wait_for_calls: impl FnMut(&mut S),
        log_p: impl FnMut(&S) -> Option<f64>,
        mut discard: impl FnMut(S),
    ) -> ExecutorReport<S::Report>
    where
        S: Close,
    {
        self.lifecycle.close();
        let calls = std::mem::take(&mut self.calls);
        let mut states = self.states.take_raising_first();
        // Made in the catch, since its `Default` is the implementer's code
        // (a state's report's), so that the states are freed whatever panics;
        // and after the waits (`report_states`), since freeing a placed
        // state cancels its calls still queued.
        let mut report = None;
        // The states stay in `states`, borrowed, until the end, so that a
        // panic leaves them there, to be freed after the unwind; the states
        // closed are dropped with it, each in a catch of its own.
        let closed = panic::catch_unwind(AssertUnwindSafe(|| {
            self.report_states(
                &mut states,
                &mut report,
                calls,
                wait_for_calls,
                log_p,
                &mut discard,
            );
        }));
        if let Err(panic) = closed {
            end_each_raising_first(Some(panic), std::mem::take(&mut *states), |kept| {
                discard(kept.state);
            });
        }
        // Known issue, open (it needs two bugs at once; blocks neither this
        // commit nor a later one): no report was made only when a wait for
        // a state's calls or the report's `Default` panicked, before the
        // report was built, and then the panic was raised above, unless
        // the thread unwinds another panic (`close` run from a `drop`),
        // which goes on: the first panic is not raised, and the report
        // comes back without the states'; making the default here (again,
        // if it panicked) may then panic, which aborts the process, as an
        // adapter's close does (`report_states` in `adapter_fn.rs`).
        report.unwrap_or_else(|| ExecutorReport::of_calls(calls))
    }

    /// The steps of [`ShotStates::close`] that run the states' code: waits
    /// for every state's calls, makes the report of `calls` into `report`,
    /// frees the states made for a failed call or a later one, then reads
    /// the others into the report.
    fn report_states(
        &self,
        states: &mut Vec<Kept<S>>,
        report: &mut Option<ExecutorReport<S::Report>>,
        calls: u64,
        mut wait_for_calls: impl FnMut(&mut S),
        mut log_p: impl FnMut(&S) -> Option<f64>,
        mut discard: impl FnMut(S),
    ) where
        S: Close,
    {
        // Every state's calls ended before any state is read: a placed
        // state's failure, known only then, decides which states are read.
        // Known issue, open (blocks neither this commit nor a later one):
        // it waits for each state's calls, then closes each state, one shot
        // after another, so placed shots take 2n round trips to their
        // threads (38-41 ms for 256 shots in a debug build), and each state
        // it leaves out one more, as it is freed (`discard`). Queueing every
        // wait before receiving any answer is the remedy; not measured
        // against this.
        for kept in states.iter_mut() {
            wait_for_calls(&mut kept.state);
        }
        let report = report.insert(ExecutorReport::of_calls(calls));
        let parts_failed = self
            .parts_failure
            .as_ref()
            .and_then(PartsFailure::earliest_call);
        // Known issue, open (blocks neither this commit nor a later one):
        // two failures are not known at the call that failed, so the states
        // made for that call are reported. A state's failure that its reply
        // had not shown is first seen as its refusal of a later call (the
        // `EarlierFailure` arm with `n == 0` in `serve`), and recorded at
        // that later call; and a reply's failure that nobody polled before
        // close is not known here at all. States that record their failures
        // as they happen (placed shots, through `PartsFailure`) are not
        // affected; a plain state whose reply fails after its call is, such
        // as the crate's test double's.
        let failed = match (self.lifecycle.first_failed_call(), parts_failed) {
            (Some(own), Some(parts)) => own.min(parts),
            (own, parts) => own.or(parts).unwrap_or(u64::MAX),
        };
        // The same states read whatever the executor's form (placed or not,
        // a state per shot or not): a placed form may have run shots of the
        // failed call that the others never reached.
        for left_out in states.extract_if(.., |kept| kept.made_in >= failed) {
            discard(left_out.state);
        }
        for kept in states.iter_mut() {
            if let Some(ln_p) = log_p(&kept.state) {
                report.log_p.add(kept.shot, ln_p);
            }
            report.states.fold(kept.state.close());
        }
    }
}

/// A state kept by [`ShotStates`]: its shot's, made for the executor's call
/// `made_in` (by its lifecycle's numbering; 0 for one held before the
/// executor's first call), by which `close` leaves out the states made for
/// a failed call.
#[derive(Debug)]
struct Kept<S> {
    shot: ShotId,
    state: S,
    made_in: u64,
}

/// The reply of a batch run on several shots, each on an executor of its
/// own (each shot's placed executor under
/// [`OnePerShot`](crate::OnePerShot)): the replies `R` of the batch's
/// shots, as one reply, each shot's outcomes, named by the shot, in its
/// reply's order; outcomes of different shots may interleave. Each poll that
/// does not end the reply polls every part still pending, with the caller's
/// waker, so the first part with news wakes the caller; an earlier call's
/// failure wakes it too.
///
/// It ends once every part has ended: done, or with one failure: the call's,
/// when a later shot rejected the batch or a part failed inside `call`;
/// else the failure of the first part a poll finds failed, which that poll
/// records as the executor's, so every later call is refused from then on.
/// It ends
/// [`ReplyStatus::Cancelled`] when a part was cancelled and none failed: a
/// failure outranks a cancellation, whatever order the parts end or are
/// polled in, since a failure is news the caller must report, where a
/// cancellation only says that a shot never ran. A part cancelled inside
/// `call` fails the executor, as a failure does; one cancelled later
/// refuses nothing, for a placed part (under
/// [`OnePerShot`](crate::OnePerShot), placed): it was dropped with the
/// executor, and the replies of later calls end as their own parts do. A
/// part not placed may be cancelled later from below it too, and refuses
/// nothing either (a known issue, flagged in `ShotStates`' `serve`).
/// Once an earlier call's failure is recorded as the executor's (inside
/// `call`, or as a reply of that call was polled), this reply ends at its
/// next poll with that failure, even if it failed itself, and hands over
/// no more outcomes: neither those it holds from `call` nor those its
/// parts give. If that call failed during the poll in which the last part
/// ends, it ends at that poll, without the outcomes it held from `call` or
/// took in that poll. Until the failure is recorded as the executor's, this
/// reply ends as its own parts do, though, placed, a shot's failure shared
/// through `PartsFailure` refuses new calls already.
//
// Design question, open (blocks neither this commit nor a later one): a
// reply in flight ends at an earlier call's failure once it is recorded as
// the executor's, so under `.run_on(..).shots(..)` its end depends on the order
// in which the caller polls the replies; reading the shots' shared failure
// (`PartsFailure`) at each poll, as `call` does, would end it as the failure
// happens, for one atomic load per poll.
pub struct ShotReplies<R> {
    /// The outcomes taken already, for the next poll.
    ready: Vec<ShotOutcome>,
    /// How the reply ends, unless done (`Failed` or `Cancelled`): the
    /// call's failure (a later shot's rejection, or a part that failed or
    /// was cancelled inside `call`), else the parts' end that outranks the
    /// others ([`outrank`]).
    end: Option<ReplyStatus>,
    /// `None` while no reply of the executor kept a part pending.
    late: Option<Box<LateParts<R>>>,
}

/// The parts of a reply still pending after its call, and what the reply
/// needs once a failure can come after its call.
struct LateParts<R> {
    /// The handle of the call on the executor's lifecycle, through which a
    /// part's failure fails the executor at once.
    lifecycle: ExecutorLifecycleHandle,
    /// The parts still pending, each with its shot.
    pending: Vec<(ShotId, R)>,
}

impl<R> fmt::Debug for ShotReplies<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShotReplies").finish_non_exhaustive()
    }
}

impl<R: Reply<Item = ShotOutcome>> Reply for ShotReplies<R> {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        let Self { ready, end, late } = self;
        let Some(late) = late else {
            out.append(ready);
            return ended(end);
        };
        let LateParts { lifecycle, pending } = &mut **late;
        lifecycle.poll_bound(out, waker, |lifecycle, out, waker| {
            out.append(ready);
            if poll_parts(pending, lifecycle, end, out, waker) {
                ReplyStatus::Pending
            } else {
                ended(end)
            }
        })
    }
}

/// How a reply ends once its parts ended: with `end`, if any, else done.
fn ended(end: &mut Option<ReplyStatus>) -> ReplyStatus {
    end.take().unwrap_or(ReplyStatus::Done)
}

// Known issue, open (blocks a run of many shots per batch on
// `OnePerShotPlacedExecutor`, the first per-shot executor whose parts end
// after the call): each wake polls every pending part, so a reply costs
// O(pending) per wake, and O(n²) per batch for n shots that finish
// asynchronously.
// Measured by benches/shot_replies_wake.rs (docs/performance.md): woken
// once per shot, each poll costs about 19 ns per part still pending, and
// a batch of 1024 shots 27 times what one poll per batch costs. Per-part
// wakers or a ready queue would poll only the parts with news.
/// Polls the parts still `pending` into `out`, each part's outcomes named
/// by its shot; a part's failure or cancellation becomes the reply's end
/// where it outranks the end it has ([`outrank`]), and a failure fails the
/// executor through `lifecycle`. Whether some part is still pending.
fn poll_parts<R: Reply<Item = ShotOutcome>>(
    pending: &mut Vec<(ShotId, R)>,
    lifecycle: &ExecutorLifecycleHandle,
    end: &mut Option<ReplyStatus>,
    out: &mut Vec<ShotOutcome>,
    waker: &Waker,
) -> bool {
    pending.retain_mut(|(j, part)| {
        let from = out.len();
        let status = part.poll_events(out, waker);
        name_by_shot(&mut out[from..], *j);
        match status {
            ReplyStatus::Pending => true,
            ReplyStatus::Done => false,
            ReplyStatus::Failed(reason) => {
                // Recorded at once, so that the executor refuses its next
                // call even while other parts are pending.
                //
                // Design question, open (blocks neither this commit nor a
                // later one): passed up as the part gives it, so a call
                // queued on a shot behind its failure fails with `an
                // earlier call failed: …`, not named by the shot, where a
                // shot that refuses the call in `call` is (`shot j: …`).
                lifecycle.fail(&reason);
                outrank(end, ReplyStatus::Failed(reason));
                false
            }
            // Not recorded: the part was dropped with the executor, which
            // refuses no later call, and recorded, it would end each later
            // call's reply as failed by an earlier one, not as cancelled.
            ReplyStatus::Cancelled => {
                outrank(end, ReplyStatus::Cancelled);
                false
            }
        }
    });
    !pending.is_empty()
}

/// Makes `part_end`, a part's failure or cancellation, the reply's `end`
/// if it outranks it: a failure outranks a cancellation whatever the
/// order; of two failures, the first seen is kept. A failure is news the
/// caller must report, and it refuses later calls, where a cancellation
/// only says that a shot never ran.
fn outrank(end: &mut Option<ReplyStatus>, part_end: ReplyStatus) {
    let outranks = match end {
        None => true,
        Some(ReplyStatus::Cancelled) => matches!(part_end, ReplyStatus::Failed(_)),
        Some(_) => false,
    };
    if outranks {
        *end = Some(part_end);
    }
}

/// Names each of `outcomes`, which shot `j`'s state gave, by shot `j`: a
/// state's own naming is not trusted, so that no state can mislabel them.
fn name_by_shot(outcomes: &mut [ShotOutcome], j: ShotId) {
    for outcome in outcomes {
        outcome.shot = j;
    }
}
