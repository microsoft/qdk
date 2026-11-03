// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An executor run where a [`Placement`] says ([`PlacedExecutor`]), made by
//! `.run_on(place)` ([`ExecutorFn::run_on`],
//! [`OnePerShotExecutor::run_on`]); given a state per shot after it, each
//! shot runs on a placement of its own ([`PlacedExecutor::shots`]).
//!
//! - `call_queue`: the queue of its calls, shared by it and the job
//!   running them, and what a queued call holds as it waits and runs.

mod call_queue;

use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Mutex, mpsc};

use call_queue::{BatchCopy, CallQueue, Drain, Queue, QueuedCall, Request, SharedCallQueue, lock};

use crate::facilities::executor_lifecycle::{PartsFailure, failing_on_panic};
use crate::facilities::teardown::HeldPart;
use crate::{
    Capabilities, Close, Describe, Executor, ExecutorLifecycle, IncrementalReply, InstructionBatch,
    InstructionBatchRef, Placement, Rejected, ShotOutcome, ShotStrategy, reply_channel,
};
#[cfg(doc)]
use crate::{
    ExecutorFn, OnePerShot, OnePerShotExecutor, OnePerShotPlacedExecutor, OwnThread, ReplyStatus,
};

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
        strategy.hold_shots_of(self)
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
