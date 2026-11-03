// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::task::Waker;
use std::thread;
use std::time::Duration;

use crate::reply::CANCELLATION;
use crate::{ImmediateReply, IncrementalReply, Reply, ReplyStatus, reply_channel, wait};

use crate::tests::{counted, ends_after, ends_soon};

/// What a test double's two ends share: the events pushed so far, whether
/// the run ended, and the waker of the last pending poll.
#[derive(Default)]
struct Shared {
    events: Vec<u32>,
    ended: bool,
    waker: Option<Waker>,
}

/// A reply whose events another thread pushes, as a worker would.
struct TestDoubleReply(Arc<Mutex<Shared>>);

impl Reply for TestDoubleReply {
    type Item = u32;

    fn poll_events(&mut self, out: &mut Vec<u32>, waker: &Waker) -> ReplyStatus {
        let mut shared = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        out.append(&mut shared.events);
        if shared.ended {
            ReplyStatus::Done
        } else {
            shared.waker = Some(waker.clone());
            ReplyStatus::Pending
        }
    }
}

fn push(shared: &Mutex<Shared>, event: Option<u32>) {
    let waker = {
        let mut shared = shared.lock().unwrap_or_else(PoisonError::into_inner);
        match event {
            Some(event) => shared.events.push(event),
            None => shared.ended = true,
        }
        shared.waker.take()
    };
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// `wait` returns every event another thread pushed, in order, once the
/// reply ends.
#[test]
fn wait_returns_the_events_pushed_by_another_thread() {
    let shared = Arc::new(Mutex::new(Shared::default()));
    let worker = {
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || {
            for event in 0..100 {
                push(&shared, Some(event));
            }
            push(&shared, None);
        })
    };
    let mut out = Vec::new();
    assert_eq!(wait(TestDoubleReply(shared), &mut out), Ok(()));
    assert_eq!(out, (0..100).collect::<Vec<_>>());
    assert!(worker.join().is_ok());
}

/// `wait` returns a failure after the events pushed before it.
#[test]
fn wait_returns_the_failure_after_the_events_before_it() {
    let mut out = Vec::new();
    assert_eq!(
        wait(ImmediateReply::failed(vec![7], "lost".into()), &mut out),
        Err("lost".into())
    );
    assert_eq!(out, [7]);
}

/// `wait` gives a cancelled reply as an error, after the events delivered
/// before it: a synchronous caller that takes the whole reply learns that
/// its call did not complete, as for a failure, by a reason that says it
/// was cancelled. Chosen so that `wait` keeps one error type for every
/// way a call does not complete; a caller that tells a cancellation from a
/// failure polls the reply, whose end is typed.
#[test]
fn wait_gives_a_cancelled_reply_as_an_error() {
    let (sender, reply) = reply_channel();
    sender.push(7);
    sender.cancel();
    let mut out = Vec::new();
    assert_eq!(wait(reply, &mut out), Err(CANCELLATION.to_owned()));
    assert_eq!(out, [7]);
}

/// A `wait` that spins: poll, `yield_now`, poll again. The baseline against
/// which `wait`'s parking is measured.
fn spin_wait<R: Reply>(mut reply: R, out: &mut Vec<R::Item>) -> Result<(), String> {
    loop {
        match reply.poll_events(out, Waker::noop()) {
            ReplyStatus::Done => return Ok(()),
            ReplyStatus::Failed(reason) => return Err(reason),
            ReplyStatus::Cancelled => return Err("cancelled".into()),
            ReplyStatus::Pending => thread::yield_now(),
        }
    }
}

/// `wait` parks until it is woken instead of spinning: a call that ends 30 ms
/// later is polled twice (once if it ended before the first poll), against
/// more when it spins.
#[test]
fn wait_parks_until_woken_instead_of_spinning() {
    let (reply, polls) = counted(ends_after(Duration::from_millis(30)));
    assert_eq!(wait(reply, &mut Vec::new()), Ok(()));
    let parked = polls.load(Ordering::SeqCst);
    assert!((1..=2).contains(&parked), "{parked} polls");

    let (reply, polls) = counted(ends_after(Duration::from_millis(30)));
    assert_eq!(spin_wait(reply, &mut Vec::new()), Ok(()));
    let spun = polls.load(Ordering::SeqCst);
    // The control only shows that the count tells spinning from parking. A
    // spinning caller yields between polls, so how often it polls depends on
    // the machine's load: hundreds of times in 30 ms when idle, 10 to 12 on
    // two cores shared with the rest of the suite; more than parking's two
    // in every run measured.
    assert!(spun > 2, "{spun} polls when spinning");
}

/// A reply that wakes its waker during its first poll, then is done.
struct WakesEarly(u32);

impl Reply for WakesEarly {
    type Item = u32;

    fn poll_events(&mut self, out: &mut Vec<u32>, waker: &Waker) -> ReplyStatus {
        self.0 += 1;
        if self.0 == 1 {
            waker.wake_by_ref();
            return ReplyStatus::Pending;
        }
        out.push(self.0);
        ReplyStatus::Done
    }
}

/// An incremental reply whose first pending poll lets a producer thread end
/// the reply, and returns only once it has: the wake comes before `wait`
/// parks.
struct Handshake {
    reply: IncrementalReply<u32>,
    go: Option<mpsc::Sender<()>>,
    ended: mpsc::Receiver<()>,
}

impl Reply for Handshake {
    type Item = u32;

    fn poll_events(&mut self, out: &mut Vec<u32>, waker: &Waker) -> ReplyStatus {
        let status = self.reply.poll_events(out, waker);
        if let (ReplyStatus::Pending, Some(go)) = (&status, self.go.take()) {
            go.send(()).expect("the producer waits for it");
            self.ended.recv().expect("the producer ends the reply");
        }
        status
    }
}

/// A wake that comes during `wait`'s poll, from the reply itself, is not
/// lost: `wait` polls again at once, without a third poll.
#[test]
fn a_wake_during_the_poll_is_not_lost() {
    let (polls, out) = ends_soon(|| {
        let (reply, polls) = counted(WakesEarly(0));
        let mut out = Vec::new();
        assert_eq!(wait(reply, &mut out), Ok(()));
        (polls.load(Ordering::SeqCst), out)
    });
    assert_eq!((polls, out), (2, vec![2]));
}

/// A wake from another thread after `wait`'s poll but before it parks is
/// not lost: the park returns at once, without a third poll.
#[test]
fn a_wake_from_another_thread_before_the_park_is_not_lost() {
    let polls = ends_soon(|| {
        let (sender, reply) = reply_channel();
        let (go, started) = mpsc::channel();
        let (finished, ended) = mpsc::channel();
        let producer = thread::spawn(move || {
            started.recv().expect("the caller polls first");
            sender.push(1);
            sender.done(); // wakes the caller, which has not parked yet
            finished.send(()).expect("the caller waits for it");
        });
        let (reply, polls) = counted(Handshake {
            reply,
            go: Some(go),
            ended,
        });
        let mut out = Vec::new();
        assert_eq!(wait(reply, &mut out), Ok(()));
        assert!(producer.join().is_ok());
        assert_eq!(out, [1]);
        polls.load(Ordering::SeqCst)
    });
    assert_eq!(
        polls, 2,
        "the park returned at once, on the flag the wake set"
    );
}

/// A reply whose first pending poll hands the polling thread to a
/// bystander thread, which unparks it with no wake.
struct HandsOverItsThread {
    reply: IncrementalReply<u32>,
    to: Option<mpsc::Sender<thread::Thread>>,
}

impl Reply for HandsOverItsThread {
    type Item = u32;

    fn poll_events(&mut self, out: &mut Vec<u32>, waker: &Waker) -> ReplyStatus {
        let status = self.reply.poll_events(out, waker);
        if let (ReplyStatus::Pending, Some(to)) = (&status, self.to.take()) {
            to.send(thread::current())
                .expect("the bystander waits for it");
        }
        status
    }
}

/// An unpark that comes with no wake (another part of the program unparks
/// the thread, or the park returns spuriously) costs `wait` no poll: it
/// parks again until the reply wakes it. Here two unparks come before the
/// reply's one wake, its end, so `wait` polls twice: once pending, once
/// at the end.
#[test]
fn an_unpark_without_a_wake_costs_wait_no_poll() {
    let polls = ends_soon(|| {
        let (sender, reply) = reply_channel();
        let (to, thread) = mpsc::channel();
        let bystander = thread::spawn(move || {
            let waiting: thread::Thread = thread.recv().expect("the caller polls first");
            waiting.unpark();
            thread::sleep(Duration::from_millis(30));
            waiting.unpark();
            thread::sleep(Duration::from_millis(30));
            sender.done(); // the reply's one wake
        });
        let (reply, polls) = counted(HandsOverItsThread {
            reply,
            to: Some(to),
        });
        assert_eq!(wait(reply, &mut Vec::new()), Ok(()));
        assert!(bystander.join().is_ok());
        polls.load(Ordering::SeqCst)
    });
    assert_eq!(polls, 2, "the unparks with no wake were not polls");
}
