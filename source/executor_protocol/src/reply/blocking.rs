// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The blocking way to take a whole reply: [`wait`], and [`wait_status`],
//! which gives its end as it is, for the crate's own checks; the pair of
//! [`wait_async`](crate::wait_async). Between polls the caller's thread
//! parks on its [`Parker`] (the module [`parker`](super::parker) says how
//! the waking works).

use super::parker::PARKER;
use crate::{Reply, ReplyStatus};

#[cfg(doc)]
use super::parker::Parker;
#[cfg(doc)]
use crate::relay;

/// Blocks until `reply` ends, appending its events to `out`: the way a
/// synchronous caller, such as an adapter that needs a result before it
/// can go on, takes a whole reply.
///
/// It is the one way for a caller to take a reply that blocks the caller's
/// thread, and the caller chooses it ([`relay`] blocks too, but a worker's
/// thread, not the caller's). A caller that must not block takes the reply
/// otherwise (see [`Reply`]): by polling it with its own waker, as a
/// [`Future`] with [`wait_async`](crate::wait_async),
/// or as a `Stream` with [`async_events`](crate::async_events). An adapter
/// that calls `wait` inside its own `call` blocks its caller's thread for
/// that time, whichever way the caller takes the reply.
///
/// Between polls the thread parks, and the reply's waker unparks it, so
/// waiting for work done elsewhere uses no CPU. A wake that comes before the
/// thread parks is not lost: the waker sets a flag before it unparks, and
/// `wait` parks only while the flag is clear. The flag is needed because the
/// park token alone is not enough: a poll that itself blocks on something
/// that parks (a channel, a mutex) can use the token up.
///
/// Do not call it from inside a `poll_events`: `wait` and [`relay`] share
/// one parker per thread, so a nested call can take a wake meant for the
/// outer one, which then parks with no waker left. A poll must not block.
///
/// # Errors
///
/// The reason of [`ReplyStatus::Failed`], after the events delivered before
/// it; for [`ReplyStatus::Cancelled`], a reason that says the call was
/// cancelled. To tell a cancellation from a failure, poll the reply.
//
// Known issue, open (blocks neither): `wait`, `wait_async` and
// `async_events` give a cancellation as text, as they give a failure; a
// typed end would change their error type at every call site.
pub fn wait<R: Reply>(reply: R, out: &mut Vec<R::Item>) -> Result<(), String> {
    (wait_status(reply, out).into_end()).expect("`wait_status` gives a reply's end, not `Pending`")
}

/// [`wait`], giving the reply's end as it is, not as text: for the crate's
/// own checks that tell a cancellation from a failure.
//
// Design question, open (blocks neither this commit nor a later one):
// making it public, as the prototype's `wait_status`, would let a caller
// of `wait` tell a cancellation from a failure without a parking loop of
// its own (see the known issue above).
pub(crate) fn wait_status<R: Reply>(mut reply: R, out: &mut Vec<R::Item>) -> ReplyStatus {
    PARKER.with(|parker| {
        loop {
            match reply.poll_events(out, &parker.waker) {
                ReplyStatus::Pending => parker.park(),
                end => return end,
            }
        }
    })
}
