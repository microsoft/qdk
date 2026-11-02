// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The life of an executor whose calls share state until it is closed:
//! open, failed from its first failed call on, and closed
//! ([`ExecutorLifecycle`]), and the reply of a call bound to it
//! ([`LifecycleBoundReply`]): the rules of the executor's contract
//! ([`Executor`]) on failures and close, written once, for executors
//! written by hand and for those the facility makes
//! ([`executor_fn`](fn@crate::executor_fn)).

use std::collections::BTreeMap;
use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::Waker;

#[cfg(doc)]
use crate::Executor;
use crate::absorb::end_each_raising_first;
use crate::{Rejected, Reply, ReplyStatus};

/// The first failed call of an executor and why, shared by the executor and
/// the replies of its calls. Every call reads `call` once (one atomic load).
/// Every handle made for a reply takes its entry (one atomic add). Every
/// poll of a bound reply (a [`LifecycleBoundReply`], or a
/// [`ShotReplies`](crate::ShotReplies) made once some reply kept a part
/// pending) reads `call` once, and a second time unless the first read ends
/// the reply. The `reason` lock is taken by each failure, and by each check
/// that finds an earlier call failed (`before`). A reply left pending takes
/// the `waiting` lock once per new waker, and once when it ends or is
/// dropped; each new earliest failure takes it once, to wake the later
/// calls' replies.
#[derive(Debug)]
struct FirstFailure {
    /// The earliest failed call, or `u64::MAX` while none has failed.
    call: AtomicU64,
    reason: Mutex<Option<(u64, String)>>,
    /// The entry of the next handle made: its own place in `waiting`.
    next_entry: AtomicU64,
    /// The wakers of the replies left pending, by call, then by entry: a
    /// failure wakes those of later calls, which it ends. An executor may
    /// bind several replies for one call (the parts of a call sent to two
    /// devices), so each reply has its own entry: one part's waker neither
    /// replaces nor removes another's. Each registration and end costs
    /// O(log n) for n replies left pending, and a failure O(log n) and one
    /// wake per reply it ends. Measured once with a throwaway bench (release,
    /// one pending poll, then an ending poll for half the replies and a
    /// failure for the others): about 85 ns per first poll and 70 ns per
    /// ending poll at n = 16k, against 3.1 and 1.5 µs with a `Vec` searched
    /// linearly; the same as that `Vec` at n = 2 to 64.
    waiting: Mutex<BTreeMap<(u64, u64), Waker>>,
    /// The failure of the executor this one is a part of, which each of
    /// its failures sets too ([`PartsFailure`]).
    whole: Option<PartsFailure>,
}

impl Default for FirstFailure {
    fn default() -> Self {
        Self {
            call: AtomicU64::new(u64::MAX),
            reason: Mutex::new(None),
            next_entry: AtomicU64::new(0),
            waiting: Mutex::new(BTreeMap::new()),
            whole: None,
        }
    }
}

impl FirstFailure {
    fn record(&self, call: u64, reason: &str) {
        // First: the whole executor's calls read it before they reach this
        // part.
        if let Some(whole) = &self.whole {
            whole.record(call, reason);
        }
        // A poisoned lock still holds a valid record: it is replaced whole.
        let mut first = self.reason.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*first, Some((earliest, _)) if earliest <= call) {
            return;
        }
        *first = Some((call, reason.to_owned()));
        // Under the lock: two replies polled on two threads may both fail
        // before either sees the other's failure, and the later call's must
        // not raise `call` past the earlier one's. Stored after the unlock,
        // call 3's record could unlock, call 1's record and store 1, then
        // call 3's store 3: `before`, which reads `call` first, would then
        // hide call 1's failure from call 2. No test pins this: the window
        // holds no code a test can run in.
        self.call.store(call, Ordering::Release);
        drop(first);
        // Taken after `call` is stored, so that a reply that registers
        // later sees the failure (`ExecutorLifecycleHandle::wake_on_failure`).
        // The race tests of `src/tests/lifecycle.rs` pin this order.
        let mut waiting = self.waiting.lock().unwrap_or_else(PoisonError::into_inner);
        // Every entry of every later call; this call's own entries stay.
        let ended = waiting.split_off(&(call + 1, 0));
        drop(waiting);
        // Woken after the unlock: a wake may drop a reply of this executor,
        // whose handle then takes the lock. Each in a catch of its own, so
        // that a waker that panics leaves none of the others unwoken; the
        // first panic is raised after, in the caller that recorded the
        // failure.
        end_each_raising_first(None, ended.into_values(), Waker::wake);
    }

    /// The reason of the earliest failed call, if it is earlier than `call`.
    #[inline]
    fn before(&self, call: u64) -> Option<String> {
        if self.call.load(Ordering::Acquire) >= call {
            return None;
        }
        let first = self.reason.lock().unwrap_or_else(PoisonError::into_inner);
        first.as_ref().map(|(_, reason)| reason.clone())
    }

    /// The earliest failed call and its reason, once a call failed.
    fn first(&self) -> Option<(u64, String)> {
        let first = self.reason.lock().unwrap_or_else(PoisonError::into_inner);
        first.clone()
    }
}

/// The first failure of any part of an executor made of parts that fail on
/// threads of their own, such as the shots' placements of
/// `.run_on(placement).shots(OnePerShot)`: each part records its
/// failures in it as they happen ([`ExecutorLifecycle::part_of`]), and the
/// executor reads it at the start of each call, so that a call after a
/// part's failure is refused before anything runs, whatever parts it names.
/// It also holds the executor's call being given to its parts
/// ([`begin`](Self::begin)), by which each part numbers its own calls, so
/// that a part's failure names the executor's call it failed in. Of several
/// parts' failures, the earliest call's is kept, with its reason, whatever
/// order they happen in: the earliest failed call decides, as for the
/// executor's own failures ([`ExecutorLifecycle::fail`]), both the reason
/// later calls are refused with and, at close, which states are left out as
/// made for a failed call ([`ShotStates`](crate::ShotStates)). A read while
/// no part failed is one atomic load; a failure, and each read after one,
/// takes a lock, and a new earliest failure costs a copy of its reason.
#[derive(Clone, Debug, Default)]
pub(crate) struct PartsFailure(Arc<Parts>);

/// What [`PartsFailure`] shares between an executor and its parts.
#[derive(Debug)]
struct Parts {
    /// The earliest of the executor's calls a part failed in, with its
    /// reason, once one did: kept together, so that the reason is that
    /// call's whatever order the parts fail in.
    first: Mutex<Option<(u64, String)>>,
    /// The call of `first`, or `u64::MAX` while none has failed: read
    /// without the lock.
    earliest: AtomicU64,
    /// The executor's call being given to its parts.
    call: AtomicU64,
}

impl Default for Parts {
    fn default() -> Self {
        Self {
            first: Mutex::new(None),
            earliest: AtomicU64::new(u64::MAX),
            call: AtomicU64::new(0),
        }
    }
}

impl PartsFailure {
    /// Records that a part failed in the executor's call `call`, for
    /// `reason`; kept unless a part failed in that call or an earlier one
    /// already.
    fn record(&self, call: u64, reason: &str) {
        // A poisoned lock still holds a valid record: it is replaced whole.
        let mut first = self.0.first.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*first, Some((earliest, _)) if earliest <= call) {
            return;
        }
        *first = Some((call, reason.to_owned()));
        // Under the lock, as in `FirstFailure::record`: stored after the
        // unlock, a later call's store could raise `earliest` past an
        // earlier call's.
        self.0.earliest.store(call, Ordering::Release);
    }

    /// The earliest of the executor's calls a part failed in, and why,
    /// once one did.
    #[inline]
    pub(crate) fn first(&self) -> Option<(u64, String)> {
        if self.0.earliest.load(Ordering::Acquire) == u64::MAX {
            return None;
        }
        let first = self.0.first.lock().unwrap_or_else(PoisonError::into_inner);
        first.clone()
    }

    /// The executor gives its parts its call `call`: each part admitted from
    /// now on numbers its call so. The parts are given a call on the
    /// executor's thread, inside its `call`.
    #[inline]
    pub(crate) fn begin(&self, call: u64) {
        self.0.call.store(call, Ordering::Relaxed);
    }

    /// The earliest of the executor's calls a part failed in, once one did.
    pub(crate) fn earliest_call(&self) -> Option<u64> {
        let call = self.0.earliest.load(Ordering::Acquire);
        (call != u64::MAX).then_some(call)
    }
}

/// The life of an executor whose calls share state until it is closed, such
/// as one that keeps its shots' states between calls, or one state for all
/// its shots: open; failed from its first failed call on; closed. It keeps
/// the rules of the [`Executor`] contract on failures and close, for an
/// executor written by hand; one made with
/// [`executor_fn`](fn@crate::executor_fn) keeps them already.
///
/// The executor admits each call ([`admit`](Self::admit)) before it checks
/// the call's own faults: a call to a failed executor is refused with
/// [`Rejected::EarlierFailure`], and to a closed one with
/// [`Rejected::Closed`], which is checked first. A failure that the executor
/// learns of before `call` returns, it records with [`fail`](Self::fail); a
/// reply that may fail after `call` returned, it binds to the lifecycle
/// ([`bind`](Self::bind)), and the reply records its own failure when
/// polled. [`close`](Self::close) ends its life.
///
/// A failure is final because the shots it hit are left part-way through their
/// instructions, or through a batch that ran on some of its shots only, so a
/// later batch on them would run on a state the caller's program never made. It
/// is final for the whole executor, not only for the failed call's shots,
/// though the other shots' states are sound. Narrowing a failure to its
/// shots is planned (`docs/planned.md`), for a long sampling run that keeps
/// its other shots after one shot's device fails: the lifecycle would keep
/// the set of failed shots, have `admit` take the call's shots, and refuse
/// only a call that names a failed one. A failure drops none of the
/// executor's states: the lifecycle refuses later calls, and wakes the
/// pending replies bound for calls after the failed one, each of which
/// ends with the failure at its next poll. An executor built on it may
/// keep its states until `close`, so that `close` reports them, as one
/// made with [`executor_fn`](fn@crate::executor_fn) keeps those made before
/// the failed call (a failed call reports no shot of it). Keeping
/// them is the crate's choice, not the protocol's: dropping them at the
/// failure is planned, for a case that needs their memory back sooner.
///
/// [`fail`](Self::fail) and [`bind`](Self::bind) concern the call admitted
/// last; before the first [`admit`](Self::admit) there is none, and what they
/// do then is unspecified.
///
/// # Example
///
/// An executor whose shots share one counter: a `true` counts, a `false` is
/// where its device stops, after the instructions before it ran. Once its
/// device stopped, it refuses every call.
///
/// ```
/// use executor_protocol::{
///     Close, Executor, ExecutorLifecycle, ImmediateReply, InstructionBatch,
///     InstructionBatchRef, Rejected, ShotOutcome, ShotSet, wait,
/// };
///
/// #[derive(Default)]
/// struct Counter {
///     count: usize,
///     lifecycle: ExecutorLifecycle,
/// }
///
/// impl Executor<InstructionBatch<bool>> for Counter {
///     type Reply = ImmediateReply<ShotOutcome>;
///
///     fn call(
///         &mut self,
///         batch: InstructionBatchRef<'_, bool>,
///     ) -> Result<Self::Reply, Rejected> {
///         self.lifecycle.admit()?;
///         for &counts in batch.instructions {
///             if !counts {
///                 // The count moved: a later call would run on a count its
///                 // caller never made.
///                 self.lifecycle.fail("the device stopped");
///                 return Ok(ImmediateReply::failed(
///                     Vec::new(),
///                     "the device stopped".into(),
///                 ));
///             }
///             self.count += batch.shots.len();
///         }
///         Ok(ImmediateReply::done(Vec::new()))
///     }
/// }
///
/// impl Close for Counter {
///     type Report = ();
///
///     fn close(&mut self) {
///         self.lifecycle.close();
///     }
/// }
///
/// let mut counter = Counter::default();
/// let shots = ShotSet::one(0);
/// let reply = counter
///     .call(InstructionBatchRef {
///         shots: &shots,
///         instructions: &[true, false],
///     })
///     .expect("an open counter runs a call");
/// assert_eq!(
///     wait(reply, &mut Vec::new()),
///     Err("the device stopped".into())
/// );
/// let next = InstructionBatchRef {
///     shots: &shots,
///     instructions: &[true],
/// };
/// assert_eq!(
///     counter.call(next).err(),
///     Some(Rejected::EarlierFailure("the device stopped".into()))
/// );
/// ```
#[derive(Debug, Default)]
pub struct ExecutorLifecycle {
    failure: Arc<FirstFailure>,
    calls: u64,
    closed: bool,
}

impl ExecutorLifecycle {
    /// The lifecycle of a part of an executor: each of its failures is
    /// recorded in `whole` too, as it happens, and it numbers its calls by
    /// the executor's ([`PartsFailure::begin`]).
    pub(crate) fn part_of(whole: PartsFailure) -> Self {
        // Every field named: `..Self::default()` would allocate a failure
        // only to drop it, once per part (a shot's first call).
        Self {
            failure: Arc::new(FirstFailure {
                whole: Some(whole),
                ..FirstFailure::default()
            }),
            calls: 0,
            closed: false,
        }
    }

    /// Admits a call. Checked before the call's own faults, so that a closed
    /// or failed executor says so whatever the call.
    ///
    /// # Errors
    ///
    /// [`Rejected::Closed`] after [`ExecutorLifecycle::close`]; otherwise
    /// [`Rejected::EarlierFailure`], with the failure's reason, after a call
    /// failed.
    #[inline]
    pub fn admit(&mut self) -> Result<(), Rejected> {
        if self.closed {
            return Err(Rejected::Closed);
        }
        if let Some(reason) = self.failure.before(u64::MAX) {
            return Err(Rejected::EarlierFailure(reason));
        }
        self.calls = match &self.failure.whole {
            Some(whole) => whole.0.call.load(Ordering::Relaxed),
            None => self.calls + 1,
        };
        Ok(())
    }

    /// The lifecycle of an executor that wraps the one this lifecycle is
    /// of, such as the executor placed by `.run_on(..)`: it continues its
    /// refusals (closed, and the first failure, of the same call), and
    /// numbers its calls on from the same count. A wrapper continues what
    /// it wraps, failures included, as iterator adapters and Tower's
    /// `Buffer` do: a wrapper never forgets a refusal. Numbering on from the
    /// same count is not observable today; it keeps the numbers of both
    /// lifecycles comparable, should a reply ever cross a continuation.
    pub(crate) fn continued(&self) -> Self {
        let continued = Self {
            failure: Arc::default(),
            calls: self.calls,
            closed: self.closed,
        };
        if let Some((call, reason)) = self.failure.first() {
            continued.failure.record(call, &reason);
        }
        continued
    }

    /// The number of the call admitted last, by which its failure is
    /// recorded; 0 before the first.
    pub(crate) fn call(&self) -> u64 {
        self.calls
    }

    /// The earliest failed call, by number, once a call failed.
    pub(crate) fn first_failed_call(&self) -> Option<u64> {
        let call = self.failure.call.load(Ordering::Acquire);
        (call != u64::MAX).then_some(call)
    }

    /// Records that the call admitted last failed, before its `call`
    /// returns: every later call is refused. Of one call's failures, the
    /// first recorded is kept: one recorded here wins over the call's bound
    /// reply failing later; when several calls failed, the earliest one's
    /// reason is the executor's.
    ///
    /// # Panics
    ///
    /// When the waker of a later call's pending reply panics as the failure
    /// wakes it: the other replies' wakers are woken first, then the first
    /// such panic is raised here, unless the thread is unwinding.
    pub fn fail(&self, reason: &str) {
        self.failure.record(self.calls, reason);
    }

    /// `rejected`, the refusal by an executor downstream of the call this
    /// executor admitted last, to pass up as this executor's: a refusal for
    /// an earlier failure there ([`Rejected::EarlierFailure`]) fails this
    /// executor too, with its reason, so that it refuses every later call,
    /// even one it would answer without calling downstream. Any other
    /// refusal leaves it open. The rule of every executor of the crate over
    /// others, written once: [`adapter_fn`](fn@crate::adapter_fn)'s, the
    /// executor composed with an adapter written by hand, and
    /// [`ShotStates`](crate::ShotStates) over its shots' executors. A
    /// placement fails on any refusal from the executor it places, a
    /// stronger rule, since it accepted the call before that executor saw
    /// it.
    pub(crate) fn pass_up_refusal(&self, rejected: Rejected) -> Rejected {
        if let Rejected::EarlierFailure(reason) = &rejected {
            self.fail(reason);
        }
        rejected
    }

    /// The handle of the call admitted last on this lifecycle, for its reply,
    /// or for one of its parts: a reply that learns of a failure only when
    /// polled records it through the handle, and learns there of an earlier
    /// call's failure.
    #[must_use]
    pub(crate) fn handle(&self) -> ExecutorLifecycleHandle {
        ExecutorLifecycleHandle {
            failure: Arc::clone(&self.failure),
            call: self.calls,
            entry: self.failure.next_entry.fetch_add(1, Ordering::Relaxed),
            waker: None,
        }
    }

    /// `reply`, the reply of the call admitted last, or one of its parts,
    /// bound to this lifecycle, for an executor that learns of a failure
    /// only when the reply is polled: see [`LifecycleBoundReply`]. An
    /// executor may bind several parts of one call, such as the shots it
    /// sends to two devices, then gather them into the call's reply: each
    /// part keeps the bound reply's rules on its own.
    #[must_use]
    pub fn bind<R: Reply>(&self, reply: R) -> LifecycleBoundReply<R> {
        LifecycleBoundReply {
            reply,
            lifecycle: self.handle(),
        }
    }

    /// Ends the executor's life: later calls are refused with
    /// [`Rejected::Closed`]. Replies still pending are left alone: each ends
    /// as if the executor were open, as the reply it binds does unless an
    /// earlier call failed. Cancelling them at close is planned (see
    /// [`Reply`]).
    pub fn close(&mut self) {
        self.closed = true;
    }
}

/// Why an executor whose `call` panicked refuses every later call.
pub(crate) const PANICKED: &str = "the executor panicked during the call";

/// Runs `call` on `executor`; should it panic, fails the executor's
/// lifecycle, which `lifecycle` gives, with [`PANICKED`], then lets the
/// panic go on to the caller, as a `Mutex` held by a panicking thread is
/// poisoned: the panic may have left a state part-way through a batch,
/// which a later call would run on, so every later call is refused. The
/// rule of every executor of the crate that keeps a lifecycle: each form
/// of [`executor_fn`](fn@crate::executor_fn) and of
/// [`adapter_fn`](fn@crate::adapter_fn), placed or not, the executor
/// composed with an adapter written by hand, and the shots of the placed
/// per-shot executor, through [`ShotStates`](crate::ShotStates). Of one
/// call's failures the first recorded is kept, so a failure that the call
/// recorded as it unwound stays its reason.
pub(crate) fn failing_on_panic<E, U>(
    executor: &mut E,
    lifecycle: impl FnOnce(&E) -> &ExecutorLifecycle,
    call: impl FnOnce(&mut E) -> U,
) -> U {
    // Caught, not guarded by a value dropped as the call unwinds: such a
    // value would borrow the lifecycle across `call`, which borrows the
    // whole executor. A catch costs nothing until something panics.
    match panic::catch_unwind(AssertUnwindSafe(|| call(executor))) {
        Ok(returned) => returned,
        Err(panic) => {
            lifecycle(executor).fail(PANICKED);
            panic::resume_unwind(panic)
        }
    }
}

/// A reply's handle on its executor's [`ExecutorLifecycle`]: the reply's
/// call, by number, its own entry among the replies to wake, and the
/// executor's first failure, shared. Through it a reply records its own
/// failure, learns of an earlier one, and has an earlier one wake its caller
/// while the reply is pending. The call's number is the
/// executor's own count, which only orders its calls: it counts every
/// admitted call, also those refused afterwards for their own faults. A
/// [`CallId`](crate::CallId) (planned, for a link) is another number: it
/// names a call so that a resent call is recognized as the same one.
#[derive(Debug)]
pub(crate) struct ExecutorLifecycleHandle {
    failure: Arc<FirstFailure>,
    call: u64,
    /// This handle's own place among the replies to wake, apart from the
    /// other replies bound for its call.
    entry: u64,
    /// The waker registered last, so that an unchanged one is not
    /// registered again, which would only cost a lock and two clones of the
    /// waker.
    waker: Option<Waker>,
}

impl ExecutorLifecycleHandle {
    /// Records that this call failed. Of one call's failures, the first
    /// recorded is kept, and of several calls', the earliest call's: a
    /// caller that polls several replies may see their failures out of
    /// order.
    pub(crate) fn fail(&self, reason: &str) {
        self.failure.record(self.call, reason);
    }

    /// Why this call must end without more events, as
    /// [`Rejected::EarlierFailure`] displays it: an earlier call failed. A
    /// caller that keeps several calls in flight learns of a failure when it
    /// polls, so this call may have been admitted before the failure was
    /// known.
    #[must_use]
    #[inline]
    pub(crate) fn earlier_failure(&self) -> Option<String> {
        self.failure
            .before(self.call)
            .map(|reason| Rejected::EarlierFailure(reason).to_string())
    }

    /// Polls this call's reply with `poll`, by the rules of a reply bound to
    /// the lifecycle ([`LifecycleBoundReply`]), which `poll` need not know:
    /// it polls the reply into `out`, records the reply's own failure
    /// through the handle it is given, and returns the reply's status. Once
    /// an earlier call failed, `poll` is not run, and the reply ends with
    /// that failure; while the reply is pending, an earlier call's failure
    /// wakes `waker`; when it comes during the poll that ends the reply, the
    /// reply ends with it, and `out` loses the events of that poll.
    pub(crate) fn poll_bound<T>(
        &mut self,
        out: &mut Vec<T>,
        waker: &Waker,
        poll: impl FnOnce(&Self, &mut Vec<T>, &Waker) -> ReplyStatus,
    ) -> ReplyStatus {
        if let Some(reason) = self.earlier_failure() {
            self.stop_waking();
            return ReplyStatus::Failed(reason);
        }
        let held = out.len();
        let status = poll(self, out, waker);
        if matches!(status, ReplyStatus::Pending) {
            if self.wake_on_failure(waker) {
                // An earlier call failed since the check above: the next poll
                // ends the reply (a known issue at `LifecycleBoundReply`).
                waker.wake_by_ref();
            }
            return ReplyStatus::Pending;
        }
        self.stop_waking();
        // An earlier call may have failed since the check above, and this
        // poll ends the reply: there is no next poll to end it. Its failure
        // wins over the reply's own, as at the check above.
        if let Some(reason) = self.earlier_failure() {
            out.truncate(held);
            return ReplyStatus::Failed(reason);
        }
        status
    }

    /// Has an earlier call's failure wake `waker`, for a reply left pending:
    /// the failure ends the reply, though the reply has no news. Whether
    /// such a failure came already: it may not wake `waker`, so the reply
    /// wakes its caller, whose next poll ends it.
    fn wake_on_failure(&mut self, waker: &Waker) -> bool {
        // Saves a lock and two waker clones only: a re-listed waker may be
        // woken again (allowed).
        if self.waker.as_ref().is_none_or(|w| !w.will_wake(waker)) {
            // Cloned before the lock: a clone runs the waker's own code.
            let listed = waker.clone();
            let mut waiting = (self.failure.waiting.lock()).unwrap_or_else(PoisonError::into_inner);
            let replaced = waiting.insert((self.call, self.entry), listed);
            drop(waiting);
            // Both clones of the old waker, the list's and `self.waker`, are
            // dropped after the unlock: a waker that holds another reply of
            // this executor drops it, whose own handle then takes the lock.
            drop(replaced);
            self.waker = Some(waker.clone());
        }
        // Read after registering: a failure recorded before then is seen
        // here, and one recorded after wakes `waker`. Strictly earlier: were
        // this call's own failure counted (recorded in its `call` while its
        // reply still runs), every poll would wake its caller, a busy loop
        // under `wait` (`LifecycleBoundReply`).
        self.failure.call.load(Ordering::Acquire) < self.call
    }

    /// The reply ended: no failure wakes its caller any more.
    fn stop_waking(&mut self) {
        let Some(registered) = self.waker.take() else {
            return;
        };
        let mut waiting = (self.failure.waiting.lock()).unwrap_or_else(PoisonError::into_inner);
        let removed = waiting.remove(&(self.call, self.entry));
        drop(waiting);
        // Both clones dropped after the unlock, as in `wake_on_failure`.
        drop((registered, removed));
    }
}

impl Drop for ExecutorLifecycleHandle {
    fn drop(&mut self) {
        self.stop_waking();
    }
}

/// A reply `R` bound to its executor's [`ExecutorLifecycle`]
/// ([`ExecutorLifecycle::bind`]), for an executor that learns of a failure
/// only when the reply is polled (a worker thread, a device, a link): the
/// failure is then final for the executor all the same.
///
/// It hands over the events of the reply it binds, and ends as that reply
/// does, except that:
///
/// - when that reply fails, the executor fails at that poll, and refuses
///   every later call ([`Rejected::EarlierFailure`]), even while the replies
///   of other calls are pending; and so when that reply ends
///   [`ReplyStatus::Cancelled`], as a failure: the executor that was to
///   run it was dropped, so a later call has nothing to run on;
/// - once an earlier call of the executor failed, it ends at its next poll
///   with that failure, displayed as [`Rejected::EarlierFailure`] is
///   (`an earlier call failed: …`), even if it failed itself, and hands over
///   no more events, since they ran on the state that failure left; if that
///   call failed during the poll that ends the reply, it ends at that poll,
///   without the events of that poll;
/// - while it is pending, an earlier call's failure wakes it, though the
///   reply it binds has no news, and its next poll ends it, as above: the
///   failure wakes the waker of its last poll, or, when it comes during a
///   poll that leaves the reply pending, the reply wakes that poll's waker
///   itself, so no wake is lost (that poll still hands over its own
///   events, a known issue noted at this type's source). It may be woken
///   more than once, as Rust's wakers allow, when the failure races a poll:
///   the failure and the reply may both wake it, and a waker that poll
///   replaced may be woken too.
///   A waker that panics as a failure wakes it leaves the other replies'
///   wakers woken; the first such panic comes out of the call or the poll
///   that recorded the failure ([`ExecutorLifecycle::fail`], or the failing
///   reply's poll), not out of a poll of this reply.
///
/// An executor may bind several replies for one call, the parts it then
/// gathers into the call's reply (the shots it sends to two devices, say):
/// each keeps these rules on its own, so an earlier call's failure wakes
/// every part still pending, whichever parts ended before.
///
/// A later call's failure does not concern it: it ends as the reply it binds
/// does. Nor does a failure of its own call, recorded with
/// [`ExecutorLifecycle::fail`] before `call` returned, by another reply
/// bound for its call, or by this reply itself as it ends: it ends as the
/// reply it binds does, and that failure does not wake the waker of its last
/// poll, since its caller would otherwise poll it in a busy loop while it
/// waits, or poll it again once it ended. (A task that waits on a later
/// call's reply too, with the same waker, is still woken, through that
/// reply, by the rule above: for that reply, the failure is an earlier
/// call's.)
/// The reply that gathers a call's parts is then the one to end the call
/// when it sees a part fail.
// Known issue, open (blocks neither): a reply that learns of an earlier
// failure as it is left pending returns `Pending` and wakes its caller, and
// ends at the next poll: one poll more than needed. The poll that learns of
// the failure also hands over its events, though they ran on the state the
// failure left, while the poll that ends the reply at an earlier failure
// drops its own. Ending at once, without those events, is proposed.
//
// Known issue, open (blocks neither): a later reply's waker that panics
// as this reply's failure wakes it raises its panic out of this reply's
// poll, which has taken the failure from the reply it binds, so a later
// poll of this reply waits for an end that was already given. The waker's
// panic is the caller's bug; ending the reply at its next poll would need
// the status kept across the panic.
pub struct LifecycleBoundReply<R> {
    reply: R,
    lifecycle: ExecutorLifecycleHandle,
}

impl<R> fmt::Debug for LifecycleBoundReply<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LifecycleBoundReply")
            .field("lifecycle", &self.lifecycle)
            .finish_non_exhaustive()
    }
}

impl<R: Reply> Reply for LifecycleBoundReply<R> {
    type Item = R::Item;

    fn poll_events(&mut self, out: &mut Vec<R::Item>, waker: &Waker) -> ReplyStatus {
        let reply = &mut self.reply;
        self.lifecycle
            .poll_bound(out, waker, |lifecycle, out, waker| {
                let status = reply.poll_events(out, waker);
                if let Some(reason) = status.failure() {
                    lifecycle.fail(reason);
                }
                status
            })
    }
}
