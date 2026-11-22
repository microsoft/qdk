// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Waiting for a future on this thread: it sleeps until woken, and loses
//! no wake.

use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::Duration;

use crate::composition::runner::wait::wait;

/// How long a test waits for a thread before it fails, rather than hangs.
const PATIENT: Duration = Duration::from_secs(20);

/// Runs `f` on a thread of its own, and fails the test if it does not end
/// soon: a lost wake would park its thread forever.
fn ends_soon<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || sender.send(f()).expect("the test waits"));
    receiver
        .recv_timeout(PATIENT)
        .expect("it never ended: a wake was lost")
}

/// `future`, counting how often it is polled into `polls`.
struct Counted<F> {
    future: Pin<Box<F>>,
    polls: Arc<AtomicUsize>,
}

impl<F: Future> Future for Counted<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<F::Output> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        self.future.as_mut().poll(context)
    }
}

fn counted<F: Future>(future: F) -> (Counted<F>, Arc<AtomicUsize>) {
    let polls = Arc::new(AtomicUsize::new(0));
    let counted = Counted {
        future: Box::pin(future),
        polls: Arc::clone(&polls),
    };
    (counted, polls)
}

/// A value another thread hands over, and the waker to wake then.
#[derive(Default)]
struct Handover {
    value: Option<u32>,
    waker: Option<Waker>,
}

type Shared = Arc<Mutex<Handover>>;

/// Hands `value` over through `shared`, and wakes the waker it holds.
fn hand_over(shared: &Shared, value: u32) {
    let mut handover = shared.lock().expect("no panic while held");
    handover.value = Some(value);
    if let Some(waker) = handover.waker.take() {
        waker.wake();
    }
}

/// The value handed over through `shared`, pending until then.
async fn handed_over(shared: Shared) -> u32 {
    std::future::poll_fn(|context| {
        let mut handover = shared.lock().expect("no panic while held");
        if let Some(value) = handover.value.take() {
            Poll::Ready(value)
        } else {
            handover.waker = Some(context.waker().clone());
            Poll::Pending
        }
    })
    .await
}

/// A value a thread hands over 30 ms after the first poll: the wait polls
/// twice, pending then ready, and sleeps between, not polling while it
/// waits.
#[test]
fn a_wait_sleeps_until_woken() {
    let polls = ends_soon(|| {
        let shared = Shared::default();
        let (go, started) = mpsc::channel::<()>();
        let worker = {
            let shared = Arc::clone(&shared);
            thread::spawn(move || {
                started.recv().expect("the first poll starts the worker");
                thread::sleep(Duration::from_millis(30));
                hand_over(&shared, 7);
            })
        };
        let mut handed = Box::pin(handed_over(shared));
        let mut go = Some(go);
        let (future, polls) = counted(std::future::poll_fn(move |context| {
            let poll = handed.as_mut().poll(context);
            if let (Poll::Pending, Some(go)) = (&poll, go.take()) {
                go.send(()).expect("the worker waits");
            }
            poll
        }));
        assert_eq!(wait(future), 7);
        worker.join().expect("the worker ends");
        polls.load(Ordering::SeqCst)
    });
    assert_eq!(polls, 2);
}

/// A future woken during its first poll, before it returns `Pending`: the
/// wait polls it again at once, rather than sleeping forever.
#[test]
fn a_wake_during_the_poll_is_not_lost() {
    let polls = ends_soon(|| {
        let mut first = true;
        let (future, polls) = counted(std::future::poll_fn(move |context| {
            if first {
                first = false;
                context.waker().wake_by_ref();
                return Poll::Pending;
            }
            Poll::Ready(())
        }));
        wait(future);
        polls.load(Ordering::SeqCst)
    });
    assert_eq!(polls, 2);
}

/// A thread that hands the value over and wakes the waiting thread while
/// the poll still runs, before the thread sleeps: the wake is not lost.
#[test]
fn a_wake_from_another_thread_before_the_sleep_is_not_lost() {
    let polls = ends_soon(|| {
        let shared = Shared::default();
        let (go, started) = mpsc::channel::<()>();
        let (finished, ended) = mpsc::channel::<()>();
        let producer = {
            let shared = Arc::clone(&shared);
            thread::spawn(move || {
                started.recv().expect("the poll starts the producer");
                hand_over(&shared, 1);
                finished.send(()).expect("the poll waits");
            })
        };
        let mut handed = Box::pin(handed_over(shared));
        let mut go = Some(go);
        let (future, polls) = counted(std::future::poll_fn(move |context| {
            let poll = handed.as_mut().poll(context);
            if let (Poll::Pending, Some(go)) = (&poll, go.take()) {
                go.send(()).expect("the producer waits");
                ended.recv().expect("the producer ends");
            }
            poll
        }));
        assert_eq!(wait(future), 1);
        producer.join().expect("the producer ends");
        polls.load(Ordering::SeqCst)
    });
    assert_eq!(polls, 2, "the sleep ended at once, on the wake");
}

/// A future that, inside its poll, waits for another future of its own,
/// while a thread wakes the outer one: the inner wait takes nothing of
/// the outer wake, and both end.
#[test]
fn a_wait_inside_a_wait_keeps_each_wake() {
    let polls = ends_soon(|| {
        let mut first = true;
        let (future, polls) = counted(std::future::poll_fn(move |context| {
            if !first {
                return Poll::Ready(());
            }
            first = false;
            let outer = context.waker().clone();
            let inner = Shared::default();
            let worker = {
                let inner = Arc::clone(&inner);
                thread::spawn(move || {
                    outer.wake();
                    thread::sleep(Duration::from_millis(20));
                    hand_over(&inner, 3);
                })
            };
            assert_eq!(wait(handed_over(inner)), 3);
            worker.join().expect("the worker ends");
            Poll::Pending
        }));
        wait(future);
        polls.load(Ordering::SeqCst)
    });
    assert_eq!(polls, 2);
}
