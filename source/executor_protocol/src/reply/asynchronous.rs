// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Replies for async drivers: a whole reply as a [`Future`], or its events
//! as a [`Stream`].
//!
//! The four ways to take a reply: all of it, with [`wait`](crate::wait)
//! (sync) or [`wait_async`] (async); as its events arrive, with
//! [`Reply::poll_events`] (sync) or [`async_events`] (async).

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;

use crate::Reply;

/// `reply` as a [`Future`] of all its events: what an async driver
/// awaits where a synchronous one calls [`wait`](crate::wait). Its output is
/// the events and how the run ended, as [`wait`](crate::wait) gives it (a
/// cancelled call ends in an error that says so); a failure keeps the
/// events delivered before it.
///
/// No pinning is needed: [`Reply::poll_events`] takes `&mut self`, so
/// the future is [`Unpin`] whatever it holds.
pub fn wait_async<R: Reply>(reply: R) -> WaitAsync<R> {
    WaitAsync {
        reply: Some(reply),
        events: Vec::new(),
    }
}

/// The future of [`wait_async`], over the reply `R`.
pub struct WaitAsync<R: Reply> {
    reply: Option<R>,
    events: Vec<R::Item>,
}

impl<R: Reply> Unpin for WaitAsync<R> {}

impl<R: Reply> Future for WaitAsync<R> {
    type Output = (Vec<R::Item>, Result<(), String>);

    /// Polls the reply with the task's waker. Polled again after it was
    /// ready, it is ready at once with no events and an error, rather than
    /// panicking.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let Some(reply) = this.reply.as_mut() else {
            return Poll::Ready((Vec::new(), Err("polled after it was ready".into())));
        };
        let Some(end) = reply.poll_events(&mut this.events, cx.waker()).into_end() else {
            return Poll::Pending;
        };
        this.reply = None;
        Poll::Ready((std::mem::take(&mut this.events), end))
    }
}

/// `reply` as a [`Stream`] of event batches: each item is the events
/// one poll found (never none), and a failure is the last item, `Err`, as
/// is a cancellation, with a reason that says so (as [`wait`](crate::wait)
/// gives it).
/// `StreamExt::next` (from `futures` or `futures-util`) gives a future per
/// batch.
///
/// A `Stream` because Rust developers expect an async sequence to be one, so
/// that the ecosystem's combinators apply; `futures-core` is small,
/// runtime-free, and adds no cost per poll.
pub fn async_events<R: Reply>(reply: R) -> AsyncEvents<R> {
    AsyncEvents {
        reply: Some(reply),
        failure: None,
    }
}

/// The stream of [`async_events`], over the reply `R`.
pub struct AsyncEvents<R> {
    reply: Option<R>,
    failure: Option<String>,
}

impl<R> Unpin for AsyncEvents<R> {}

impl<R: Reply> Stream for AsyncEvents<R> {
    type Item = Result<Vec<R::Item>, String>;

    /// The next events: `Ready(Some(Ok(events)))` with what is there,
    /// `Pending` (the waker kept) while nothing is, `Ready(Some(Err(reason)))`
    /// once if the run failed or was cancelled, then `Ready(None)`.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let Some(reply) = this.reply.as_mut() else {
            return Poll::Ready(this.failure.take().map(Err));
        };
        let mut batch = Vec::new();
        match reply.poll_events(&mut batch, cx.waker()).into_end() {
            None if batch.is_empty() => return Poll::Pending,
            None => {}
            Some(end) => {
                this.reply = None;
                this.failure = end.err();
            }
        }
        if batch.is_empty() {
            return Poll::Ready(this.failure.take().map(Err));
        }
        Poll::Ready(Some(Ok(batch)))
    }
}
