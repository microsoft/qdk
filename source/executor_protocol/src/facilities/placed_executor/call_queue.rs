// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The queue of a [`PlacedExecutor`]'s calls, shared by it and the job
//! running them ([`CallQueue`]), and what a queued call holds as it waits
//! and runs ([`QueuedCall`]).

use std::collections::VecDeque;
use std::mem;
use std::ops::Deref;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::task::Waker;
use std::thread;

use crate::facilities::executor_lifecycle::{ExecutorLifecycleHandle, PANICKED};
use crate::facilities::panics::{absorb, absorb_each};
use crate::{
    Executor, InstructionBatch, InstructionBatchRef, Reply, ReplySender, ReplyStatus, ShotOutcome,
    ShotSet, relay,
};
#[cfg(doc)]
use crate::{ExecutorPlacement, PlacedExecutor};

/// What runs a queued call on the executor, its batch copied in.
type RunCall<E> = Box<dyn FnOnce(QueuedCall, &mut E) + Send>;

/// What the executor's thread runs: a call, the close, or a wait for the
/// calls queued before it, then the drop of the executor once the
/// [`PlacedExecutor`] is dropped.
pub(super) enum Request<E> {
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
pub(super) struct CallQueue<E> {
    pub(super) queue: Mutex<Queue<E>>,
    /// Taken by the job for each request it runs; uncontended, since one
    /// job runs at a time. Emptied by the last request, the drop.
    pub(super) executor: Mutex<Option<E>>,
}

pub(super) struct Queue<E> {
    pub(super) requests: VecDeque<Request<E>>,
    /// Whether a job was handed to the placement and has not ended: it
    /// runs every request queued until the queue is empty.
    pub(super) draining: bool,
    /// Set when a job stopped before it ended (it panicked, or the
    /// placement dropped it unrun): no request runs any more.
    pub(super) stopped: bool,
}

impl<E> CallQueue<E> {
    /// Queues `request`: whether a job must be handed to the placement to
    /// run it. Once stopped, it drops `request`, which fails its call.
    pub(super) fn queue(&self, request: Request<E>) -> bool {
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
    pub(super) fn cancel_then_drop(&self, dropped: Option<mpsc::Sender<()>>) -> bool {
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
pub(super) struct BatchCopy<I> {
    pub(super) shots: ShotSet,
    pub(super) instructions: Vec<I>,
}

impl<I> BatchCopy<I> {
    pub(super) fn as_ref(&self) -> InstructionBatchRef<'_, I> {
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
pub(super) struct Drain<E> {
    pub(super) calls: SharedCallQueue<E>,
    pub(super) ended: bool,
}

impl<E> Drain<E> {
    pub(super) fn run(mut self) {
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
pub(super) struct SharedCallQueue<E>(Option<Arc<CallQueue<E>>>);

impl<E> SharedCallQueue<E> {
    pub(super) fn new(queue: CallQueue<E>) -> Self {
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
pub(super) struct QueuedCall {
    pub(super) lifecycle: ExecutorLifecycleHandle,
    pub(super) sender: Option<ReplySender<ShotOutcome>>,
    /// Whether the executor's `call` is under way: tells a panic of the
    /// executor's, as the call runs, from one of the placement's, which
    /// drops the call unrun, and from a call that returned on a thread
    /// already unwinding (a placement that runs the job inline, in a
    /// `Drop` during a panic), which did not fail.
    pub(super) running: bool,
}

impl QueuedCall {
    /// Runs `batch` on `executor`, unless an earlier call failed, and
    /// hands its events to the caller as they come.
    pub(super) fn run<I, E>(mut self, executor: &mut E, batch: InstructionBatchRef<'_, I>)
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
pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
