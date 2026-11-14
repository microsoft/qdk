// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A part's panics, caught by the protocol: in a part's future, made,
//! polled, or dropped by the protocol ([`catch_panic`]), which makes a
//! failure of it, and in its `end_shots` ([`end_both`]), which ends every
//! other part before raising it again.
//!
//! So that one part's panic neither leaves another part's shots held nor
//! escapes where the protocol promises a failure. Only where a panic
//! unwinds: on wasm32, it aborts, and nothing is caught.

use std::any::Any;
use std::future::poll_fn;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::pin::{Pin, pin};
use std::task::{Context, Poll, ready};

/// Makes a part's future with `make`, and awaits it: its output, or the
/// message of the part's panic, when the message is text. A panic in the
/// future's destructor is the part's too when the protocol drops the
/// future: it is dropped inside the catch, as soon as it is ready or has
/// panicked, so the boundary that names the part catches its panic whether
/// the future is made, polled or dropped.
///
/// If this future is itself dropped before the part's is ready, the part's
/// future is dropped with it, outside the catch: a panic in its destructor
/// is raised again from the drop, as [`end_both`] raises a panic of
/// `end_shots`, unless the thread is already unwinding, where a second
/// panic would abort the process: then that panic's payload is dropped.
/// Whose panic it then is depends on who dropped this future: a part above
/// that stopped waiting for it, whose own boundary catches the panic in its
/// call and names that part above; or the drop of a run's future, where no
/// one is left to return a failure to.
///
/// `AssertUnwindSafe`, because a part may be left half-updated by its
/// panic, and the protocol calls it again: the panic reaches the part
/// above as a failure, the runner ends the run's shots on every part, the
/// part that panicked included, and a later run calls it as before. So a
/// part must tolerate being called after its panic, as after an error, as
/// [`Failure::Panicked`](crate::Failure::Panicked) says.
/// Unless a part above recovers from the failure, the run gives no output,
/// so nothing reaches the user from a half-updated part.
pub(crate) async fn catch_panic<F: Future>(
    make: impl FnOnce() -> F,
) -> Result<F::Output, Option<String>> {
    let future = catch_unwind(AssertUnwindSafe(make)).map_err(panic_message)?;
    let mut held = HeldFuture(pin!(Some(future)));
    poll_fn(
        |context| match catch_unwind(AssertUnwindSafe(|| held.poll_then_drop(context))) {
            Ok(Poll::Ready(output)) => Poll::Ready(Ok(output)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => {
                // The poll's panic is the part's first, so its message is
                // the one the failure gives.
                drop(held.drop_caught());
                Poll::Ready(Err(panic_message(payload)))
            }
        },
    )
    .await
}

/// A part's future, pinned in place until it is dropped, which happens
/// inside a catch: [`catch_panic`]'s, [`drop_caught`](Self::drop_caught)'s
/// or its own drop's.
struct HeldFuture<'a, F>(Pin<&'a mut Option<F>>);

impl<F: Future> HeldFuture<'_, F> {
    /// Polls the future, and drops it once it is ready, so that a panic in
    /// its destructor unwinds through the caller's catch.
    fn poll_then_drop(&mut self, context: &mut Context<'_>) -> Poll<F::Output> {
        let future = self
            .0
            .as_mut()
            .as_pin_mut()
            .expect("a future is not polled once it is ready or has panicked");
        let output = ready!(future.poll(context));
        self.0.set(None);
        Poll::Ready(output)
    }
}

impl<F> HeldFuture<'_, F> {
    /// Drops the future, if it is still held, in a catch of its own: the
    /// payload of a panic there, if any. An assignment whose drop panics
    /// still stores the new value, so the future is not held after.
    fn drop_caught(&mut self) -> Option<Box<dyn Any + Send>> {
        catch_unwind(AssertUnwindSafe(|| self.0.set(None))).err()
    }
}

impl<F> Drop for HeldFuture<'_, F> {
    fn drop(&mut self) {
        if let Some(payload) = self.drop_caught()
            && !std::thread::panicking()
        {
            resume_unwind(payload);
        }
    }
}

/// Calls `first`, then `second`, each a part's `end_shots`, each in a
/// catch of its own, so that a panic of the first leaves the second
/// ended; then raises the first panic again, once both are ended.
///
/// Unless this thread is already unwinding, such as when a run's future is
/// dropped by another panic: a second panic would abort the process, so
/// the end's is dropped. `AssertUnwindSafe`, as for [`catch_panic`].
pub(crate) fn end_both(first: impl FnOnce(), second: impl FnOnce()) {
    // Known issue: a payload whose own drop panics (`panic_any` of such a
    // value) is dropped outside any catch, the second panic's here, or
    // either while unwinding, so its drop's panic escapes; it needs two
    // bugs at once. So it is wherever this module drops a payload,
    // `panic_message` included.
    let first = catch_unwind(AssertUnwindSafe(first));
    let second = catch_unwind(AssertUnwindSafe(second));
    if let Err(payload) = first.and(second)
        && !std::thread::panicking()
    {
        resume_unwind(payload);
    }
}

/// The message of a panic whose payload is `payload`, when it is text.
fn panic_message(payload: Box<dyn Any + Send>) -> Option<String> {
    match payload.downcast::<String>() {
        Ok(message) => Some(*message),
        Err(payload) => payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned()),
    }
}
