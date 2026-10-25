// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Replies for async drivers: a whole reply as a [`Future`], or its events
//! as a [`Stream`].
//!
//! The four ways to take a reply: all of it, with [`wait`](crate::wait)
//! (sync) or [`wait_async`] (async); as its events arrive, with
//! [`Completion::poll_events`] (sync) or [`async_events`] (async).

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;

use crate::{Completion, Status};

/// `completion` as a [`Future`] of all its events: what an async driver
/// awaits where a synchronous one calls [`wait`](crate::wait). Its output is
/// the events and how the run ended; a failure keeps the events delivered
/// before it.
///
/// No pinning is needed: [`Completion::poll_events`] takes `&mut self`, so
/// the future is [`Unpin`] whatever it holds.
pub fn wait_async<C: Completion>(completion: C) -> WaitAsync<C> {
    WaitAsync {
        completion: Some(completion),
        events: Vec::new(),
    }
}

/// The future of [`wait_async`].
pub struct WaitAsync<C: Completion> {
    completion: Option<C>,
    events: Vec<C::Item>,
}

impl<C: Completion> Unpin for WaitAsync<C> {}

impl<C: Completion> Future for WaitAsync<C> {
    type Output = (Vec<C::Item>, Result<(), String>);

    /// Polls the completion with the task's waker. Polled again after it was
    /// ready, it is ready at once with no events and an error, rather than
    /// panicking.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let Some(completion) = this.completion.as_mut() else {
            return Poll::Ready((Vec::new(), Err("polled after it was ready".into())));
        };
        let end = match completion.poll_events(&mut this.events, cx.waker()) {
            Status::Pending => return Poll::Pending,
            Status::Done => Ok(()),
            Status::Failed(reason) => Err(reason),
        };
        this.completion = None;
        Poll::Ready((std::mem::take(&mut this.events), end))
    }
}

/// `completion` as a [`Stream`] of event batches: each item is the events
/// one poll found (never none), and a failure is the last item, `Err`.
/// `StreamExt::next` (from `futures` or `futures-util`) gives a future per
/// batch.
///
/// A `Stream` because Rust developers expect an async sequence to be one, so
/// that the ecosystem's combinators apply; `futures-core` is small,
/// runtime-free, and adds no cost per poll.
pub fn async_events<C: Completion>(completion: C) -> AsyncEvents<C> {
    AsyncEvents {
        completion: Some(completion),
        failure: None,
    }
}

/// The stream of [`async_events`].
pub struct AsyncEvents<C> {
    completion: Option<C>,
    failure: Option<String>,
}

impl<C> Unpin for AsyncEvents<C> {}

impl<C: Completion> Stream for AsyncEvents<C> {
    type Item = Result<Vec<C::Item>, String>;

    /// The next events: `Ready(Some(Ok(events)))` with what is there,
    /// `Pending` (the waker kept) while nothing is, `Ready(Some(Err(reason)))`
    /// once if the run failed, then `Ready(None)`.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let Some(completion) = this.completion.as_mut() else {
            return Poll::Ready(this.failure.take().map(Err));
        };
        let mut batch = Vec::new();
        match completion.poll_events(&mut batch, cx.waker()) {
            Status::Pending if batch.is_empty() => return Poll::Pending,
            Status::Pending => {}
            Status::Done => this.completion = None,
            Status::Failed(reason) => {
                this.completion = None;
                this.failure = Some(reason);
            }
        }
        if batch.is_empty() {
            return Poll::Ready(this.failure.take().map(Err));
        }
        Poll::Ready(Some(Ok(batch)))
    }
}
