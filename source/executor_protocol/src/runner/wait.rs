// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How [`Runner::run`](crate::Runner::run) waits for a run with no async
//! runtime: [`wait`] polls the run's future on the calling thread, and
//! parks the thread until a part wakes it.
//!
//! The thread sleeps while a part waits, such as an executor whose device
//! answers later, and wakes as soon as the part wakes the run's waker,
//! from any thread:
//!
//! 1. The thread polls the future with a [`ThreadWaker`].
//! 2. On `Pending`, it parks, in a loop on `woken.swap(false)`.
//! 3. The part, on any thread, wakes the waker, which sets `woken`, then
//!    unparks the thread.
//! 4. The loop sees the flag, clears it, and the thread polls again.
//!
//! A wake that comes before the thread parks, even during the poll, is not
//! lost: the flag is already set, and the loop returns at once. A spurious
//! unpark, or one used up by a part that blocked on a lock, is harmless: the
//! loop parks again. Each wait has a waker of its own, so a part may itself
//! wait for a run of its own inside the run without taking its wakes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::Thread;

/// Polls `future` on this thread until it is ready, parking between polls
/// until its waker is woken: its output.
pub(crate) fn wait<F: Future>(future: F) -> F::Output {
    let thread_waker = Arc::new(ThreadWaker {
        thread: std::thread::current(),
        woken: AtomicBool::new(false),
    });
    let waker = Waker::from(Arc::clone(&thread_waker));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        while !thread_waker.woken.swap(false, Ordering::Acquire) {
            std::thread::park();
        }
    }
}

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
