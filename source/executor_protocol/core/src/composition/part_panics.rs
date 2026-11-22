// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A part's panics, caught by the protocol: in a part's future, made,
//! polled, or dropped by the protocol ([`catch_panic`]), which makes a
//! failure of it, and in its `end_shots` ([`end_both`]), which ends every
//! other part before raising it again. Wherever the protocol awaits a
//! part's future, it does so through [`await_part`], so that a panic
//! leaves no live future of a part behind it.
//!
//! So that one part's panic neither leaves another part's shots held nor
//! escapes where the protocol promises a failure, nor aborts the process
//! by a second panic. Only where a panic unwinds: on wasm32, it aborts,
//! and nothing is caught.
//!
//! It guards the futures of parts it drops, not the values parts return,
//! as std's containers do not: an output or refusal held while a panic
//! unwinds, such as one of its future's destructor, is dropped unguarded,
//! and a panic of its own destructor then aborts the process.
//!
//! Its futures are written by hand, not as `async fn`s, because of their
//! shape: an `async fn` would make a generator of its own, nested in its
//! caller's, and a boundary's poll would go down through each level, a
//! cost per adapter and call that shows when batches are small; written
//! by hand, each is a plain struct whose poll calls the one below it
//! directly, with no generator's state of its own. The gain is the
//! shape's, not inlining's: it holds with their polls out of line. Measured
//! on a batch of one instruction, it more than halves the cost of a call
//! through three boundaries (about 125 ns to 56).
//!
//! A future written by hand that holds another one must poll it where it
//! lies: a part's future, made by an `async fn` for instance, may point
//! into itself, so it is pinned, and reaching it through the outer
//! future's pin (a pin projection) takes `unsafe` code in plain Rust. The
//! `pin_project!` macro of `pin-project-lite` writes that projection
//! safely, and its `PinnedDrop` lets [`AwaitPart`]'s destructor drop the
//! part's future in place, inside a catch. It is the usual way to write
//! such futures: `futures-util` builds its own with it, among them
//! `CatchUnwind`, which wraps a future as [`catch_panic`] does but guards
//! only its polls; `tokio` depends on it too, and the workspace already
//! builds both. The alternatives cost more: a projection by hand is
//! `unsafe` code to prove sound, boxing the part's future allocates on
//! every call, and requiring `Unpin` futures would refuse those of
//! `async fn`s and `async` blocks, which is how parts write theirs.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

/// Makes a part's future with `make`, and awaits it: its output, or the
/// payload of the part's panic. `make` is called
/// when this future is first polled, as an `async fn`'s body runs, so a
/// future dropped before its first poll has called nothing of the part. A
/// panic in the part's future's destructor is the part's too when the
/// protocol drops the future: it is dropped inside the catch, as soon as it
/// is ready or has panicked, so the boundary that names the part catches
/// its panic whether the future is made, polled or dropped.
///
/// If this future is itself dropped before the part's is ready, the part's
/// future is dropped with it, in a catch of its own: a panic in its
/// destructor is raised again from the drop, as [`end_both`] raises a
/// panic of `end_shots`, unless the thread is already unwinding, where a
/// second panic would abort the process: then that panic's payload is
/// dropped.
/// Whose panic it then is depends on who dropped this future: a part above
/// that stopped waiting for it, whose own boundary catches the panic in its
/// call and names that part above; or the drop of a run's future, where no
/// one is left to return a failure to.
///
/// `AssertUnwindSafe`, because a part may be left half-updated by its
/// panic, and the protocol calls it again: the panic reaches the part
/// above as a failure, the protocol ends the run's shots on every part, the
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
    type Output = Result<F::Output, Box<dyn Any + Send>>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        if let Some(make) = this.make.take() {
            match catch_unwind(AssertUnwindSafe(make)) {
                Ok(future) => this.part.as_mut().start(future),
                Err(payload) => return Poll::Ready(Err(payload)),
            }
        }
        match this.part.poll_caught(context) {
            Ok(poll) => poll.map(Ok),
            Err(payload) => Poll::Ready(Err(payload)),
        }
    }
}

/// Awaits a part's future, `future`, which it drops in the poll that finds
/// it ready or panicking, inside a catch, so that only the first panic
/// leaves: a panic of the poll is raised again once the future is dropped,
/// a panic of that drop being dropped; a panic of the drop of a ready
/// future is raised from that poll, the output dropped as it unwinds. A
/// future dropped before it is ready drops the part's inside a catch too,
/// and raises its panic again. Each raises a drop's panic unless the
/// thread is already unwinding, where a second panic would abort the
/// process: its payload is then dropped.
///
/// So that a panic unwinds through the protocol's own futures, such as a
/// joined part's, holding no live future of a part, whose destructor
/// could panic a second time while the first unwinds, and so abort the
/// process. Every future of a part the protocol awaits is awaited through
/// this, [`catch_panic`]'s included. A panic leaves it as it would leave
/// the part's future, so the boundary that catches it names the same
/// part. `AssertUnwindSafe`, as for [`catch_panic`].
pub(crate) fn await_part<F: Future>(future: F) -> AwaitPart<F> {
    AwaitPart {
        future: Some(future),
    }
}

pin_project! {
    /// [`await_part`]'s future: a part's future, pinned in place until it
    /// is dropped, which only [`drop_caught`] does, inside a catch of its
    /// own: in the poll that finds it ready or panicking, or in this
    /// future's drop.
    #[must_use = "futures do nothing unless polled"]
    pub(crate) struct AwaitPart<F> {
        #[pin]
        future: Option<F>,
    }

    impl<F> PinnedDrop for AwaitPart<F> {
        fn drop(this: Pin<&mut Self>) {
            if let Some(payload) = drop_caught(this.project().future)
                && !std::thread::panicking()
            {
                resume_unwind(payload);
            }
        }
    }
}

impl<F> AwaitPart<F> {
    /// An `AwaitPart` holding no future yet, for [`start`](Self::start).
    fn unstarted() -> Self {
        Self { future: None }
    }

    /// Holds `future`, the part's, to await it. Called once, on an
    /// [`unstarted`](Self::unstarted) one: replacing a held future would
    /// drop it outside a catch, which a debug build asserts against (the
    /// one caller, [`CatchPanic`]'s first poll, takes its `make` first, so
    /// it cannot call this twice; a release build spares the hot path the
    /// check).
    fn start(self: Pin<&mut Self>, future: F) {
        let mut held = self.project().future;
        debug_assert!(held.is_none(), "a part's future is started once");
        held.set(Some(future));
    }
}

impl<F: Future> AwaitPart<F> {
    /// Polls the part's future in a catch, and drops it, in a catch of its
    /// own, in the poll that finds it ready or panicking: the payload of
    /// the first panic, as `Err`. A panic of the drop of a ready future is
    /// raised inside the catch, unless the thread is already unwinding, so
    /// the output is dropped as it unwinds, as std drops a value held when
    /// a panic leaves.
    fn poll_caught(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Result<Poll<F::Output>, Box<dyn Any + Send>> {
        let mut held = self.project().future;
        // Outside the catch, so that a future polled again panics, as an
        // `async fn`'s does, rather than failing as its part.
        assert!(
            held.is_some(),
            "a part's future is not polled once it is ready or has panicked"
        );
        let polled = catch_unwind(AssertUnwindSafe(|| {
            let Some(future) = held.as_mut().as_pin_mut() else {
                unreachable!("checked above")
            };
            let poll = future.poll(context);
            if poll.is_ready()
                && let Some(payload) = drop_caught(held.as_mut())
                && !std::thread::panicking()
            {
                resume_unwind(payload);
            }
            poll
        }));
        if polled.is_err() {
            // The poll's panic is the one raised; a second, of the drop,
            // is dropped.
            drop(drop_caught(held));
        }
        polled
    }
}

impl<F: Future> Future for AwaitPart<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<F::Output> {
        match self.poll_caught(context) {
            Ok(poll) => poll,
            Err(payload) => resume_unwind(payload),
        }
    }
}

/// Drops the future `held`, if it still holds one, in a catch of its own:
/// the payload of a panic there, if any. An assignment whose drop panics
/// still stores the new value, so it holds no future after.
fn drop_caught<F>(mut held: Pin<&mut Option<F>>) -> Option<Box<dyn Any + Send>> {
    catch_unwind(AssertUnwindSafe(|| held.set(None))).err()
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

/// The payload of a panic in an end of a run's shots that a joined driver
/// makes, raised again from its `drive` around the part's own payload, so
/// that every driver above it, and the runner, tell it from a panic of
/// the driver's: they raise the part's own payload again, as the runner
/// raises a panic in its own end of the executor's shots, rather than
/// failing the run naming the driver. Code outside the crate cannot name
/// it, so no part's panic is taken for one.
pub(crate) struct PanicEndingShots(pub(crate) Box<dyn Any + Send>);

/// The message of a panic whose payload is `payload`, when it is text.
pub(crate) fn panic_message(payload: Box<dyn Any + Send>) -> Option<String> {
    match payload.downcast::<String>() {
        Ok(message) => Some(*message),
        Err(payload) => payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned()),
    }
}
