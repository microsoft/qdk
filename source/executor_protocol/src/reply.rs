// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Replies: how the events of a call reach the caller.
//!
//! - [`asynchronous`]: the async ways to take a reply: a whole reply as a
//!   `Future`, or its events as a `Stream`.
//! - [`blocking`]: the blocking way, [`wait`]: the whole reply, the thread
//!   parked between polls.
//! - [`parker`]: how a thread parks until a reply wakes it, for [`wait`],
//!   [`relay`] and [`Run::wait`](crate::Run::wait).

pub(crate) mod asynchronous;
pub(crate) mod blocking;
pub(crate) mod parker;

use std::task::Waker;

#[cfg(doc)]
use crate::{
    ImmediateReply, IncrementalReply, MappedReply, ReplySender, relay, reply_channel, wait,
};

/// How a reply stands after a poll.
///
/// A failure is the executor's account of a run that started and could not
/// finish, so its reason stays text: callers report it, they do not match
/// it. A call that starts no work is refused with a typed
/// [`Rejected`](crate::Rejected) instead, and a call abandoned before it
/// ran ends [`Cancelled`](Self::Cancelled), typed too, so that a caller
/// can tell it from a failure.
///
/// Non-exhaustive, so that a status can be added without breaking callers:
/// a match outside this crate has an arm for the statuses it does not know.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplyStatus {
    /// More events may come.
    Pending,
    /// Every event has been delivered.
    Done,
    /// The run stopped; the events delivered before it stand.
    Failed(String),
    /// The call was cancelled before it ran to its end: it, or some of its
    /// parts (a call on several shots, each on an executor of its own),
    /// never started, because an executor that was to run them was
    /// dropped: this executor, or one below it whose reply it hands on. The
    /// events delivered before it stand. Today a dropped
    /// [`PlacedExecutor`](crate::PlacedExecutor) cancels its calls not yet
    /// started; a call that was running is not stopped, and ends as it
    /// would have. Where a call's parts end differently, a failed part
    /// outranks a cancelled one: the call ends `Failed`.
    ///
    /// A variant, not a reason of [`Failed`](Self::Failed): any executor
    /// could fail with the same text, and the compiler cannot check a match
    /// on a string. The state of the art types it too: tokio's
    /// `JoinError::is_cancelled`, Java's `CancellationException`, Python's
    /// `CancelledError`.
    Cancelled,
}

/// The reason [`wait`] and the async ways to take a reply give for a
/// cancelled call, and the failure an executor records for a reply of its
/// own that ends cancelled: where only text fits. It names no cause, since
/// the executor dropped may be this one or one below it.
pub(crate) const CANCELLATION: &str = "the call was cancelled before it ran to its end";

impl ReplyStatus {
    /// How the reply ended, as [`wait`] gives it: `Ok` when done, else the
    /// failure's reason, or [`CANCELLATION`] for a cancelled call; `None`
    /// while it is pending.
    pub(crate) fn into_end(self) -> Option<Result<(), String>> {
        match self {
            Self::Pending => None,
            Self::Done => Some(Ok(())),
            Self::Failed(reason) => Some(Err(reason)),
            Self::Cancelled => Some(Err(CANCELLATION.to_owned())),
        }
    }

    /// The failure an executor records when a reply of its own ends so: a
    /// failure's reason, or [`CANCELLATION`] for a cancelled call, which
    /// fails the executor as a failure does unless a rule says otherwise;
    /// `None` while it is pending, and once it is done.
    pub(crate) fn failure(&self) -> Option<&str> {
        match self {
            Self::Pending | Self::Done => None,
            Self::Failed(reason) => Some(reason),
            Self::Cancelled => Some(CANCELLATION),
        }
    }
}

/// The events of one call, however they arrive: all at once when the work
/// ran inside the call, or piece by piece when it runs elsewhere.
///
/// One trait serves every way of making a reply and every way of taking it,
/// so the executor and its caller choose independently, and a driver written
/// once runs on every chain:
///
/// - The executor chooses how the events are made: [`ImmediateReply`] when the
///   work ran inside `call`; [`reply_channel`] when it runs elsewhere (a worker
///   thread, a device, a remote link), whose [`ReplySender`] pushes events into
///   the [`IncrementalReply`] as they are ready; [`relay`] passes an
///   inner reply's events up from a worker; [`MappedReply`] converts them on
///   the way.
/// - The caller, which may be an adapter, chooses how to take them, and
///   whether a thread waits meanwhile:
///
/// | The caller                                  | Takes the reply with                                                             | A thread blocked?  |
/// | ------------------------------------------- | -------------------------------------------------------------------------------- | ------------------ |
/// | a synchronous caller                        | [`wait`]: the whole reply, the thread parked between polls                       | yes, by its choice |
/// | a loop that does other work                 | [`poll_events`](Reply::poll_events) with its own waker, or [`Waker::noop`]       | no                 |
/// | an async caller, the whole reply            | [`wait_async`](crate::wait_async): a `Future`                                    | no                 |
/// | an async caller, the events as they come    | [`async_events`](crate::async_events): a `Stream` of event batches (streaming)   | no                 |
///
/// The choice stays the caller's only while every executor downstream returns
/// from `call` without waiting for an inner reply: an adapter that calls
/// [`wait`] inside its own `call`, because it needs a result before it can go
/// on, blocks its caller's thread for that time, whichever way the caller
/// then takes the reply.
///
/// A reply is polled and never blocks. After a poll that returns
/// [`ReplyStatus::Pending`], it keeps the waker of that poll (the latest one
/// replaces the others) and wakes it once a new poll could deliver more
/// events or end the reply. The wake is needed because without it a caller
/// that wants the events could only poll again and again: the executor knows
/// when its events are ready, the caller knows when it wants them, and the
/// waker joins the two.
///
/// So a caller that gives a waker never needs to poll in a loop: it polls,
/// and on `Pending` sleeps (its thread parks, or its task yields) until the
/// wake; then it polls again, without blocking, and takes every event made
/// since its last poll. One wake can stand for many events: events that
/// arrive while the caller has not polled yet are taken together, at its
/// next poll. The crate's `docs/replies.md`, §The waking model, walks
/// through the cycle.
///
/// The caller does not poll a reply again after it ends (`Done`, `Failed`
/// or `Cancelled`), because its end is delivered once. What a reply does
/// when polled again is not promised; as written today, an
/// [`IncrementalReply`] returns `Pending` and never wakes, so a caller
/// waiting on it would hang, and an [`ImmediateReply`] that failed returns
/// `Done`. [`wait`] returns at the end; [`wait_async`](crate::wait_async)
/// and [`async_events`](crate::async_events) keep this rule for their own
/// callers.
///
/// Dropping a reply, or a future or stream over it, does not stop its work:
/// the executor still pushes its events and ends it, and nobody takes them.
/// Dropping an executor may stop work: a dropped
/// [`PlacedExecutor`](crate::PlacedExecutor) cancels its calls not yet
/// started, whose replies end [`ReplyStatus::Cancelled`]. Cancellation, at an
/// executor's `close`, is planned, with no milestone yet, for a caller that
/// stops a run before its batches end (a user who interrupts a long run
/// from Python).
pub trait Reply {
    /// The event this reply delivers.
    type Item;

    /// Moves the events available now to the end of `out`, leaving what `out`
    /// already held as it was. Never blocks.
    fn poll_events(&mut self, out: &mut Vec<Self::Item>, waker: &Waker) -> ReplyStatus;
}
