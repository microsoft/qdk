// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Replies: how the events of a call reach the caller.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// This thread's waker for [`wait`], and its flag.
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
