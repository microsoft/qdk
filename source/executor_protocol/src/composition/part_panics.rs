// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A part's panics, caught by the protocol: in a part's future, made,
//! polled, or dropped by the protocol ([`catch_panic`]), which makes a
//! failure of it, and in its `end_shots` ([`end_both`]), which ends every
//! other part before raising it again. Wherever the protocol awaits a
//! part's future, it does so through the roles'
//! [`await_part`](fn@crate::roles::await_part), so that a panic leaves no
//! live future of a part behind it. A panic in what the caller gives a
//! part to call back, an `execute_each`'s `each`, a run's `each`, or an
//! `execute_stream`'s instructions, is kept by [`CallersCallback`] and
//! raised again on the caller's side, so that it names the caller, not the
//! part that called it back.
//!
//! So that one part's panic neither leaves another part's shots held nor
//! escapes where the protocol promises a failure, nor aborts the process
//! by a second panic. Only where a panic unwinds: on wasm32, it aborts,
//! and nothing is caught.
//!
//! [`CatchPanic`] is written by hand with `pin_project!`, as [`AwaitPart`]
//! is, which it builds on: the header of `roles/await_part.rs` says why,
//! and which values a panic leaves unguarded.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

use crate::roles::AwaitPart;

/// Makes a part's future with `make`, and awaits it: its output, or the
/// message of the part's panic, when the message is text. `make` is called
/// when this future is first polled, as an `async fn`'s body runs, so a
/// future dropped before its first poll has called nothing of the part. A
/// panic in the part's future's destructor is the part's too when the
/// protocol drops the future: it is dropped inside the catch, as soon as it
/// is ready or has panicked, so the boundary that names the part catches
/// its panic whether the future is made, polled or dropped.
///
/// If this future is itself dropped before the part's is ready, the part's
/// future is dropped with it, in a catch of its own: a panic in its
/// destructor is raised again from the drop, as [`end_both`] raises a panic of
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
pub(crate) fn catch_panic<M, F>(make: M) -> CatchPanic<M, F>
where
    M: FnOnce() -> F,
    F: Future,
{
    CatchPanic {
        make: Some(make),
        part: AwaitPart::unstarted(),
    }
}

pin_project! {
    /// [`catch_panic`]'s future.
    #[must_use = "futures do nothing unless polled"]
    pub(crate) struct CatchPanic<M, F> {
        // `make`, until the first poll calls it.
        make: Option<M>,
        // The part's future, once `make` has made it.
        #[pin]
        part: AwaitPart<F>,
    }
}

impl<M, F> Future for CatchPanic<M, F>
where
    M: FnOnce() -> F,
    F: Future,
{
    type Output = Result<F::Output, Option<String>>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        if let Some(make) = this.make.take() {
            match catch_unwind(AssertUnwindSafe(make)) {
                Ok(future) => this.part.as_mut().start(future),
                Err(payload) => return Poll::Ready(Err(panic_message(payload))),
            }
        }
        match this.part.poll_caught(context) {
            Ok(poll) => poll.map(Ok),
            Err(payload) => Poll::Ready(Err(panic_message(payload))),
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

/// The caller's callback (an `execute_each`'s `each`, a run's `each`, an
/// `execute_stream`'s instructions), as the part below calls it, so that a
/// panic in it is the caller's, not that part's: each call is made in a
/// catch, the first panic is kept, and no later call reaches the callback;
/// the protocol raises the panic again with
/// [`raise_panic`](Self::raise_panic) once the part below has answered
/// (for a run's `each`, once the run's shots are ended), outside the catch
/// that names that part, so that it unwinds through the caller.
/// `AssertUnwindSafe`, as for [`catch_panic`]: the callback is not called
/// again once it has panicked.
///
/// Known issue: if the future holding it is dropped before `raise_panic`,
/// the kept panic is dropped with it.
pub(crate) struct CallersCallback<C> {
    callback: C,
    panic: Option<Box<dyn Any + Send>>,
}

impl<C> CallersCallback<C> {
    /// `callback`, not yet called.
    pub(crate) fn new(callback: C) -> Self {
        Self {
            callback,
            panic: None,
        }
    }

    /// Calls the callback by `call`, in a catch, unless it has panicked
    /// before: its first panic is kept.
    #[inline]
    pub(crate) fn call(&mut self, call: impl FnOnce(&mut C)) {
        if self.panic.is_none()
            && let Err(payload) = catch_unwind(AssertUnwindSafe(|| call(&mut self.callback)))
        {
            self.panic = Some(payload);
        }
    }

    /// Raises the callback's first panic again, if it panicked.
    pub(crate) fn raise_panic(&mut self) {
        if let Some(payload) = self.panic.take() {
            resume_unwind(payload);
        }
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
