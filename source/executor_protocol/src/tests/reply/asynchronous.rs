// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::sync::mpsc;
use std::task::{Context, Poll};
use std::thread;

use futures_core::Stream;

use crate::reply::CANCELLATION;
use crate::tests::{PATIENT, YieldNow, block_on, counting, ends_soon, outcome, run_tasks, wakes};
use crate::{ImmediateReply, ShotOutcome, async_events, reply_channel, wait_async};

/// An async driver awaits a whole reply, fed from another thread.
#[test]
fn wait_async_is_a_future_of_the_whole_reply() {
    let (sender, reply) = reply_channel();
    let feeder = thread::spawn(move || {
        sender.extend([1, 2]);
        sender.push(3);
        sender.done();
    });
    assert_eq!(block_on(wait_async(reply)), (vec![1, 2, 3], Ok(())));
    assert!(feeder.join().is_ok());
}

/// A reply that fails gives the events before the failure, then the
/// failure.
#[test]
fn wait_async_gives_the_events_before_a_failure_then_the_failure() {
    assert_eq!(
        block_on(wait_async(ImmediateReply::failed(vec![4], "lost".into()))),
        (vec![4], Err("lost".into()))
    );
}

/// A cancelled reply gives the events before the cancellation, then an
/// error that says the call was cancelled, as `wait` does: an async driver
/// learns that its call did not complete, as for a failure. Chosen so that
/// the whole reply keeps one error type for every way a call does not
/// complete; a driver that tells a cancellation from a failure polls the
/// reply, whose end is typed.
#[test]
fn wait_async_gives_a_cancelled_reply_as_an_error() {
    let (sender, reply) = reply_channel();
    sender.push(4);
    sender.cancel();
    assert_eq!(
        block_on(wait_async(reply)),
        (vec![4], Err(CANCELLATION.to_owned()))
    );
}

/// A `wait_async` polled again after it was ready does not panic: it says
/// so, with no events.
#[test]
fn wait_async_polled_after_it_was_ready_does_not_panic() {
    let mut ready = wait_async(ImmediateReply::done(vec![4]));
    let (_, waker) = counting();
    let mut context = Context::from_waker(&waker);
    assert_eq!(
        Pin::new(&mut ready).poll(&mut context),
        Poll::Ready((vec![4], Ok(())))
    );
    assert_eq!(
        Pin::new(&mut ready).poll(&mut context),
        Poll::Ready((Vec::new(), Err("polled after it was ready".into())))
    );
}

fn next<S: Stream + Unpin>(stream: &mut S, context: &mut Context<'_>) -> Poll<Option<S::Item>> {
    Pin::new(stream).poll_next(context)
}

/// The stream is pending while no event has come, and wakes its task when
/// one does; each poll that finds events gives them as one item.
#[test]
fn async_events_gives_each_batch_of_events_as_it_comes() {
    let (count, waker) = counting();
    let mut context = Context::from_waker(&waker);
    let (sender, reply) = reply_channel();
    let mut events = async_events(reply);
    assert_eq!(next(&mut events, &mut context), Poll::Pending);
    sender.extend([1, 2]);
    assert_eq!(wakes(&count), 1);
    assert_eq!(
        next(&mut events, &mut context),
        Poll::Ready(Some(Ok(vec![1, 2])))
    );
    assert_eq!(next(&mut events, &mut context), Poll::Pending);
    sender.push(3);
    assert_eq!(wakes(&count), 2);
    assert_eq!(
        next(&mut events, &mut context),
        Poll::Ready(Some(Ok(vec![3])))
    );
}

/// A failure is the stream's last item, after the events before it; then
/// the stream ends.
#[test]
fn async_events_gives_a_failure_last_then_ends() {
    let (count, waker) = counting();
    let mut context = Context::from_waker(&waker);
    let mut events = async_events(ImmediateReply::failed(vec![3], "stopped".into()));
    assert_eq!(
        next(&mut events, &mut context),
        Poll::Ready(Some(Ok(vec![3])))
    );
    assert_eq!(
        next(&mut events, &mut context),
        Poll::Ready(Some(Err("stopped".into())))
    );
    assert_eq!(next(&mut events, &mut context), Poll::Ready(None));
    assert_eq!(wakes(&count), 0);
}

/// A cancellation is the stream's last item, an error that says the call
/// was cancelled, after the events before it; then the stream ends, as
/// after a failure. Chosen so that the stream keeps one error type for
/// every way a call does not complete; a driver that tells a cancellation
/// from a failure polls the reply, whose end is typed.
#[test]
fn async_events_gives_a_cancellation_last_then_ends() {
    let (_, waker) = counting();
    let mut context = Context::from_waker(&waker);
    let (sender, reply) = reply_channel();
    sender.push(3);
    sender.cancel();
    let mut events = async_events(reply);
    assert_eq!(
        next(&mut events, &mut context),
        Poll::Ready(Some(Ok(vec![3])))
    );
    assert_eq!(
        next(&mut events, &mut context),
        Poll::Ready(Some(Err(CANCELLATION.to_owned())))
    );
    assert_eq!(next(&mut events, &mut context), Poll::Ready(None));
}

/// A reply that ends with no events gives no empty item: the stream ends.
#[test]
fn async_events_gives_no_empty_batch() {
    let (_, waker) = counting();
    let mut context = Context::from_waker(&waker);
    let mut done = async_events(ImmediateReply::done(Vec::<u32>::new()));
    assert_eq!(next(&mut done, &mut context), Poll::Ready(None));
}

// ------------------------------------------------------------- an async driver

/// An async task that awaits a call leaves its thread to the other tasks
/// until the call ends: task A awaits a reply that ends only after task B's
/// fifth step, and B runs on the same thread meanwhile. Run
/// on a thread of its own, so that a task that blocked its thread deadlocks
/// A and B and fails the test instead of hanging it.
#[test]
fn an_async_task_awaiting_a_call_leaves_its_thread_to_other_tasks() {
    ends_soon(two_tasks_on_one_thread);
}

fn two_tasks_on_one_thread() {
    let (sender, reply) = reply_channel::<ShotOutcome>();
    let (go, gate) = mpsc::channel::<()>();
    let producer = thread::spawn(move || {
        gate.recv_timeout(PATIENT).expect("task B lets it go");
        sender.push(outcome(0, 7));
        sender.done();
    });
    let log = RefCell::new(Vec::new());
    let a = async {
        let (got, end) = wait_async(reply).await;
        assert_eq!((got, end), (vec![outcome(0, 7)], Ok(())));
        log.borrow_mut().push("A done".to_string());
    };
    let b = async {
        for step in 0..10 {
            log.borrow_mut().push(format!("B {step}"));
            if step == 4 {
                go.send(()).expect("the producer waits for it"); // A's call may now end
            }
            YieldNow(false).await;
        }
        log.borrow_mut().push("B done".to_string());
    };
    run_tasks(vec![Box::pin(a), Box::pin(b)], &log);
    assert!(producer.join().is_ok());
    let log = log.into_inner();
    let at = |entry: &str| {
        log.iter()
            .position(|e| e == entry)
            .unwrap_or_else(|| panic!("no {entry}: {log:?}"))
    };
    assert_eq!(
        log[0], "B 0",
        "A was polled first, and was pending: {log:?}"
    );
    assert!(at("B 4") < at("A done"), "B ran while A awaited: {log:?}");
    assert!(
        !log[..at("B done")].iter().any(|e| e == "park"),
        "the thread slept while B had work: {log:?}"
    );
}
