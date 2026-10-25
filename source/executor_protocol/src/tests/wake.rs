// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::future::Future;
use std::pin::{Pin, pin};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;

use futures_core::Stream;

use crate::{
    Completion, Immediate, Lifecycle, Map, Rejected, Status, async_events, completer, relay, wait,
    wait_async,
};

/// A waker that counts its wakes.
#[derive(Default)]
struct CountWakes(AtomicUsize);

impl Wake for CountWakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn counting() -> (Arc<CountWakes>, Waker) {
    let count = Arc::new(CountWakes::default());
    (Arc::clone(&count), Waker::from(count))
}

fn wakes(count: &CountWakes) -> usize {
    count.0.load(Ordering::SeqCst)
}

/// A reply made elsewhere hands over what was pushed, wakes the caller's
/// latest waker once per push after a pending poll, and never blocks.
#[test]
fn a_completer_wakes_its_caller_once_per_push() {
    let (count, waker) = counting();
    let (completer, mut reply) = completer();
    let mut out = Vec::new();
    assert_eq!(reply.poll_events(&mut out, &waker), Status::Pending);
    completer.push(1);
    completer.extend(Vec::new());
    assert_eq!(wakes(&count), 1, "an empty extend does not wake");
    completer.push(2);
    assert_eq!(wakes(&count), 1, "the waker is taken by the first wake");
    assert_eq!(reply.poll_events(&mut out, &waker), Status::Pending);
    assert_eq!(out, [1, 2]);
    completer.append(&mut vec![3, 4]);
    completer.done();
    assert_eq!(wakes(&count), 2);
    assert_eq!(reply.poll_events(&mut out, &waker), Status::Done);
    assert_eq!(out, [1, 2, 3, 4]);
}

/// A worker that stops without ending its reply ends it with a failure, so
/// that its caller is not left waiting.
#[test]
fn a_dropped_completer_fails_its_reply() {
    let (count, waker) = counting();
    let (completer, mut reply) = completer::<u32>();
    let mut out = Vec::new();
    assert_eq!(reply.poll_events(&mut out, &waker), Status::Pending);
    let worker = thread::spawn(move || {
        completer.push(5);
        drop(completer);
    });
    assert!(worker.join().is_ok());
    assert_eq!(wakes(&count), 1);
    assert_eq!(
        reply.poll_events(&mut out, &waker),
        Status::Failed("the executor dropped the reply before its end".into())
    );
    assert_eq!(out, [5]);
}

/// `relay` passes an inner reply's events up from a worker thread, and
/// `wait` takes them on the caller's thread, parked between pushes.
#[test]
fn relay_and_wait_pass_events_across_threads() {
    let (inner_completer, inner) = completer();
    let (outer_completer, outer) = completer();
    let worker = thread::spawn(move || relay(inner, outer_completer));
    let feeder = thread::spawn(move || {
        for event in 0..100 {
            inner_completer.push(event);
            thread::yield_now();
        }
        inner_completer.fail("stopped".into());
    });
    let mut out = Vec::new();
    assert_eq!(wait(outer, &mut out), Err("stopped".into()));
    assert_eq!(out, (0..100).collect::<Vec<_>>());
    assert!(worker.join().is_ok() && feeder.join().is_ok());
}

/// A minimal executor of one future, for the tests: polls it with a waker
/// that unparks this thread.
fn block_on<F: Future>(future: F) -> F::Output {
    struct Unpark(thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        thread::park();
    }
}

/// An async driver awaits a whole reply, fed from another thread.
#[test]
fn wait_async_is_a_future_of_the_whole_reply() {
    let (completer, reply) = completer();
    let feeder = thread::spawn(move || {
        completer.extend([1, 2]);
        completer.push(3);
        completer.done();
    });
    assert_eq!(block_on(wait_async(reply)), (vec![1, 2, 3], Ok(())));
    assert!(feeder.join().is_ok());
    let mut ready = wait_async(Immediate::failed(vec![4], "lost".into()));
    let (_, waker) = counting();
    let mut context = Context::from_waker(&waker);
    assert_eq!(
        Pin::new(&mut ready).poll(&mut context),
        Poll::Ready((vec![4], Err("lost".into())))
    );
    assert_eq!(
        Pin::new(&mut ready).poll(&mut context),
        Poll::Ready((Vec::new(), Err("polled after it was ready".into()))),
        "polled again, it does not panic"
    );
}

fn next<S: Stream + Unpin>(stream: &mut S, context: &mut Context<'_>) -> Poll<Option<S::Item>> {
    Pin::new(stream).poll_next(context)
}

/// An async driver takes a reply's events as they come: one item per poll
/// that found some, pending (and woken) in between, a failure last.
#[test]
fn async_events_is_a_stream_of_event_batches() {
    let (count, waker) = counting();
    let mut context = Context::from_waker(&waker);
    let (completer, reply) = completer();
    let mut events = async_events(reply);
    assert_eq!(next(&mut events, &mut context), Poll::Pending);
    completer.extend([1, 2]);
    assert_eq!(wakes(&count), 1);
    assert_eq!(
        next(&mut events, &mut context),
        Poll::Ready(Some(Ok(vec![1, 2])))
    );
    assert_eq!(next(&mut events, &mut context), Poll::Pending);
    completer.push(3);
    completer.fail("stopped".into());
    assert_eq!(
        next(&mut events, &mut context),
        Poll::Ready(Some(Ok(vec![3])))
    );
    assert_eq!(
        next(&mut events, &mut context),
        Poll::Ready(Some(Err("stopped".into())))
    );
    assert_eq!(next(&mut events, &mut context), Poll::Ready(None));
    let mut done = async_events(Immediate::done(Vec::<u32>::new()));
    assert_eq!(
        next(&mut done, &mut context),
        Poll::Ready(None),
        "no empty batch"
    );
}

#[test]
fn map_converts_each_event_and_keeps_the_end() {
    let mut out = Vec::new();
    let doubled = Map::new(Immediate::failed(vec![1, 2], "lost".into()), |x: u32| x * 2);
    assert_eq!(wait(doubled, &mut out), Err("lost".into()));
    assert_eq!(out, [2, 4]);
}

/// A session refuses every call after its first failure, a closed one says
/// so first, and a reply still in flight ends at an earlier failure.
#[test]
fn a_failure_ends_the_session_and_the_calls_in_flight() {
    let (_, waker) = counting();
    let mut lifecycle = Lifecycle::default();
    assert_eq!(lifecycle.admit(), Ok(()));
    let (first_completer, first) = completer::<u32>();
    let mut first = lifecycle.bind(first);
    assert_eq!(lifecycle.admit(), Ok(()));
    let (second_completer, second) = completer::<u32>();
    let mut second = lifecycle.bind(second);
    let mut out = Vec::new();
    assert_eq!(second.poll_events(&mut out, &waker), Status::Pending);
    first_completer.fail("lost".into());
    assert_eq!(
        first.poll_events(&mut out, &waker),
        Status::Failed("lost".into())
    );
    second_completer.push(1);
    assert_eq!(
        second.poll_events(&mut out, &waker),
        Status::Failed("an earlier submission failed: lost".into()),
        "the later call ends with no more events"
    );
    assert!(out.is_empty());
    assert_eq!(
        lifecycle.admit(),
        Err(Rejected::SessionFailed("lost".into()))
    );
    lifecycle.close();
    assert!(lifecycle.is_closed());
    assert_eq!(lifecycle.admit(), Err(Rejected::Closed));
}

/// Failures seen out of order keep the earliest call's reason.
#[test]
fn the_earliest_failed_call_is_the_sessions_failure() {
    let mut lifecycle = Lifecycle::default();
    assert_eq!(lifecycle.admit(), Ok(()));
    let first = lifecycle.handle();
    assert_eq!(lifecycle.admit(), Ok(()));
    lifecycle.fail("second");
    first.fail("first");
    assert_eq!(
        first.earlier_failure(),
        None,
        "a call is not earlier than itself"
    );
    assert_eq!(
        lifecycle.admit(),
        Err(Rejected::SessionFailed("first".into()))
    );
}
