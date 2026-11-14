// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A part's panics, caught by the protocol: in a part's future
//! ([`catch_panic`]), which makes a failure of it, and in its `end_shots`
//! ([`end_both`]), which ends every other part before raising it again.
//!
//! So that one part's panic neither leaves another part's shots held nor
//! escapes where the protocol promises a failure. Only where a panic
//! unwinds: on wasm32, it aborts, and nothing is caught.

use std::any::Any;
use std::future::poll_fn;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::pin::pin;
use std::task::Poll;

/// Makes a part's future with `make`, and awaits it: its output, or the
/// message of the part's panic, when the message is text.
///
/// `AssertUnwindSafe`, because a part may be left half-updated by its
/// panic, and the protocol calls it again: the run fails, the runner ends
/// its shots on every part, the part that panicked included, and a later
/// run calls it as before. So a part must tolerate being called after its
/// panic, as after an error; the run that panicked gives no output, so
/// nothing reaches the user from a half-updated part.
pub(crate) async fn catch_panic<F: Future>(
    make: impl FnOnce() -> F,
) -> Result<F::Output, Option<String>> {
    let future = catch_unwind(AssertUnwindSafe(make)).map_err(panic_message)?;
    let mut future = pin!(future);
    poll_fn(
        |context| match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(context))) {
            Ok(Poll::Ready(output)) => Poll::Ready(Ok(output)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => Poll::Ready(Err(panic_message(payload))),
        },
    )
    .await
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
