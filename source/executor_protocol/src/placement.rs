// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Where an executor runs ([`Placement`]): on a thread of its own
//! ([`OwnThread`]), or wherever a closure puts its work; and the executor
//! so placed ([`PlacedExecutor`]), which `.run_on(place)` makes
//! ([`ExecutorFn::run_on`], [`OnePerShotExecutor::run_on`]); given a
//! state per shot after it, each shot runs on a placement of its own
//! ([`PlacedExecutor::shots`]).

use std::collections::VecDeque;
use std::mem;
use std::ops::Deref;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::task::Waker;
use std::thread;

use crate::absorb::{HeldPart, absorb, absorb_each};
use crate::lifecycle::{ExecutorLifecycleHandle, PANICKED, PartsFailure, failing_on_panic};
use crate::{
    Capabilities, Close, Describe, Executor, ExecutorLifecycle, IncrementalReply, InstructionBatch,
    InstructionBatchRef, Rejected, Reply, ReplySender, ReplyStatus, ShotOutcome, ShotSet,
    ShotStrategy, relay, reply_channel,
};
#[cfg(doc)]
use crate::{ExecutorFn, OnePerShot, OnePerShotExecutor, OnePerShotPlacedExecutor};

/// The work a [`PlacedExecutor`] hands its placement: it runs the calls
/// queued so far, in order, and those queued while it runs, then ends.
pub type PlacementJob = Box<dyn FnOnce() + Send>;

/// Where an executor runs, given to `.run_on(place)`: [`OwnThread`], or a
/// closure that is handed each [`PlacementJob`] and runs it where it
/// chooses, such as `move |job| pool.spawn(job)` for a thread pool.
///
/// The closure's argument type is inferred where the closure passes the
/// job on (`pool.spawn(job)`, `thread::spawn(job)`); one that calls it
/// itself names it: `|job: PlacementJob| job()`.
///
/// A placement runs or drops each job it is handed: a job dropped unrun
/// fails the calls it held, but one kept unrun holds the executor's calls,
/// their replies and its `close` until it runs. And since `close`, or a
/// `wait` on one of its replies, blocks its thread until a job ran, a
/// placement must not defer a job to run later on that thread (a one-thread
/// runtime's spawn, under the task that closes or waits), or it waits for
/// ever; nor may a waker that runs on the executor's thread (woken by a
/// reply, inside a job) close the executor, or `wait` on one of its
/// replies, since it would wait for the job it runs in.
///
/// Dropping a placed executor does not wait for its placement: its calls
/// not yet started are cancelled, and a last job drops the executor it
/// places, with the states it holds, after the call running, if any (a call
/// cannot be stopped mid-way). So what those states hold (a device, GPU
/// memory) is freed some time after `drop` returns: call `close`, which
/// waits, before building another executor on it ([`PlacedExecutor`]). A
/// placement must not panic as `drop` hands it that last job: the panic
/// leaves `drop`, and aborts the process when the drop runs during an
/// unwind.
///
/// A job keeps the panics of the code it runs, so that none reaches the
/// placement's thread (a pool's). A call's fails its reply, since the
/// caller may be elsewhere by then. An instruction's drop that panics, the
/// caller's code, is absorbed, whether its call ran or not; so is the drop
/// of an executor of the crate it places, the implementer's code (a
/// state's, or what a closure captures), wherever it is freed but by
/// `close`, which raises a state's ([`PlacedExecutor`]); and so is the
/// drop of the placement, or of what it captures once started: `drop`
/// never panics. The
/// close's (a report closure's, or a reported state's drop) is raised
/// again in the caller of `close`, which waits for it, its payload
/// unchanged, as `std::thread::scope` and rayon raise the panic of work
/// moved to another thread in the caller that waits for it: the least
/// surprise for a Rust user ([`PlacedExecutor`]).
///
/// Planned, not built: `Remote(link)`, an executor serving the far end of
/// a link (the near end is an executor of its own); and placing any
/// executor, not only those `executor_fn` makes, or a state that is not
/// `Send` (`docs/executors.md`, §Where an executor runs).
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a placement",
    note = "a placement is `OwnThread`, or a closure `FnMut(PlacementJob)`; \
            `Remote(link)` is planned, not built"
)]
pub trait Placement {
    /// Starts this placement for one executor, at its first call: what
    /// runs the executor's jobs from then on. The executor hands it one
    /// job at a time.
    fn start(self) -> Box<dyn FnMut(PlacementJob) + Send>;
}

/// The placement on a thread of the executor's own, made at its first
/// call: every call of the executor runs on that one thread, so that an
/// executor that must run on one thread (a device's driver that expects
/// it) can be placed, provided it and its state are [`Send`]: the executor
/// is moved to the thread, and a placed executor is an executor only when
/// what it places is `Send`. The thread ends once the executor is dropped,
/// its call running has finished (those queued are cancelled), and the
/// executor it places was dropped there. A call that panicked there stops
/// the placement, though: the executor it places is then dropped where it
/// is closed, or, never closed, usually where the [`PlacedExecutor`] is
/// dropped, not on that thread (a known issue).
///
/// Planned, not built: placing a state that is not `Send`, because it is
/// bound to the thread that made it, such as an engine's GPU context or a
/// state built on `Rc`. Under `.run_on(OwnThread)`, the state closure
/// would run on that thread, so that the state never leaves it and needs
/// no `Send`, as in a prototype of this crate. Today the compiler refuses
/// such a state where the placed executor is called, or at `.shots(..)`
/// after `.run_on(..)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwnThread;

impl Placement for OwnThread {
    /// # Panics
    ///
    /// When the system cannot make a thread, as [`thread::spawn`].
    fn start(self) -> Box<dyn FnMut(PlacementJob) + Send> {
        let (jobs, queued) = mpsc::channel::<PlacementJob>();
        thread::spawn(move || {
            for job in queued {
                job();
            }
        });
        Box::new(move |job| {
            // The thread runs every job sent, and ends only once this
            // sender is dropped: the send cannot fail.
            jobs.send(job).expect("the executor's thread outlives it");
        })
    }
}

impl<F: FnMut(PlacementJob) + Send + 'static> Placement for F {
    fn start(self) -> Box<dyn FnMut(PlacementJob) + Send> {
        Box::new(self)
    }
}

/// An executor run where a [`Placement`] says, made by `.run_on(place)`: its
/// calls are queued and run, in order, in the jobs it hands its placement, one
/// job at a time. A call returns at once when the placement schedules the job
/// to run elsewhere, and after its work when the placement runs the job inline
/// (`|job: PlacementJob| job()`).
///
/// - Its reply is an [`IncrementalReply`]: the events come as the
///   executor it places hands them over on its thread, which wakes the
///   caller. The batch is copied into the call's job.
/// - It refuses a call once closed ([`Rejected::Closed`]), then after a
///   failure ([`Rejected::EarlierFailure`]). Every other refusal comes on
///   the executor's thread, after the call was queued: the call's reply
///   fails with it, as with a failure.
/// - A failure (a failed reply, a refusal on the thread) is recorded as it
///   happens on the thread: the calls queued after it fail without running
///   (`an earlier call failed: …`), since the state is part-way through
///   the failed call, and every later call is refused.
/// - A panic in the executor's `call` fails its call (`the executor
///   panicked during the call`) and stops the placement: the calls queued
///   behind it fail as never run (`the call never ran: its placement
///   stopped`), and every later call is refused with the panic's reason.
///   The panic stays in the job, and does not reach the placement's
///   thread, which may serve other executors (a pool's).
/// - A caller's waker that panics as it is woken on the placement's
///   thread (as the call hands over its events, or ends) is the caller's
///   bug, not the executor's: its panic is absorbed there, after the panic
///   hook reported it, and the call goes on as if the waker had returned:
///   the call ends as it would have, and the executor stays open.
/// - So is an instruction whose drop panics as the copy of the batch the
///   call's job holds is dropped: after the call, as its panic unwinds
///   (which would otherwise abort the process), or unrun, wherever that
///   happens. A call that ran ends as it would have, and the placement
///   goes on serving the calls after it and keeps them in its report.
/// - A job the placement drops unrun stops it too: the calls it held fail
///   as never run, and every later call is refused with that reason.
/// - A panic inside its own `call`, on the caller's thread, reaches the
///   caller and fails it, as a `Mutex` is poisoned: every later call is
///   refused, with `the executor panicked during the call` when copying
///   the batch panicked (an instruction's `Clone`), and with `the call
///   never ran: its placement stopped` when its placement panicked as it
///   started or was handed the call's job, which dropped the call unrun.
/// - It promises what the executor it places promised when it was placed
///   ([`Describe`]).
/// - `close` waits for the calls queued, then closes the executor it
///   places (on its thread once started; before any call, on the
///   caller's) and returns its report: the report of the executor it
///   places, which counts only the calls that reached it, not those that
///   failed before (queued behind a failure, or never run);
///   empty once the placement stopped running the jobs (a call panicked,
///   or the placement dropped one unrun, or panicked as it was handed
///   one). So, placed whole,
///   `.shots(OnePerShot).run_on(..)` then reports no state, where
///   `.run_on(..).shots(..)` leaves out only the states whose placement
///   stopped and those a failed call made ([`OnePerShotPlacedExecutor`]).
///   Once the placement stopped, `close` drops the executor it places
///   where it is called, not on the placement, so that the states it
///   leaves out are freed when it returns, in both orders.
///   A placement that panicked as it started never took the executor:
///   `close` closes it where it is, and its report is whole.
/// - A panic of the executor it places as it closes, on its placement (a
///   report closure's, such as [`ExecutorFn::state_report`]'s, or a
///   reported state's drop), is raised
///   again in the caller of `close` ([`std::panic::resume_unwind`]), its
///   payload unchanged, as when it is not placed: work moved to another
///   thread reports its panic to the caller that waits for it, as
///   `std::thread::scope` and rayon do. It does not stop the placement,
///   which still drops the executor it places; a second `close` gives that
///   executor's second report (empty, by [`Close`]'s contract).
///
/// It is [`Send`] when the executor it places and the placement are, so
/// that a run can move it to the caller's thread; its replies always are.
///
/// Dropped, it cancels the calls queued and not yet started: none of them runs,
/// and each one's reply ends [`ReplyStatus::Cancelled`], so that a dropped
/// executor frees its states after the call in progress at most, as dropping
/// tokio's `JoinSet`, Java's `shutdownNow` and Python's
/// `shutdown(cancel_futures=True)` cancel the work not yet started. A call
/// already running is not stopped (a call cannot be stopped mid-way): it
/// finishes, and its reply ends as it would have. Then the executor it places
/// is dropped on its placement (in a last job), where its calls ran, as an
/// executor that must run on one thread needs ([`OwnThread`]); dropped before
/// its first call, it drops that executor where it is. The caller's code that
/// panics as a call is cancelled, its waker or an instruction's drop as the
/// call's batch is dropped, is absorbed, after the panic hook reported it, as
/// on the placement's thread: the calls after it are cancelled still, and
/// `drop` raises no caller's panic, during an unwind too, where a panic would
/// abort the process (Rust API Guidelines, C-DTOR-FAIL). `drop` does not wait
/// for that last job: what the states hold (a device, GPU memory) may still be
/// held when it returns, so call `close`, which waits, to free it before
/// building another executor on it. Once its placement stopped, no job runs any
/// more: the executor it places is then dropped by `close`, or else with the
/// last of it and its jobs, usually on the thread that drops it (a known issue,
/// flagged at its `Drop`). A state's drop that panics (the implementer's code)
/// is raised only by `close`, which frees the states before it returns (the
/// type's `close`). Wherever else the executor it places is freed, the panic is
/// absorbed, after the panic hook reported it: in its last job, by `drop`
/// (before the first call, or once its placement stopped), or by a job still
/// ending, or dropped unrun, that releases it last once its placement stopped;
/// so `drop` never panics, and no panic reaches the placement's thread. The
/// executors of the crate absorb that panic in their own drop, and the drops of
/// what their closures capture, also when `close` frees them. Its own drop
/// absorbs that of the placement, or of what it captures once started.
///
/// # Type parameters
///
/// - `E`: the executor it places, whose calls run on the placement: an
///   [`ExecutorFn`] or a [`OnePerShotExecutor`].
/// - `P`: the placement, given to `.run_on` ([`ExecutorFn::run_on`],
///   [`OnePerShotExecutor::run_on`]).
//
// Known issue, open (blocks neither this commit nor a later one, since no
// executor of the crate reaches it; documented and pinned rather than
// fixed): a panic in the poll of the reply of the executor it places fails
// that reply with `the executor dropped the reply before its end`, not
// with the panic's reason, and stops the placement only after; a call
// made in between is accepted, then fails as never run. Pinned by
// `a_reply_that_panics_fails_as_dropped_and_stops_the_placement`. No
// executor of the crate reaches it: their replies are `ImmediateReply`s,
// and `PlacedExecutor::new` is crate-internal.
//
// Design question, open (blocks neither this commit nor a later one): a
// refusal on the executor's thread fails the executor, also one the caller
// is told it may retry (`Rejected::Unavailable`, `RetryClass::Later`), such
// as a busy device's under `.shots(OnePerShot).run_on(..)`, whose
// states are made on the thread. So does every refusal of the executor it
// places (its check's, such as a tag repeated within a batch, the one for
// want of room, its state closure's), which then arrives through the
// reply: such a placed executor cannot keep the conformance kit's
// `refuses_a_tag_repeated_within_a_batch` and
// `accepts_calls_after_a_refusal`, while `.run_on(..).shots(..)` refuses
// at `call`. Options: make the states, and run the check, on the caller's
// thread; record no refusal of class `Later`; or keep the permanent
// failure and say so beside `RetryClass::Later` and in
// docs/rejections.md.
pub struct PlacedExecutor<E, P> {
    capabilities: Capabilities,
    lifecycle: ExecutorLifecycle,
    calls: SharedCallQueue<E>,
    /// The placement until it is started, at the first call. It and the
    /// runner are each dropped in a catch of their own, so that a panic of
    /// what they capture leaves neither `drop` nor `drop_and_wait`.
    placement: HeldPart<Option<P>>,
    /// Once the placement is started: hands it a job that runs the
    /// requests queued.
    runner: HeldPart<Option<Box<dyn FnMut() + Send>>>,
    /// Frees the executor it places on the caller's thread, raising the
    /// first panic of its states' drops, which that executor's own `drop`
    /// absorbs (with those of what its closures capture): for `close` once
    /// its placement stopped, and for a shot strategy's frees
    /// ([`PlacedExecutor::drop_and_wait`],
    /// [`PlacedExecutor::drop_here_if_unstarted`]).
    free_here: fn(E),
}

impl<E: Describe, P> PlacedExecutor<E, P> {
    /// `executor`, to run where `placement` says from its first call on,
    /// with the refusals of `lifecycle`: the executor's own, continued
    /// ([`ExecutorLifecycle::continued`]), so that placing a failed or
    /// closed executor gives one that refuses at `call` as it did.
    /// `free_here` frees the executor on the caller's thread, raising the
    /// first panic of its states' drops, where it is not freed on its
    /// placement.
    pub(crate) fn new(
        executor: E,
        placement: P,
        lifecycle: ExecutorLifecycle,
        free_here: fn(E),
    ) -> Self {
        Self {
            capabilities: executor.capabilities(),
            lifecycle,
            calls: SharedCallQueue::new(CallQueue {
                queue: Mutex::new(Queue {
                    requests: VecDeque::new(),
                    draining: false,
                    stopped: false,
                }),
                executor: Mutex::new(Some(executor)),
            }),
            placement: HeldPart::new(Some(placement)),
            runner: HeldPart::new(None),
            free_here,
        }
    }

    /// [`PlacedExecutor::new`], as a part of an executor that reads `whole`
    /// at each call: each of its failures is recorded there too, on the
    /// thread where it happens.
    pub(crate) fn part_of(
        executor: E,
        placement: P,
        whole: PartsFailure,
        free_here: fn(E),
    ) -> Self {
        Self::new(
            executor,
            placement,
            ExecutorLifecycle::part_of(whole),
            free_here,
        )
    }
}

impl<E, P> PlacedExecutor<E, P> {
    /// The executor it places, its placement and its lifecycle, taken back
    /// before its first call: for a shot strategy that places each shot's
    /// state on a placement of its own, and continues this executor's
    /// refusals (the lifecycle): those of the executor it places, continued
    /// when it was placed, and its own, such as a call whose batch could
    /// not be copied, which never reached the executor it places.
    ///
    /// # Panics
    ///
    /// Once it was called: the executor it places, and the state it holds,
    /// are then on the placement's thread (taking them back is not built).
    pub(crate) fn into_unstarted(mut self) -> (E, P, ExecutorLifecycle) {
        let placement = (self.placement.take()).expect(
            "a state per shot is chosen before the first call: \
             `.run_on(..).shots(..)` on an executor already called is not built",
        );
        let executor = (lock(&self.calls.executor).take())
            .expect("the executor it places is held until it is dropped");
        // Taken, not moved out: it implements `Drop`.
        (executor, placement, std::mem::take(&mut self.lifecycle))
    }
}

// Known issue, open (blocks neither this commit nor a later one; a limit
// recorded with the fix that queues the drop): once the placement stopped
// (a job panicked, or the placement dropped one unrun), the queue drops
// the drop request, so the executor it places is dropped by `close`, on
// the closer's thread, or else with the last `Arc` of the queue, usually
// here, on the thread that drops this one: wrong for an executor that must
// run on its placement's thread (a device's driver). Dropping it as the
// queue stops would hold for a job that panicked, not for a placement that
// drops jobs, wherever it drops them. A state's drop that panics is
// absorbed wherever the executor it places is freed, but by `close`, which
// raises it (the type's rule).
// Pinned by `once_stopped_it_drops_the_executor_it_places_where_it_is_dropped`
// and `once_stopped_its_close_frees_the_states_of_the_executor_it_places`.
impl<E, P> Drop for PlacedExecutor<E, P> {
    /// Once started, cancels the calls queued, queues the drop of the
    /// executor it places, behind the call running if any, and hands the
    /// placement a job to run it if none is running; nothing waits for it.
    fn drop(&mut self) {
        if let Some(hand) = &mut *self.runner
            && self.calls.cancel_then_drop(None)
        {
            hand();
        }
    }
}

impl<E, P> PlacedExecutor<E, P> {
    /// Drops it as `drop` does, then waits until the executor it places,
    /// with the states it holds, was dropped on its placement: for a shot
    /// strategy whose `close` frees every state before it returns, as
    /// [`Placement`]'s doc tells callers to rely on. Before its first
    /// call, the executor it places is freed here; once its placement
    /// stopped, no job runs any more: the wait ends at once, and it takes
    /// that executor from the queue and frees it here, as `close` does,
    /// rather than leave it to the last of its jobs, which may still be on
    /// the placement's thread
    /// (`close_frees_a_stopped_placements_state_before_it_returns_though_its_job_is_still_ending`).
    ///
    /// # Panics
    ///
    /// When its placement panics as it is handed the drop's job (as
    /// `call`'s). Once its placement stopped, or before its first call,
    /// when a state of the executor it places panics as it is freed here
    /// (a state's drop, the implementer's code; the first one's, once
    /// every state is freed).
    pub(crate) fn drop_and_wait(mut self) {
        // Taken, so that `drop` queues no second drop.
        let Some(hand) = self.runner.take() else {
            self.drop_here_if_unstarted();
            return;
        };
        // Held, so that the placement it captures is dropped in a catch of
        // its own, during an unwind too.
        let mut hand = HeldPart::new(hand);
        let (dropped, freed) = mpsc::channel();
        if self.calls.cancel_then_drop(Some(dropped)) {
            (*hand)();
        }
        // An error says that the request was dropped: the placement stopped.
        if freed.recv().is_err() {
            self.free_executor_here();
        }
    }
}

impl<E, P> PlacedExecutor<E, P> {
    /// Drops it as `drop` does, but before its first call frees the
    /// executor it places here, outside the queue's catch: for a shot
    /// strategy that frees the states made for a call it refuses, as the
    /// executor not placed does, raising the first panic of their drops.
    ///
    /// # Panics
    ///
    /// Before its first call, when a state of the executor it places
    /// panics as it is freed (a state's drop, the implementer's code; the
    /// first one's, once every state is freed).
    pub(crate) fn drop_here_if_unstarted(self) {
        if self.runner.is_none() {
            self.free_executor_here();
        }
    }

    /// Takes the executor it places from the queue, if still there, and
    /// frees it here, raising the first panic of its states' drops
    /// (`free_here`).
    fn free_executor_here(&self) {
        let executor = lock(&self.calls.executor).take();
        if let Some(executor) = executor {
            (self.free_here)(executor);
        }
    }
}

impl<E: Send + 'static, P: Placement> PlacedExecutor<E, P> {
    /// The executor that holds its shots' states as `strategy` says, each
    /// on a placement of its own: `.run_on(OwnThread).shots(OnePerShot)`
    /// runs each shot on a thread of its own, where
    /// `.shots(OnePerShot).run_on(OwnThread)` runs every shot on the
    /// one thread ([`ExecutorFn::run_on`]). It continues this executor's
    /// refusals, its own and those of the executor it places, since a
    /// wrapper never forgets a refusal.
    ///
    /// Known issue, open: each shot's reply wakes the caller, at most once
    /// per sleep, as the shot's events come and when it ends; the caller
    /// polls the reply once per wake, and each poll polls every shot still
    /// pending, so a batch of n shots, woken once per shot that ends, costs
    /// O(n²) polls of shots (`docs/performance.md`, §Shots on placements of
    /// their own).
    ///
    /// # Panics
    ///
    /// Once it was called (for [`OnePerShot`]): the state it holds is then
    /// on its placement's thread, and taking it back is not built. Or when
    /// the executor it places holds a state, from a call made before it
    /// was placed, and a `Clone` of the logic, of a report closure or of
    /// the placement panics as that state's shot's copies are made (for
    /// [`OnePerShot`]; the state is freed as the panic unwinds).
    pub fn shots<H: ShotStrategy<Self>>(self, strategy: H) -> H::Executor {
        strategy.apply(self)
    }

    /// Queues `request`, and hands the placement a job to run it if none
    /// is running. The placement is started first, so that should it
    /// panic as it starts, `request` is dropped, which fails its call.
    fn queue(&mut self, request: Request<E>) {
        let (calls, placement) = (&self.calls, &mut self.placement);
        let hand = self.runner.get_or_insert_with(|| {
            let mut place = (placement.take())
                .expect("a placement not started is held")
                .start();
            let calls = calls.clone();
            Box::new(move || {
                let job = Drain {
                    calls: calls.clone(),
                    ended: false,
                };
                place(Box::new(move || job.run()));
            })
        });
        if self.calls.queue(request) {
            hand();
        }
    }

    /// Waits until the calls queued so far ended on its placement, so that
    /// the failures they record ([`PlacedExecutor::part_of`]) are known:
    /// for a shot strategy that decides at close which shots to report,
    /// before it closes any. Returns at once before the first call, and
    /// once the placement stopped, when no call runs any more.
    ///
    /// # Panics
    ///
    /// When its placement panics as it is handed the wait's job (as
    /// `call`'s).
    pub(crate) fn wait_for_calls(&mut self) {
        if self.runner.is_none() {
            return;
        }
        let (waiting, ended) = mpsc::channel();
        self.queue(Request::Wait(waiting));
        // An error says that the request was dropped: the placement stopped.
        let _ = ended.recv();
    }
}

impl<I, E, P> Executor<InstructionBatch<I>> for PlacedExecutor<E, P>
where
    I: Clone + Send + 'static,
    E: Executor<InstructionBatch<I>> + Send + 'static,
    P: Placement,
{
    /// The events come from the executor's thread.
    type Reply = IncrementalReply<ShotOutcome>;

    /// # Errors
    ///
    /// [`Rejected::Closed`] once closed, then [`Rejected::EarlierFailure`]
    /// after a failure. The refusals of the executor it places fail the
    /// reply instead.
    ///
    /// # Panics
    ///
    /// When its placement panics as it starts, at the first call
    /// ([`OwnThread`] when the system cannot make a thread), or as it is
    /// handed a job. The calls queued then never run, and every later
    /// call is refused (`the call never ran: its placement stopped`).
    /// When copying the batch panics (an instruction's `Clone`): the call
    /// never runs, and every later call is refused (`the executor
    /// panicked during the call`).
    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<Self::Reply, Rejected> {
        failing_on_panic(self, |this| &this.lifecycle, |this| this.serve(batch))
    }
}

impl<E: Send + 'static, P: Placement> PlacedExecutor<E, P> {
    /// Serves `batch`: the steps of `call`, which runs them failing the
    /// executor should they panic ([`failing_on_panic`]).
    fn serve<I>(
        &mut self,
        batch: InstructionBatchRef<'_, I>,
    ) -> Result<IncrementalReply<ShotOutcome>, Rejected>
    where
        I: Clone + Send + 'static,
        E: Executor<InstructionBatch<I>>,
    {
        self.lifecycle.admit()?;
        // Copied before the call is queued: should a copy panic, the call
        // fails as one that panicked, not as one never run. Copied into its
        // guard, one instruction at a time, so that the copies made before
        // one panics are in it, and dropped through it ([`BatchCopy`]): the
        // standard library does not promise that extending a `Vec` from a
        // slice keeps them.
        let mut copy = BatchCopy {
            shots: batch.shots.clone(),
            instructions: Vec::with_capacity(batch.instructions.len()),
        };
        for instruction in batch.instructions {
            copy.instructions.push(instruction.clone());
        }
        // On the executor's thread, its sender wakes the caller's waker,
        // code the executor does not own: its panic is absorbed, so that it
        // neither fails the executor nor escapes `drop`.
        let (sender, reply) = reply_channel();
        let call = QueuedCall {
            lifecycle: self.lifecycle.handle(),
            sender: Some(sender.absorbing_waker_panics()),
            running: false,
        };
        self.queue(Request::Call {
            call,
            run: Box::new(move |call: QueuedCall, executor: &mut E| {
                call.run(executor, copy.as_ref());
            }),
        });
        Ok(reply)
    }
}

impl<E, P> Describe for PlacedExecutor<E, P> {
    /// The promise of the executor it places, when it was placed.
    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }
}

impl<E, P> Close for PlacedExecutor<E, P>
where
    E: Close + Send + 'static,
    E::Report: Send,
    P: Placement,
{
    type Report = E::Report;

    /// Refuses later calls, waits for the calls queued, and closes the
    /// executor it places, on its thread once started. A placement that
    /// panicked as it started never took the executor: `close` closes it
    /// where it is, and its report is whole; one that panicked as it was
    /// handed a job stopped, and the report is empty. A second `close` runs
    /// where the first did, or nowhere once the placement stopped, and gives
    /// an empty report (by [`Close`]'s contract).
    ///
    /// # Panics
    ///
    /// When its placement panics as it is handed the close's job (as `call`'s):
    /// the report of the executor it places is then lost. When the executor it
    /// places panics as it closes, on its placement (a report closure's, or its
    /// report's `Default`): that panic is raised again here, its payload
    /// unchanged, as `std::thread::scope` raises a scoped thread's panic in its
    /// caller (the type's doc). Once its placement stopped, `close` frees the
    /// executor it places here: the panic of a state's drop (the implementer's
    /// code; the first one's, once every state is freed) then reaches the
    /// caller.
    fn close(&mut self) -> E::Report {
        self.lifecycle.close();
        if self.runner.is_none() {
            return (lock(&self.calls.executor).as_mut())
                .expect("the executor it places is held until it is dropped")
                .close();
        }
        let (report, reported) = mpsc::channel();
        self.queue(Request::Close(Box::new(move |executor: &mut E| {
            // Caught, to be raised again in the closer, and so that it does
            // not stop the placement, which still drops the executor. The
            // closer waits for the report until the request is dropped.
            let closed = panic::catch_unwind(AssertUnwindSafe(|| executor.close()));
            let _ = report.send(closed);
        })));
        match reported.recv() {
            Ok(Ok(report)) => report,
            Ok(Err(panic)) => panic::resume_unwind(panic),
            Err(mpsc::RecvError) => {
                // The request was dropped: the placement stopped, and no
                // job runs any more. The executor it places is freed here,
                // so that its states are freed when `close` returns, as the
                // type's doc promises, and the first panic of their drops,
                // which that executor's own `drop` absorbs, is raised.
                self.free_executor_here();
                E::Report::default()
            }
        }
    }
}

/// What runs a queued call on the executor, its batch copied in.
type RunCall<E> = Box<dyn FnOnce(QueuedCall, &mut E) + Send>;

/// What the executor's thread runs: a call, the close, or a wait for the
/// calls queued before it, then the drop of the executor once the
/// [`PlacedExecutor`] is dropped.
enum Request<E> {
    /// A call, which `run` runs on the executor; or, should the executor
    /// be dropped before it starts, `call` cancels it.
    Call {
        call: QueuedCall,
        run: RunCall<E>,
    },
    Close(Box<dyn FnOnce(&mut E) + Send>),
    /// Tells the caller waiting on it that the calls queued before it
    /// ended ([`PlacedExecutor::wait_for_calls`]).
    Wait(mpsc::Sender<()>),
    /// Drops the executor, then tells the caller waiting on it, if any
    /// ([`PlacedExecutor::drop_and_wait`]).
    Drop(Option<mpsc::Sender<()>>),
}

/// The calls of a [`PlacedExecutor`] waiting to run, and the executor
/// they run on, shared by it and the job running them.
struct CallQueue<E> {
    queue: Mutex<Queue<E>>,
    /// Taken by the job for each request it runs; uncontended, since one
    /// job runs at a time. Emptied by the last request, the drop.
    executor: Mutex<Option<E>>,
}

struct Queue<E> {
    requests: VecDeque<Request<E>>,
    /// Whether a job was handed to the placement and has not ended: it
    /// runs every request queued until the queue is empty.
    draining: bool,
    /// Set when a job stopped before it ended (it panicked, or the
    /// placement dropped it unrun): no request runs any more.
    stopped: bool,
}

impl<E> CallQueue<E> {
    /// Queues `request`: whether a job must be handed to the placement to
    /// run it. Once stopped, it drops `request`, which fails its call.
    fn queue(&self, request: Request<E>) -> bool {
        let mut queue = lock(&self.queue);
        if queue.stopped {
            drop(queue);
            // Dropped after the unlock: dropping it fails its call and drops
            // its batch's copy, whose instructions' drops are the caller's
            // code, not to run under the lock; their panic is absorbed
            // ([`BatchCopy`]), so none reaches the caller of `call`.
            drop(request);
            return false;
        }
        queue.requests.push_back(request);
        !mem::replace(&mut queue.draining, true)
    }

    /// Runs the requests queued, in order, until there are none.
    fn drain(&self) {
        loop {
            let request = {
                let mut queue = lock(&self.queue);
                let Some(request) = queue.requests.pop_front() else {
                    queue.draining = false;
                    return;
                };
                request
            };
            match request {
                Request::Call { call, run } => {
                    let mut executor = lock(&self.executor);
                    run(
                        call,
                        executor.as_mut().expect("the drop is the last request"),
                    );
                }
                Request::Close(close) => {
                    close((lock(&self.executor).as_mut()).expect("the drop is the last request"));
                }
                Request::Wait(waiting) => {
                    // Its error is ignored: the waiter is gone only if its
                    // placement panicked as it was handed this job.
                    let _ = waiting.send(());
                }
                Request::Drop(dropped) => {
                    let executor = lock(&self.executor).take();
                    drop(executor);
                    if let Some(dropped) = dropped {
                        // Its error is ignored, as the wait's.
                        let _ = dropped.send(());
                    }
                }
            }
        }
    }

    /// Cancels the calls queued, which have not started, and queues the
    /// drop of the executor in their place, which tells `dropped` once done:
    /// whether a job must be handed to the placement to run it. Once
    /// stopped, it queues nothing, as [`CallQueue::queue`], and drops
    /// `dropped`.
    fn cancel_then_drop(&self, dropped: Option<mpsc::Sender<()>>) -> bool {
        let mut queue = lock(&self.queue);
        if queue.stopped {
            return false;
        }
        let cancelled = mem::replace(
            &mut queue.requests,
            VecDeque::from([Request::Drop(dropped)]),
        );
        let hand = !mem::replace(&mut queue.draining, true);
        drop(queue);
        // After the unlock, as `stop` drops its own: cancelling wakes each
        // caller, and drops its batch, the caller's code. A caller's waker
        // that panics leaves no call after it uncancelled: its panic is
        // absorbed where it is woken (`QueuedCall`'s sender). So does an
        // instruction whose drop panics ([`BatchCopy`]).
        for request in cancelled {
            if let Request::Call { call, .. } = request {
                call.cancel();
            }
        }
        hand
    }

    /// No request runs any more: those queued are dropped, which fails
    /// their calls. Run by [`Drain`]'s drop, outside the job's catch (after
    /// it, or where the placement drops the job unrun): an instruction
    /// whose drop panics is absorbed by its batch's copy ([`BatchCopy`]).
    fn stop(&self) {
        let dropped = {
            let mut queue = lock(&self.queue);
            queue.stopped = true;
            mem::take(&mut queue.requests)
        };
        drop(dropped);
    }
}

/// The copy of a call's batch its job runs, made on the caller's thread,
/// into this guard. Wherever it is dropped, it absorbs the panics of its
/// instructions' drops, the caller's code, after the panic hook reported
/// them, each instruction in a catch of its own ([`absorb_each`]): after
/// the call, so that the placement goes on serving; during an unwind (the
/// executor's panic, the placement's as it starts or is handed a job, or
/// an instruction's `Clone` that panics part-way through the copy), where
/// a second panic would abort the process; and with a request that will
/// not run (the placement stops, or `drop` cancels it, or a call comes
/// once it stopped), so that the panic reaches neither the placement's
/// thread (a pool's, whose `spawn` may abort on it) nor the caller of
/// `call` or `drop` (C-DTOR-FAIL), and the requests after it still run or
/// end. As tokio's `cancel_task` catches the panic of a cancelled task's
/// drop. When nothing panics, its catches cost about 0.2 to 0.5 ns per
/// instruction (measured with `String` instructions: nothing measurable at
/// 1 and 16, 1 to 3 % of the call at 256, about 1.5 % at 64 to 1024 on a
/// logic that does nothing), beside the copy's clone and free, so every
/// call's copy is dropped in them.
struct BatchCopy<I> {
    shots: ShotSet,
    instructions: Vec<I>,
}

impl<I> BatchCopy<I> {
    fn as_ref(&self) -> InstructionBatchRef<'_, I> {
        InstructionBatchRef {
            shots: &self.shots,
            instructions: &self.instructions,
        }
    }
}

impl<I> Drop for BatchCopy<I> {
    fn drop(&mut self) {
        absorb_each(mem::take(&mut self.instructions), drop);
    }
}

/// The job a [`PlacedExecutor`] hands its placement. Should it not end (it
/// panicked, or the placement dropped it unrun), the queue stops, so that
/// no call waits for a job that will not come; the job then releases the
/// queue, which may free the executor it places there ([`SharedCallQueue`]).
struct Drain<E> {
    calls: SharedCallQueue<E>,
    ended: bool,
}

impl<E> Drain<E> {
    fn run(mut self) {
        // Absorbed, so that a panic does not reach the placement's thread
        // (a pool's): the call that panicked fails, and the queue stops.
        self.ended = absorb(|| self.calls.drain());
    }
}

impl<E> Drop for Drain<E> {
    fn drop(&mut self) {
        if !self.ended {
            self.calls.stop();
        }
    }
}

/// A [`CallQueue`] shared by a [`PlacedExecutor`], the runner that hands
/// its placement the jobs, and those jobs. Whichever releases it last
/// frees the executor it places, with the states it holds, unless that
/// executor was taken out first (by `close`, or by the drop's request run
/// on the placement): so before the first call, `drop`; once the
/// placement stopped (the drop's request is then dropped unrun), `drop`, a
/// job still ending, or one the placement drops unrun. A panic of that
/// executor's drop is absorbed in each release, after the panic hook
/// reported it, whatever that executor is, so that it reaches neither the
/// placement's thread (a pool's, whose `spawn` may abort on it) nor the
/// caller of `drop`, which never panics (C-DTOR-FAIL). The executors of
/// the crate absorb their drops' panics themselves (`HeldStates`,
/// `HeldPart`), and only they are placed (`PlacedExecutor::new` is
/// crate-internal); this guard keeps the rule for any executor, should
/// that open
/// (`an_executors_drop_that_panics_as_a_job_dropped_unrun_frees_it_stays_off_the_placements_thread`).
struct SharedCallQueue<E>(Option<Arc<CallQueue<E>>>);

impl<E> SharedCallQueue<E> {
    fn new(queue: CallQueue<E>) -> Self {
        Self(Some(Arc::new(queue)))
    }
}

impl<E> Clone for SharedCallQueue<E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<E> Deref for SharedCallQueue<E> {
    type Target = CallQueue<E>;

    fn deref(&self) -> &CallQueue<E> {
        (self.0.as_deref()).expect("the queue is held until its holder is dropped")
    }
}

impl<E> Drop for SharedCallQueue<E> {
    fn drop(&mut self) {
        let queue = self.0.take();
        absorb(|| drop(queue));
    }
}

/// A call queued on the executor's thread: its handle on the lifecycle,
/// the sender of its reply until it runs, and whether the executor's
/// `call` is under way. Dropped before it ran to its end, it records a
/// failure, then fails its reply. The sender absorbs a panic of the
/// caller's waker ([`PlacedExecutor`]), so that failing the reply in this
/// `Drop`, during the unwind of the executor's panic, does not abort.
struct QueuedCall {
    lifecycle: ExecutorLifecycleHandle,
    sender: Option<ReplySender<ShotOutcome>>,
    /// Whether the executor's `call` is under way: tells a panic of the
    /// executor's, as the call runs, from one of the placement's, which
    /// drops the call unrun, and from a call that returned on a thread
    /// already unwinding (a placement that runs the job inline, in a
    /// `Drop` during a panic), which did not fail.
    running: bool,
}

impl QueuedCall {
    /// Runs `batch` on `executor`, unless an earlier call failed, and
    /// hands its events to the caller as they come.
    fn run<I, E>(mut self, executor: &mut E, batch: InstructionBatchRef<'_, I>)
    where
        E: Executor<InstructionBatch<I>>,
    {
        if let Some(reason) = self.lifecycle.earlier_failure() {
            if let Some(sender) = self.sender.take() {
                sender.fail(reason);
            }
            return;
        }
        self.running = true;
        match executor.call(batch) {
            Ok(reply) => {
                let sender = self.sender.take().expect("a queued call runs once");
                relay(
                    Recorded {
                        reply,
                        lifecycle: &self.lifecycle,
                    },
                    sender,
                );
            }
            Err(rejected) => self.fail(rejected.to_string()),
        }
        self.running = false;
    }

    /// Ends its reply [`ReplyStatus::Cancelled`], unrun: its executor was
    /// dropped before it started. Records no failure: a cancelled call did
    /// not fail, and the executor, dropped, refuses no later call.
    fn cancel(mut self) {
        if let Some(sender) = self.sender.take() {
            sender.cancel();
        }
    }

    /// Records the call's failure, then fails its reply: recorded first,
    /// so that a caller that sees the reply fail finds the executor
    /// failed.
    fn fail(&mut self, reason: String) {
        if let Some(sender) = self.sender.take() {
            self.lifecycle.fail(&reason);
            sender.fail(reason);
        }
    }
}

impl Drop for QueuedCall {
    fn drop(&mut self) {
        if self.running && thread::panicking() {
            // Recorded even when the reply is already failing: its sender,
            // dropped as the panic left `relay`, failed it.
            //
            // Known issue, open (see `PlacedExecutor`): a panic in the
            // reply's poll fails the reply, as dropped, before this records
            // the panic, so a call made in between is accepted, then fails
            // as never run.
            self.lifecycle.fail(PANICKED);
            self.fail(PANICKED.to_owned());
        } else {
            self.fail("the call never ran: its placement stopped".to_owned());
        }
    }
}

/// The reply of a call on the executor's thread, which records its
/// failure on the lifecycle before handing it on: the rule of a
/// [`LifecycleBoundReply`](crate::LifecycleBoundReply) but for an earlier
/// call's failure, which cannot come while it runs, since the calls run
/// one after another.
struct Recorded<'a, R> {
    reply: R,
    lifecycle: &'a ExecutorLifecycleHandle,
}

impl<R: Reply> Reply for Recorded<'_, R> {
    type Item = R::Item;

    fn poll_events(&mut self, out: &mut Vec<R::Item>, waker: &Waker) -> ReplyStatus {
        let status = self.reply.poll_events(out, waker);
        if let Some(reason) = status.failure() {
            self.lifecycle.fail(reason);
        }
        status
    }
}

/// Locks `mutex`, poisoned or not. The queue's lock is held across no
/// code that can panic; the executor's is poisoned by a call that
/// panicked, after which the queue stops and no request takes it again.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
