// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Replies: how the events of a call reach the caller.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Wake, Waker};
use std::thread::Thread;

/// How a reply stands after a poll.
///
/// A failure is the executor's account of a run that started and could not
/// finish, so its reason stays text: drivers report it, they do not match
/// it. A call that starts no work is refused with a typed
/// [`Rejected`](crate::Rejected) instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// More events may come.
    Pending,
    /// Every event has been delivered.
    Done,
    /// The run stopped; the events delivered before it stand.
    Failed(String),
}

/// The events of one call, however they arrive: all at once when the work
/// ran inside the call, or piece by piece when it runs elsewhere.
///
/// A completion is polled and never blocks. After a poll that returns
/// [`Status::Pending`], it keeps the waker of that poll (the latest one
/// replaces the others) and wakes it once a new poll could deliver more
/// events or end the reply. The wake is needed because without it a caller
/// that wants the events could only poll again and again: the executor knows
/// when its events are ready, the caller knows when it wants them, and the
/// waker joins the two. A completion is not polled again after `Done` or
/// `Failed`.
pub trait Completion {
    /// The event this completion delivers.
    type Item;

    /// Moves the events available now to the end of `out`, leaving what `out`
    /// already held as it was. Never blocks.
    fn poll_events(&mut self, out: &mut Vec<Self::Item>, waker: &Waker) -> Status;
}

/// A reply whose events all exist when `call` returns: work done
/// synchronously inside the call. Done at its first poll, so it never keeps
/// the waker.
#[derive(Clone, Debug, PartialEq)]
pub struct Immediate<T> {
    items: Vec<T>,
    failure: Option<String>,
}

impl<T> Immediate<T> {
    /// All of `items`, then [`Status::Done`].
    #[must_use]
    pub fn done(items: Vec<T>) -> Self {
        Self {
            items,
            failure: None,
        }
    }

    /// All of `items`, then [`Status::Failed`] with `reason`.
    #[must_use]
    pub fn failed(items: Vec<T>, reason: String) -> Self {
        Self {
            items,
            failure: Some(reason),
        }
    }
}

impl<T> Completion for Immediate<T> {
    type Item = T;

    fn poll_events(&mut self, out: &mut Vec<T>, _: &Waker) -> Status {
        out.append(&mut self.items);
        match self.failure.take() {
            Some(reason) => Status::Failed(reason),
            None => Status::Done,
        }
    }
}

/// Blocks until `completion` ends, appending its events to `out`: the way a
/// synchronous driver, or an adapter that needs a result before it can go
/// on, takes a whole reply.
///
/// Between polls the thread parks, and the completion's waker unparks it, so
/// waiting for work done elsewhere uses no CPU. A wake that comes before the
/// thread parks is not lost: the waker sets a flag before it unparks, and
/// `wait` parks only while the flag is clear. The flag is needed because the
/// park token alone is not enough: a poll that itself blocks on something
/// that parks (a channel, a mutex) can use the token up.
///
/// # Errors
///
/// The reason of [`Status::Failed`], after the events delivered before it.
pub fn wait<C: Completion>(mut completion: C, out: &mut Vec<C::Item>) -> Result<(), String> {
    PARKER.with(|parker| {
        loop {
            match completion.poll_events(out, &parker.waker) {
                Status::Done => return Ok(()),
                Status::Failed(reason) => return Err(reason),
                Status::Pending => parker.park(),
            }
        }
    })
}

/// Polls `completion` on this thread until it ends, and hands each poll's
/// events to `completer` at once, then the end: how an executor that runs the
/// work on another thread (a worker) passes the events of an inner reply up
/// as they come. Between polls it parks, as [`wait`] does.
pub fn relay<C: Completion>(mut completion: C, completer: Completer<C::Item>) {
    let mut events = Vec::new();
    PARKER.with(|parker| {
        loop {
            let status = completion.poll_events(&mut events, &parker.waker);
            completer.append(&mut events);
            match status {
                Status::Done => return completer.done(),
                Status::Failed(reason) => return completer.fail(reason),
                Status::Pending => parker.park(),
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
    /// earlier completion costs at most one extra poll.
    fn park(&self) {
        while !self.unpark.woken.swap(false, Ordering::Acquire) {
            std::thread::park();
        }
    }
}

thread_local! {
    /// Made once per thread, so that a wait allocates nothing.
    static PARKER: Parker = Parker::new();
}

/// Maps a function over each event of a reply on its way up: for example a
/// shot's outcome into a batch's outcome.
pub struct Map<C: Completion, U> {
    inner: C,
    f: fn(C::Item) -> U,
    buffer: Vec<C::Item>,
}

impl<C: Completion, U> Map<C, U> {
    /// `inner`'s events, each rewritten by `f`.
    pub fn new(inner: C, f: fn(C::Item) -> U) -> Self {
        Self {
            inner,
            f,
            buffer: Vec::new(),
        }
    }
}

impl<C: Completion, U> Completion for Map<C, U> {
    type Item = U;

    fn poll_events(&mut self, out: &mut Vec<U>, waker: &Waker) -> Status {
        let status = self.inner.poll_events(&mut self.buffer, waker);
        out.extend(self.buffer.drain(..).map(self.f));
        status
    }
}

/// A reply whose events are made elsewhere (a worker thread, a device, the
/// receiving end of a link), as a pair: the [`Completer`], the executor's
/// end, which pushes each event as soon as it is ready and then the end; and
/// the [`Incremental`], the caller's end, a completion that hands over
/// whatever was pushed, without blocking. The completer wakes the caller's
/// latest waker when it pushes or ends.
#[must_use]
pub fn completer<T>() -> (Completer<T>, Incremental<T>) {
    let shared = Arc::new(Mutex::new(Shared {
        events: Vec::new(),
        end: None,
        waker: None,
    }));
    (
        Completer {
            shared: Arc::clone(&shared),
            ended: false,
        },
        Incremental { shared },
    )
}

/// What the two ends of a reply share. One lock for the events, the end and
/// the waker, so that the caller's "nothing yet: wake me" and the executor's
/// "here is more: wake" cannot cross: a push comes either before the poll,
/// which then takes it, or after, and then finds the waker.
struct Shared<T> {
    events: Vec<T>,
    end: Option<Result<(), String>>,
    waker: Option<Waker>,
}

/// The executor's end of a reply made by [`completer`]: pushes events, then
/// the end, and wakes the caller. Dropped before the end (its worker
/// panicked or was stopped), it ends the reply with [`Status::Failed`], so
/// that the caller is not left waiting.
pub struct Completer<T> {
    shared: Arc<Mutex<Shared<T>>>,
    ended: bool,
}

impl<T> Completer<T> {
    /// Hands `event` to the caller now.
    pub fn push(&self, event: T) {
        self.extend(std::iter::once(event));
    }

    /// Hands `events` to the caller now; wakes it if there was at least one.
    pub fn extend(&self, events: impl IntoIterator<Item = T>) {
        let waker = {
            let mut shared = self.lock();
            let before = shared.events.len();
            shared.events.extend(events);
            if shared.events.len() == before {
                return;
            }
            shared.waker.take()
        };
        // Woken outside the lock, so that the caller does not wait for it.
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Hands every event of `events` to the caller now, leaving it empty;
    /// wakes the caller if there was at least one. It moves the buffer when
    /// the caller has taken everything before, so a large batch is not
    /// copied.
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
            waker.wake();
        }
    }

    /// Every event was pushed: the reply ends with [`Status::Done`].
    pub fn done(mut self) {
        self.end(Ok(()));
    }

    /// The run stopped: the reply ends with [`Status::Failed`] and `reason`,
    /// after the events pushed so far.
    pub fn fail(mut self, reason: String) {
        self.end(Err(reason));
    }

    fn end(&mut self, end: Result<(), String>) {
        self.ended = true;
        let waker = {
            let mut shared = self.lock();
            shared.end = Some(end);
            shared.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// The shared state. A poisoned lock still holds valid events: each
    /// change to them is one call that cannot panic halfway.
    fn lock(&self) -> std::sync::MutexGuard<'_, Shared<T>> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<T> Drop for Completer<T> {
    fn drop(&mut self) {
        if !self.ended {
            self.end(Err("the executor dropped the reply before its end".into()));
        }
    }
}

/// The caller's end of a reply made by [`completer`]: a completion whose
/// events arrive piece by piece. It hands over what the [`Completer`] pushed
/// so far, and is [`Status::Pending`] until the completer ends. The
/// counterpart of [`Immediate`], whose events all exist when the call
/// returns.
pub struct Incremental<T> {
    shared: Arc<Mutex<Shared<T>>>,
}

impl<T> Completion for Incremental<T> {
    type Item = T;

    fn poll_events(&mut self, out: &mut Vec<T>, waker: &Waker) -> Status {
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        move_into(out, &mut shared.events);
        match shared.end.take() {
            Some(Ok(())) => Status::Done,
            Some(Err(reason)) => Status::Failed(reason),
            None => {
                if !shared
                    .waker
                    .as_ref()
                    .is_some_and(|kept| kept.will_wake(waker))
                {
                    shared.waker = Some(waker.clone());
                }
                Status::Pending
            }
        }
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
