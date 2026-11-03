// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How a thread sleeps until a reply wakes it: the [`Parker`], for the
//! crate's own blocking waits.
//!
//! A reply never blocks: a poll that returns [`ReplyStatus::Pending`] keeps
//! the poll's waker and wakes it once a new poll could give more. A caller
//! that blocks needs a waker that ends its sleep, and that is the parker's:
//!
//! 1. The thread polls the reply with the parker's waker, a
//!    [`ThreadWaker`].
//! 2. On `Pending` it calls [`Parker::park`], which loops on
//!    `woken.swap(false)`, parking while the flag is clear.
//! 3. The reply's side, on any thread, wakes the waker: [`ThreadWaker`]
//!    sets `woken`, then unparks the thread.
//! 4. `park` sees the flag, clears it and returns; the thread polls again.
//!
//! Since `park` returns only once it has seen the flag, a spurious unpark
//! (or a park token used up by a poll that blocked on a channel or a mutex)
//! is harmless: the loop parks again. A wake that comes before the thread
//! parks, even during the poll, is not lost: the flag is already set, and
//! `park` returns at once. A wake left over from an earlier reply costs one
//! extra poll at most.
//!
//! Each thread has one parker ([`PARKER`]), made at its first use, so that
//! a wait allocates nothing. It serves:
//!
//! - [`wait`] (and [`wait_status`]): a synchronous caller takes a whole
//!   reply;
//! - [`relay`]: a worker thread passes an inner reply's events up as they
//!   come.
//!
//! The two share it, so neither may be called from inside a `poll_events`:
//! the nested wait could take the outer one's wake.
//!
//! [`block_on`], with which [`Run::wait`](crate::Run::wait) runs a job's
//! program on this thread, has a parker of its own ([`FUTURE_PARKER`]): the
//! program's async code, run synchronously, may itself call [`wait`] (an
//! adapter's logic does, inside its `call`), and those parks must not take
//! a wake meant for the program's future.
//!
//! Other callers do not park here. An async caller ([`wait_async`]) and a
//! stream ([`async_events`]) hand the reply their runtime's waker, and their
//! task yields instead of the thread parking. Some of the crate's polls
//! pass [`Waker::noop`] and never sleep: they poll once, and a later poll
//! with a real waker takes the reply up (an `adapter_fn`'s poll of its
//! sends, a `shot_states` executor's poll of each part as it calls it, and
//! the conformance check that polls a reply it should not have got).
//!
//! Keeping the waker is the reply's side: [`IncrementalReply`] and
//! [`LifecycleBoundReply`] keep the latest waker, skip the clone when it
//! [`will_wake`](Waker::will_wake) as the kept one does, and clone or drop
//! a waker only outside their lock, since a waker's clone and drop run its
//! own code. Replies that gather the replies of parts (a `shot_states`
//! executor's, an `adapter_fn`'s) hand the caller's waker to each part's
//! poll, so that any part wakes the caller.
//!
//! The crate's `docs/replies.md`, §The waking model, walks through the
//! cycle from the caller's side.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::Thread;

#[cfg(doc)]
use super::blocking::wait_status;
#[cfg(doc)]
use crate::{
    IncrementalReply, LifecycleBoundReply, ReplyStatus, async_events, relay, wait, wait_async,
};

/// A waker that marks itself woken, then unparks one thread.
struct ThreadWaker {
    thread: Thread,
    woken: AtomicBool,
}

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.woken.store(true, Ordering::Release);
        self.thread.unpark();
    }
}

/// This thread's waker for [`wait`] and [`relay`], and its flag.
pub(crate) struct Parker {
    thread_waker: Arc<ThreadWaker>,
    pub(crate) waker: Waker,
}

impl Parker {
    fn new() -> Self {
        let thread_waker = Arc::new(ThreadWaker {
            thread: std::thread::current(),
            woken: AtomicBool::new(false),
        });
        Self {
            waker: Waker::from(Arc::clone(&thread_waker)),
            thread_waker,
        }
    }

    /// Parks until the waker has been called since the last return. `park`
    /// may return spuriously, hence the loop. A wake left over from an
    /// earlier reply costs at most one extra poll.
    pub(crate) fn park(&self) {
        while !self.thread_waker.woken.swap(false, Ordering::Acquire) {
            std::thread::park();
        }
    }
}

thread_local! {
    /// Made once per thread, so that a wait allocates nothing.
    pub(crate) static PARKER: Parker = Parker::new();
    /// [`block_on`]'s, apart from [`PARKER`]: a future it polls may call
    /// [`wait`], whose parks must not take a wake meant for the future.
    static FUTURE_PARKER: Parker = Parker::new();
}

/// Polls `future` on this thread until it is ready, parking between polls
/// as [`wait`] does: how [`Run::wait`](crate::Run::wait) runs a driver.
///
/// It has a parker of its own, so a future that calls [`wait`] (a
/// synchronous caller) loses none of its wakes to it; but a future must not
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
