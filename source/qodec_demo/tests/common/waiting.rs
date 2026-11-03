//! How the tests wait: a bound on a test's time, a reply's end as it is,
//! and several futures awaited together on one thread.

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::Duration;

use executor_protocol::{Reply, ReplyStatus};

/// How long a test may run before it counts as hung: far more than any
/// test here takes in a debug build.
pub const HANG: Duration = Duration::from_secs(120);

/// `test`, run on a thread of its own and given [`HANG`] to end: what it
/// returns, or its panic, raised here. A test that does not end in time
/// fails as hung instead of stalling the suite. The chains are built
/// inside `test`, since a chain with a Qodec layer cannot move to another
/// thread.
pub fn bounded<T: Send + 'static>(test: impl FnOnce() -> T + Send + 'static) -> T {
    let (ended, end) = mpsc::channel();
    let handle = thread::spawn(move || {
        let output = test();
        // The receiver is gone only once the test failed as hung.
        let _ = ended.send(());
        output
    });
    match end.recv_timeout(HANG) {
        Ok(()) | Err(RecvTimeoutError::Disconnected) => match handle.join() {
            Ok(output) => output,
            Err(panic) => std::panic::resume_unwind(panic),
        },
        Err(RecvTimeoutError::Timeout) => panic!("hung: the test did not end within {HANG:?}"),
    }
}

/// Unparks the thread that waits for a reply.
struct Unpark(thread::Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Polls `reply` until it ends, parking between polls: its end as it is,
/// so that a cancellation shows apart from a failure, which `wait` gives
/// as text alike.
pub fn end_of<R: Reply>(reply: &mut R, out: &mut Vec<R::Item>) -> ReplyStatus {
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    loop {
        match reply.poll_events(out, &waker) {
            ReplyStatus::Pending => thread::park(),
            end => return end,
        }
    }
}

/// Awaits each of `futures` together on this thread, polling them in turn
/// and parking while none is ready, as a one-thread async runtime does:
/// their outputs, in their order, and how many of the polls found a future
/// pending.
pub fn join_all<F: Future>(futures: Vec<F>) -> (Vec<F::Output>, usize) {
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut futures: Vec<_> = futures.into_iter().map(Box::pin).collect();
    let mut outputs: Vec<Option<F::Output>> = futures.iter().map(|_| None).collect();
    let mut pending = 0;
    while outputs.iter().any(Option::is_none) {
        let mut ready = false;
        for (future, output) in futures.iter_mut().zip(&mut outputs) {
            if output.is_some() {
                continue;
            }
            match future.as_mut().poll(&mut context) {
                Poll::Ready(out) => {
                    *output = Some(out);
                    ready = true;
                }
                Poll::Pending => pending += 1,
            }
        }
        if !ready && outputs.iter().any(Option::is_none) {
            thread::park();
        }
    }
    (outputs.into_iter().flatten().collect(), pending)
}
