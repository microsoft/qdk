// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Replies: how the events of a call reach the caller.
//!
//! - [`asynchronous`]: the async ways to take a reply: a whole reply as a
//!   `Future`, or its events as a `Stream`.

pub(crate) mod asynchronous;

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::Thread;

use crate::absorb::absorb;

/// How a reply stands after a poll.
///
/// A failure is the executor's account of a run that started and could not
/// finish, so its reason stays text: drivers report it, they do not match
/// it. A call that starts no work is refused with a typed
/// [`Rejected`](crate::Rejected) instead, and a call abandoned before it
/// ran ends [`Cancelled`](Self::Cancelled), typed too, so that a driver
/// can tell it from a failure.
///
/// Non-exhaustive, so that a status can be added without breaking callers:
/// a match outside this crate has an arm for the statuses it does not know.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplyStatus {
    /// More events may come.
    Pending,
    /// Every event has been delivered.
    Done,
    /// The run stopped; the events delivered before it stand.
    Failed(String),
    /// The call was cancelled before it ran to its end: it, or some of its
    /// parts (a call on several shots, each on an executor of its own),
    /// never started, because an executor that was to run them was
    /// dropped: this executor, or one below it whose reply it hands on. The
    /// events delivered before it stand. Today a dropped
    /// [`PlacedExecutor`](crate::PlacedExecutor) cancels its calls not yet
    /// started; a call that was running is not stopped, and ends as it
    /// would have. Where a call's parts end differently, a failed part
    /// outranks a cancelled one: the call ends `Failed`.
    ///
    /// A variant, not a reason of [`Failed`](Self::Failed): any executor
    /// could fail with the same text, and the compiler cannot check a match
    /// on a string. The state of the art types it too: tokio's
    /// `JoinError::is_cancelled`, Java's `CancellationException`, Python's
    /// `CancelledError`.
    Cancelled,
}

/// The reason [`wait`] and the async ways to take a reply give for a
/// cancelled call, and the failure an executor records for a reply of its
/// own that ends cancelled: where only text fits. It names no cause, since
/// the executor dropped may be this one or one below it.
pub(crate) const CANCELLATION: &str = "the call was cancelled before it ran to its end";

impl ReplyStatus {
    /// How the reply ended, as [`wait`] gives it: `Ok` when done, else the
    /// failure's reason, or [`CANCELLATION`] for a cancelled call; `None`
    /// while it is pending.
    pub(crate) fn into_end(self) -> Option<Result<(), String>> {
        match self {
            Self::Pending => None,
            Self::Done => Some(Ok(())),
            Self::Failed(reason) => Some(Err(reason)),
            Self::Cancelled => Some(Err(CANCELLATION.to_owned())),
        }
    }

    /// The failure an executor records when a reply of its own ends so: a
    /// failure's reason, or [`CANCELLATION`] for a cancelled call, which
    /// fails the executor as a failure does unless a rule says otherwise;
    /// `None` while it is pending, and once it is done.
    pub(crate) fn failure(&self) -> Option<&str> {
        match self {
            Self::Pending | Self::Done => None,
            Self::Failed(reason) => Some(reason),
            Self::Cancelled => Some(CANCELLATION),
        }
    }
}

/// The events of one call, however they arrive: all at once when the work
/// ran inside the call, or piece by piece when it runs elsewhere.
///
/// One trait serves every way of making a reply and every way of taking it,
/// so the executor and its caller choose independently, and a driver written
/// once runs on every chain:
///
/// - The executor chooses how the events are made: [`ImmediateReply`] when the
///   work ran inside `call`; [`reply_channel`] when it runs elsewhere (a worker
///   thread, a device, a remote link), whose [`ReplySender`] pushes events into
///   the [`IncrementalReply`] as they are ready; [`relay`] passes an
///   inner reply's events up from a worker; [`MappedReply`] converts them on
///   the way.
/// - The caller (a driver, or an adapter) chooses how to take them, and
///   whether a thread waits meanwhile:
///
/// | The caller                                  | Takes the reply with                                                             | A thread blocked?  |
/// | ------------------------------------------- | -------------------------------------------------------------------------------- | ------------------ |
/// | a synchronous driver                        | [`wait`]: the whole reply, the thread parked between polls                       | yes, by its choice |
/// | a loop that does other work                 | [`poll_events`](Reply::poll_events) with its own waker, or [`Waker::noop`]       | no                 |
/// | an async driver, the whole reply            | [`wait_async`](crate::wait_async): a `Future`                                    | no                 |
/// | an async driver, the events as they come    | [`async_events`](crate::async_events): a `Stream` of event batches (streaming)   | no                 |
///
/// The choice stays the caller's only while every executor downstream returns
/// from `call` without waiting for an inner reply: an adapter that calls
/// [`wait`] inside its own `call`, because it needs a result before it can go
/// on, blocks its caller's thread for that time, whichever way the caller
/// then takes the reply.
///
/// A reply is polled and never blocks. After a poll that returns
/// [`ReplyStatus::Pending`], it keeps the waker of that poll (the latest one
/// replaces the others) and wakes it once a new poll could deliver more
/// events or end the reply. The wake is needed because without it a caller
/// that wants the events could only poll again and again: the executor knows
/// when its events are ready, the caller knows when it wants them, and the
/// waker joins the two.
///
/// So a caller that gives a waker never needs to poll in a loop: it polls,
/// and on `Pending` sleeps (its thread parks, or its task yields) until the
/// wake; then it polls again, without blocking, and takes every event made
/// since its last poll. One wake can stand for many events: events that
/// arrive while the caller has not polled yet are taken together, at its
/// next poll. The crate's `docs/replies.md`, §The waking model, walks
/// through the cycle.
///
/// The caller does not poll a reply again after it ends (`Done`, `Failed`
/// or `Cancelled`), because its end is delivered once. What a reply does
/// when polled again is not promised; as written today, an
/// [`IncrementalReply`] returns `Pending` and never wakes, so a caller
/// waiting on it would hang, and an [`ImmediateReply`] that failed returns
/// `Done`. [`wait`] returns at the end; [`wait_async`](crate::wait_async)
/// and [`async_events`](crate::async_events) keep this rule for their own
/// callers.
///
/// Dropping a reply, or a future or stream over it, does not stop its work:
/// the executor still pushes its events and ends it, and nobody takes them.
/// Dropping an executor may stop work: a dropped
/// [`PlacedExecutor`](crate::PlacedExecutor) cancels its calls not yet
/// started, whose replies end [`ReplyStatus::Cancelled`]. Cancellation, at an
/// executor's `close`, is planned, with no milestone yet, for a caller that
/// stops a run before its batches end (a user who interrupts a long run
/// from Python).
pub trait Reply {
    /// The event this reply delivers.
    type Item;

    /// Moves the events available now to the end of `out`, leaving what `out`
    /// already held as it was. Never blocks.
    fn poll_events(&mut self, out: &mut Vec<Self::Item>, waker: &Waker) -> ReplyStatus;
}

/// A reply whose events, of type `T`, all exist when `call` returns: work done
/// synchronously inside the call. Done at its first poll, so it never keeps the
/// waker.
///
/// An executor builds it whole ([`done`](Self::done),
/// [`failed`](Self::failed)), or event by event from
/// [`default`](Self::default) with [`push`](Self::push). Built event by
/// event, a reply of at most one event, such as the outcome of the
/// measurement that ends a batch, allocates no memory.
#[derive(Clone)]
pub struct ImmediateReply<T> {
    items: Events<T>,
    failure: Option<String>,
}

/// The events of an [`ImmediateReply`]: held in the reply while there is at
/// most one, in a `Vec` beyond.
#[derive(Clone)]
enum Events<T> {
    One(Option<T>),
    Many(Vec<T>),
}

impl<T> ImmediateReply<T> {
    /// All of `items`, then [`ReplyStatus::Done`].
    #[must_use]
    pub fn done(items: Vec<T>) -> Self {
        Self {
            items: Events::Many(items),
            failure: None,
        }
    }

    /// All of `items`, then [`ReplyStatus::Failed`] with `reason`.
    #[must_use]
    pub fn failed(items: Vec<T>, reason: String) -> Self {
        Self {
            items: Events::Many(items),
            failure: Some(reason),
        }
    }

    /// Adds `item` after the events it holds.
    pub fn push(&mut self, item: T) {
        match &mut self.items {
            Events::One(none @ None) => *none = Some(item),
            Events::One(first @ Some(_)) => {
                self.items = Events::Many(first.take().into_iter().chain([item]).collect());
            }
            Events::Many(items) => items.push(item),
        }
    }

    /// Ends it with [`ReplyStatus::Failed`] and `reason`, after the events
    /// it holds.
    pub(crate) fn fail(&mut self, reason: String) {
        self.failure = Some(reason);
    }

    /// The events it holds, in order.
    fn items(&self) -> &[T] {
        match &self.items {
            Events::One(item) => item.as_slice(),
            Events::Many(items) => items,
        }
    }
}

impl<T> Default for ImmediateReply<T> {
    /// No events, then [`ReplyStatus::Done`]: a reply to push events into.
    fn default() -> Self {
        Self {
            items: Events::One(None),
            failure: None,
        }
    }
}

impl<T: PartialEq> PartialEq for ImmediateReply<T> {
    /// The same events and the same end, however they are held.
    fn eq(&self, other: &Self) -> bool {
        self.items() == other.items() && self.failure == other.failure
    }
}

impl<T: fmt::Debug> fmt::Debug for ImmediateReply<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImmediateReply")
            .field("items", &self.items())
            .field("failure", &self.failure)
            .finish()
    }
}

impl<T> Reply for ImmediateReply<T> {
    type Item = T;

    fn poll_events(&mut self, out: &mut Vec<T>, _: &Waker) -> ReplyStatus {
        match &mut self.items {
            Events::One(item) => out.extend(item.take()),
            Events::Many(items) => out.append(items),
        }
        match self.failure.take() {
            Some(reason) => ReplyStatus::Failed(reason),
            None => ReplyStatus::Done,
        }
    }
}

/// Blocks until `reply` ends, appending its events to `out`: the way a
/// synchronous driver, or an adapter that needs a result before it can go
/// on, takes a whole reply.
///
/// It is the one way for a caller to take a reply that blocks the caller's
/// thread, and the caller chooses it ([`relay`] blocks too, but a worker's
/// thread, not the caller's). A caller that must not block takes the reply
/// otherwise (see [`Reply`]): by polling it with its own waker, as a
/// [`Future`] with [`wait_async`](crate::wait_async),
/// or as a `Stream` with [`async_events`](crate::async_events). An adapter
/// that calls `wait` inside its own `call` blocks its caller's thread for
/// that time, whichever way the caller takes the reply.
///
/// Between polls the thread parks, and the reply's waker unparks it, so
/// waiting for work done elsewhere uses no CPU. A wake that comes before the
/// thread parks is not lost: the waker sets a flag before it unparks, and
/// `wait` parks only while the flag is clear. The flag is needed because the
/// park token alone is not enough: a poll that itself blocks on something
/// that parks (a channel, a mutex) can use the token up.
///
/// Do not call it from inside a `poll_events`: `wait` and [`relay`] share
/// one parker per thread, so a nested call can take a wake meant for the
/// outer one, which then parks with no waker left. A poll must not block.
///
/// # Errors
///
/// The reason of [`ReplyStatus::Failed`], after the events delivered before
/// it; for [`ReplyStatus::Cancelled`], a reason that says the call was
/// cancelled. To tell a cancellation from a failure, poll the reply.
//
// Known issue, open (blocks neither): `wait`, `wait_async` and
// `async_events` give a cancellation as text, as they give a failure; a
// typed end would change their error type at every call site.
pub fn wait<R: Reply>(reply: R, out: &mut Vec<R::Item>) -> Result<(), String> {
    (wait_status(reply, out).into_end()).expect("`wait_status` gives a reply's end, not `Pending`")
}

/// [`wait`], giving the reply's end as it is, not as text: for the crate's
/// own checks that tell a cancellation from a failure.
//
// Design question, open (blocks neither this commit nor a later one):
// making it public, as the prototype's `wait_status`, would let a caller
// of `wait` tell a cancellation from a failure without a parking loop of
// its own (see the known issue above).
pub(crate) fn wait_status<R: Reply>(mut reply: R, out: &mut Vec<R::Item>) -> ReplyStatus {
    PARKER.with(|parker| {
        loop {
            match reply.poll_events(out, &parker.waker) {
                ReplyStatus::Pending => parker.park(),
                end => return end,
            }
        }
    })
}

/// Polls `reply` on this thread until it ends, and hands each poll's events to
/// `sender` at once, then the end: how an executor that runs the work on
/// another thread (a worker) passes the events of an inner reply up as they
/// come. A cancelled reply ends `sender`'s cancelled too. Between polls it
/// parks, as [`wait`] does, and the same limit holds: do not call it from
/// inside a `poll_events`; nor wait on a reply from a waker it wakes (the
/// caller's, woken on this thread as `sender` is handed the events), since
/// such a waker can take `relay`'s wake on the parker they share.
pub fn relay<R: Reply>(mut reply: R, sender: ReplySender<R::Item>) {
    let mut events = Vec::new();
    PARKER.with(|parker| {
        loop {
            let status = reply.poll_events(&mut events, &parker.waker);
            sender.append(&mut events);
            match status {
                ReplyStatus::Done => return sender.done(),
                ReplyStatus::Failed(reason) => return sender.fail(reason),
                ReplyStatus::Cancelled => return sender.cancel(),
                ReplyStatus::Pending => parker.park(),
            }
        }
    });
}

/// A waker that marks itself woken, then unparks one thread.
struct Unpark {
    thread: Thread,
    woken: AtomicBool,
}

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.woken.store(true, Ordering::Release);
        self.thread.unpark();
    }
}

/// This thread's waker for [`wait`] and [`relay`], and its flag.
struct Parker {
    unpark: Arc<Unpark>,
    waker: Waker,
}

impl Parker {
    fn new() -> Self {
        let unpark = Arc::new(Unpark {
            thread: std::thread::current(),
            woken: AtomicBool::new(false),
        });
        Self {
            waker: Waker::from(Arc::clone(&unpark)),
            unpark,
        }
    }

    /// Parks until the waker has been called since the last return. `park`
    /// may return spuriously, hence the loop. A wake left over from an
    /// earlier reply costs at most one extra poll.
    fn park(&self) {
        while !self.unpark.woken.swap(false, Ordering::Acquire) {
            std::thread::park();
        }
    }
}

thread_local! {
    /// Made once per thread, so that a wait allocates nothing.
    static PARKER: Parker = Parker::new();
    /// [`block_on`]'s, apart from [`PARKER`]: a future it polls may call
    /// [`wait`], whose parks must not take a wake meant for the future.
    static FUTURE_PARKER: Parker = Parker::new();
}

/// Polls `future` on this thread until it is ready, parking between polls
/// as [`wait`] does: how [`Run::wait`](crate::Run::wait) runs a driver.
///
/// It has a parker of its own, so a future that calls [`wait`] (a
/// synchronous driver) loses none of its wakes to it; but a future must not
/// call `block_on` itself.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    FUTURE_PARKER.with(|parker| {
        let mut context = Context::from_waker(&parker.waker);
        loop {
            if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
                return output;
            }
            parker.park();
        }
    })
}

/// Maps a function over each event of a reply `R` on its way up, into a
/// `U`: for example an outcome renamed under the tag its caller gave the
/// readout.
pub struct MappedReply<R: Reply, U> {
    inner: R,
    f: fn(R::Item) -> U,
    buffer: Vec<R::Item>,
}

impl<R: Reply, U> MappedReply<R, U> {
    /// `inner`'s events, each rewritten by `f`.
    pub fn new(inner: R, f: fn(R::Item) -> U) -> Self {
        Self {
            inner,
            f,
            buffer: Vec::new(),
        }
    }
}

impl<R: Reply, U> Reply for MappedReply<R, U> {
    type Item = U;

    fn poll_events(&mut self, out: &mut Vec<U>, waker: &Waker) -> ReplyStatus {
        let status = self.inner.poll_events(&mut self.buffer, waker);
        out.extend(self.buffer.drain(..).map(self.f));
        status
    }
}

/// A reply whose events, of type `T`, are made elsewhere (a worker thread, a
/// device, the receiving end of a link), as a pair: the [`ReplySender`], the
/// executor's end, which pushes each event as soon as it is ready and then the
/// end; and the [`IncrementalReply`], the caller's end, which hands over
/// whatever was pushed, without blocking.
///
/// The sender wakes the caller once per wait, not once per event: a poll
/// that returns [`ReplyStatus::Pending`] leaves its waker (replacing an older
/// one), and the first push of at least one event after it, or the end,
/// takes that waker and wakes it. Pushes after that wake no one until the
/// caller polls again: they wait in the reply, and the next poll takes them
/// all.
#[must_use]
pub fn reply_channel<T>() -> (ReplySender<T>, IncrementalReply<T>) {
    let shared = Arc::new(Mutex::new(Shared {
        events: Vec::new(),
        end: None,
        waker: None,
    }));
    (
        ReplySender {
            shared: Arc::clone(&shared),
            ended: false,
            absorbs_waker_panics: false,
        },
        IncrementalReply { shared },
    )
}

/// What the two ends of a reply share. One lock for the events, the end and
/// the waker, so that the caller's "nothing yet: wake me" and the executor's
/// "here is more: wake" cannot cross: a push comes either before the poll,
/// which then takes it, or after, and then finds the waker.
struct Shared<T> {
    events: Vec<T>,
    /// How the reply ends, once the sender ended it: never `Pending`.
    end: Option<ReplyStatus>,
    waker: Option<Waker>,
}

/// The executor's end of a reply made by [`reply_channel`]: pushes events of
/// type `T`, then the end, and wakes the caller when it waits. Dropped before
/// the end (its worker panicked or was stopped), it ends the reply with
/// [`ReplyStatus::Failed`], so that the caller is not left waiting.
pub struct ReplySender<T> {
    shared: Arc<Mutex<Shared<T>>>,
    ended: bool,
    /// Whether a panic of the caller's waker, as this sender wakes it, is
    /// absorbed rather than raised here; see
    /// [`absorbing_waker_panics`](Self::absorbing_waker_panics).
    absorbs_waker_panics: bool,
}

impl<T> ReplySender<T> {
    /// This sender, absorbing a panic of the caller's waker as it wakes it:
    /// the panic hook still reports the panic, and the push or end goes on
    /// as if the waker had returned. For a sender on an executor's own
    /// thread, where the caller's waker runs code the executor does not
    /// own: its panic is the caller's, and must neither fail the executor
    /// nor escape the executor's `drop` (C-DTOR-FAIL).
    pub(crate) fn absorbing_waker_panics(mut self) -> Self {
        self.absorbs_waker_panics = true;
        self
    }

    /// Hands `event` to the caller now; wakes it if it is waiting (see
    /// [`reply_channel`]).
    pub fn push(&self, event: T) {
        let waker = {
            let mut shared = self.lock();
            shared.events.push(event);
            shared.waker.take()
        };
        // Woken outside the lock, so that the caller does not wait for it.
        if let Some(waker) = waker {
            self.wake(waker);
        }
    }

    /// Hands `events` to the caller now; wakes it if it is waiting and there
    /// was at least one. The events are made before the lock is taken, so
    /// that a slow iterator does not hold up the caller's polls; a worker
    /// that reuses a buffer across calls uses [`append`](Self::append).
    pub fn extend(&self, events: impl IntoIterator<Item = T>) {
        self.append(&mut events.into_iter().collect());
    }

    /// Hands every event of `events` to the caller now, leaving it empty;
    /// wakes the caller if it is waiting and there was at least one. It moves
    /// the buffer when the caller has taken everything before, so a large
    /// batch is not copied.
    pub fn append(&self, events: &mut Vec<T>) {
        if events.is_empty() {
            return;
        }
        let waker = {
            let mut shared = self.lock();
            move_into(&mut shared.events, events);
            shared.waker.take()
        };
        if let Some(waker) = waker {
            self.wake(waker);
        }
    }

    /// Every event was pushed: the reply ends with [`ReplyStatus::Done`].
    pub fn done(mut self) {
        self.end(ReplyStatus::Done);
    }

    /// The run stopped: the reply ends with [`ReplyStatus::Failed`] and
    /// `reason`, after the events pushed so far.
    pub fn fail(mut self, reason: String) {
        self.end(ReplyStatus::Failed(reason));
    }

    /// The call was cancelled: the reply ends with
    /// [`ReplyStatus::Cancelled`], after the events pushed so far.
    pub(crate) fn cancel(mut self) {
        self.end(ReplyStatus::Cancelled);
    }

    fn end(&mut self, end: ReplyStatus) {
        self.ended = true;
        let waker = {
            let mut shared = self.lock();
            shared.end = Some(end);
            shared.waker.take()
        };
        if let Some(waker) = waker {
            self.wake(waker);
        }
    }

    /// Wakes the caller's `waker`, absorbing its panic if this sender does.
    fn wake(&self, waker: Waker) {
        if self.absorbs_waker_panics {
            absorb(|| waker.wake());
        } else {
            waker.wake();
        }
    }

    /// The shared state. A poisoned lock still holds valid events: each
    /// change to them is one call that cannot panic halfway.
    fn lock(&self) -> std::sync::MutexGuard<'_, Shared<T>> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<T> Drop for ReplySender<T> {
    fn drop(&mut self) {
        if !self.ended {
            self.end(ReplyStatus::Failed(
                "the executor dropped the reply before its end".into(),
            ));
        }
    }
}

/// The caller's end of a reply made by [`reply_channel`], whose events, of type
/// `T`, arrive piece by piece. It hands over what the [`ReplySender`] pushed so
/// far, and is [`ReplyStatus::Pending`] until the sender ends. The counterpart
/// of [`ImmediateReply`], whose events all exist when the call returns.
pub struct IncrementalReply<T> {
    shared: Arc<Mutex<Shared<T>>>,
}

impl<T> Reply for IncrementalReply<T> {
    type Item = T;

    fn poll_events(&mut self, out: &mut Vec<T>, waker: &Waker) -> ReplyStatus {
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        move_into(out, &mut shared.events);
        if let Some(end) = shared.end.take() {
            return end;
        }
        if (shared.waker.as_ref()).is_some_and(|kept| kept.will_wake(waker)) {
            return ReplyStatus::Pending;
        }
        drop(shared);
        // A waker's clone and drop run its own code, so neither runs under
        // the lock: an old waker that owns this channel's sender ends the
        // reply as it drops, which takes the lock. Cloned after a first
        // lock, so that a poll that keeps its waker clones nothing; what
        // the sender gave in between is taken under the second.
        let new = waker.clone();
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        move_into(out, &mut shared.events);
        let (status, old) = match shared.end.take() {
            Some(end) => (end, Some(new)),
            None => (ReplyStatus::Pending, shared.waker.replace(new)),
        };
        drop(shared);
        drop(old);
        status
    }
}

/// Appends `from` to `to` and leaves `from` empty, by swapping the buffers
/// when `to` is empty, so that no event is copied.
fn move_into<T>(to: &mut Vec<T>, from: &mut Vec<T>) {
    if to.is_empty() {
        std::mem::swap(to, from);
    } else {
        to.append(from);
    }
}
