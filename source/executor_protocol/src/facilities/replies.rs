// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The crate's replies ([`Reply`]), for an executor to reply with: made
//! whole inside the call ([`ImmediateReply`]), made elsewhere and pushed as
//! they come ([`reply_channel`]), relayed from a worker ([`relay`]), or
//! converted on the way ([`MappedReply`]).

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::Waker;

use crate::facilities::panics::absorb;
use crate::reply::parker::PARKER;
#[cfg(doc)]
use crate::wait;
use crate::{Reply, ReplyStatus};

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

    /// The events it holds, in order, to change in place.
    pub(crate) fn items_mut(&mut self) -> &mut [T] {
        match &mut self.items {
            Events::One(item) => item.as_mut_slice(),
            Events::Many(items) => items,
        }
    }

    /// The events it holds, in order.
    pub(crate) fn items(&self) -> &[T] {
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
