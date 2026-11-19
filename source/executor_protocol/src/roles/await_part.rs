// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The protocol awaits a part's future through [`await_part`], so that a
//! panic leaves no live future of the part: one whose destructor could
//! panic a second time while the first unwinds, and so abort the process.
//! Its users are the roles' provided methods, which await a part's own
//! calls (the default `_each` and `_stream` forms), and composition,
//! whose runner, compositions and boundaries await the parts' calls;
//! composition's `part_panics` builds on it to make a failure of a part's
//! panic. Only where a panic unwinds: on wasm32, it aborts, and nothing is
//! caught.
//!
//! It guards the futures of parts it drops, not the values parts return,
//! as std's containers do not: an output or refusal held while a panic
//! unwinds, such as one of its future's destructor, is dropped unguarded,
//! and a panic of its own destructor then aborts the process.
//!
//! The protocol's futures that wrap a part's, this one and composition's
//! `CatchPanic`, are written by hand, not as `async fn`s, because of their
//! shape: an `async fn` would make a generator of its own, nested in its
//! caller's, and a boundary's poll would go down through each level, a
//! cost per adapter and call that shows when batches are small; written
//! by hand, each is a plain struct whose poll calls the one below it
//! directly, with no generator's state of its own. The gain is the
//! shape's, not inlining's: it holds with their polls out of line. Measured
//! on a batch of one instruction, it more than halves the cost of a call
//! through three boundaries (about 125 ns to 56). The bench's `chains`
//! section (`examples/bench/chains.rs`) times the boundary's cost per call
//! against a direct one.
//!
//! A future written by hand that holds another one must poll it where it
//! lies: a part's future, made by an `async fn` for instance, may point
//! into itself, so it is pinned, and reaching it through the outer
//! future's pin (a pin projection) takes `unsafe` code in plain Rust. The
//! `pin_project!` macro of `pin-project-lite` writes that projection
//! safely, and its `PinnedDrop` lets [`AwaitPart`]'s destructor drop the
//! part's future in place, inside a catch. It is the usual way to write
//! such futures: `futures-util` builds its own with it, among them
//! `CatchUnwind`, which wraps a future as `CatchPanic` does but guards
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
/// composition's, holding no live future of a part, whose destructor
/// could panic a second time while the first unwinds, and so abort the
/// process. Every future of a part the protocol awaits is awaited through
/// this, composition's `catch_panic` included. A panic leaves it as it
/// would leave the part's future, so the boundary that catches it names
/// the same part. `AssertUnwindSafe`, because a part may be left
/// half-updated by its panic, and the protocol calls it again, as
/// `catch_panic` says.
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
    pub(crate) fn unstarted() -> Self {
        Self { future: None }
    }

    /// Holds `future`, the part's, to await it. Called once, on an
    /// [`unstarted`](Self::unstarted) one: replacing a held future would
    /// drop it outside a catch, which a debug build asserts against (the
    /// one caller, `CatchPanic`'s first poll, takes its `make` first, so
    /// it cannot call this twice; a release build spares the hot path the
    /// check).
    pub(crate) fn start(self: Pin<&mut Self>, future: F) {
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
    // Inlined where it can be: the default `execute_stream` awaits the
    // part's `execute` through this too, a second caller of the same
    // instance, and LLVM then left it out of line under the boundary's
    // `execute`, a call per batch (a quarter slower on batches of one
    // instruction, in the bench's `chains` section).
    #[inline]
    pub(crate) fn poll_caught(
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
