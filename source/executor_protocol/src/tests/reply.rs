// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod asynchronous;

use std::future::poll_fn;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::task::{Poll, Wake, Waker};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use futures_core::Stream;

use crate::reply::CANCELLATION;
use crate::{
    Close, Executor, ImmediateReply, IncrementalReply, InstructionBatch, InstructionBatchRef,
    MappedReply, Rejected, Reply, ReplySender, ReplyStatus, ShotId, ShotOutcome, ShotSet,
    async_events, relay, reply_channel, wait, wait_async,
};

use super::{
    PATIENT, Worker, block_on, counted, counting, ends_soon, on_shot_0, outcome, signalling, wakes,
};

struct NoWake;

impl Wake for NoWake {
    fn wake(self: Arc<Self>) {}
}

fn no_wake() -> Waker {
    Waker::from(Arc::new(NoWake))
}

/// A reply made at once appends its events to the caller's, then ends as
/// it was made.
#[test]
fn an_immediate_reply_appends_its_events_then_ends() {
    let mut out = vec![1];
    assert_eq!(
        ImmediateReply::done(vec![2, 3]).poll_events(&mut out, &no_wake()),
        ReplyStatus::Done
    );
    assert_eq!(
        ImmediateReply::failed(vec![4], "stopped".into()).poll_events(&mut out, &no_wake()),
        ReplyStatus::Failed("stopped".into())
    );
    assert_eq!(out, [1, 2, 3, 4]);
}

/// A reply built event by event hands them over in the order pushed, then
/// ends; built with no event, it ends at once.
#[test]
fn an_immediate_reply_hands_over_the_events_pushed_in_order() {
    for count in [0, 1, 3] {
        let mut reply = ImmediateReply::default();
        (0..count).for_each(|event| reply.push(event));
        let mut out = vec![9];
        assert_eq!(reply.poll_events(&mut out, &no_wake()), ReplyStatus::Done);
        assert_eq!(
            out[1..],
            (0..count).collect::<Vec<_>>()[..],
            "{count} events"
        );
    }
}

/// The reply that ends a batch with one measurement's outcome, built event
/// by event, allocates no memory.
#[test]
fn an_immediate_reply_of_at_most_one_event_allocates_nothing() {
    for count in [0, 1] {
        let made = super::allocations(|| {
            let mut reply = ImmediateReply::default();
            (0..count).for_each(|event| reply.push(event));
            std::hint::black_box(reply);
        });
        assert_eq!(made, 0, "{count} events");
    }
}

/// Replies with the same events and the same end are equal, whether built
/// whole or event by event.
#[test]
fn immediate_replies_with_the_same_events_and_end_are_equal() {
    let pushed = |events: &[u32]| {
        let mut reply = ImmediateReply::default();
        for &event in events {
            reply.push(event);
        }
        reply
    };
    for events in [&[][..], &[4], &[4, 5]] {
        assert_eq!(pushed(events), ImmediateReply::done(events.to_vec()));
    }
    assert_ne!(pushed(&[4]), pushed(&[5]));
    assert_ne!(pushed(&[4]), ImmediateReply::failed(vec![4], "lost".into()));
}

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
/// before it: a synchronous driver that takes the whole reply learns that
/// its call did not complete, as for a failure, by a reason that says it
/// was cancelled. Chosen so that `wait` keeps one error type for every
/// way a call does not complete; a driver that tells a cancellation from a
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

/// A reply its sender cancels ends `ReplyStatus::Cancelled`, after the
/// events pushed before: the caller tells a call abandoned from one that
/// failed by the variant.
/// Typed, not a reason of `Failed`: any executor could fail with the same
/// text, and the compiler cannot check a match on a string; tokio
/// (`JoinError::is_cancelled`), Java (`CancellationException`) and Python
/// (`CancelledError`) type it too.
#[test]
fn a_cancelled_reply_ends_cancelled_after_the_events_before_it() {
    let (sender, mut reply) = reply_channel();
    sender.push(7);
    sender.cancel();
    let mut out = Vec::new();
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Cancelled
    );
    assert_eq!(out, [7]);
}

/// A poll hands over every event pushed since the last one, in order,
/// however it was pushed; the reply ends with the poll after its end.
#[test]
fn a_poll_hands_over_every_event_pushed_since_the_last_one() {
    let (sender, mut reply) = reply_channel();
    let mut out = Vec::new();
    assert_eq!(
        reply.poll_events(&mut out, &no_wake()),
        ReplyStatus::Pending
    );
    sender.push(1);
    sender.extend([2, 3]);
    sender.append(&mut vec![4]);
    assert_eq!(
        reply.poll_events(&mut out, &no_wake()),
        ReplyStatus::Pending
    );
    assert_eq!(out, [1, 2, 3, 4]);
    sender.push(5);
    sender.done();
    assert_eq!(reply.poll_events(&mut out, &no_wake()), ReplyStatus::Done);
    assert_eq!(out, [1, 2, 3, 4, 5]);
}

/// A caller whose poll found nothing is woken by the next push, so it can
/// sleep until then; one wake per pending poll, since the caller takes
/// every event pushed by then with its next poll.
#[test]
fn a_reply_sender_wakes_its_caller_once_per_pending_poll() {
    let (count, waker) = counting();
    let (sender, mut reply) = reply_channel();
    let mut out = Vec::new();
    assert_eq!(reply.poll_events(&mut out, &waker), ReplyStatus::Pending);
    sender.push(1);
    assert_eq!(wakes(&count), 1);
    sender.push(2);
    assert_eq!(wakes(&count), 1, "not polled since the last wake");
    assert_eq!(reply.poll_events(&mut out, &waker), ReplyStatus::Pending);
    sender.push(3);
    assert_eq!(wakes(&count), 2);
}

/// A poll with a new waker replaces the one kept before: a caller that moved
/// (another task, another thread) is the one woken, and the old one is not.
#[test]
fn a_poll_with_a_new_waker_replaces_the_old_one() {
    let (sender, mut reply) = reply_channel();
    let mut out = Vec::new();
    let ((old, old_waker), (new, new_waker)) = (counting(), counting());
    assert_eq!(
        reply.poll_events(&mut out, &old_waker),
        ReplyStatus::Pending
    );
    assert_eq!(
        reply.poll_events(&mut out, &new_waker),
        ReplyStatus::Pending
    );
    sender.push(1);
    assert_eq!((wakes(&old), wakes(&new)), (0, 1));
}

/// A waker that owns its reply's sender, as a task holding both ends
/// might, is replaced by a poll with another waker, after the caller
/// dropped its own copy: the poll drops the last copy, and so the sender,
/// whose drop ends the reply, and the poll still returns.
#[test]
fn a_poll_returns_when_the_waker_it_replaces_drops_the_sender() {
    struct OwningTheSender(#[expect(dead_code, reason = "held for its drop")] ReplySender<u32>);
    impl Wake for OwningTheSender {
        fn wake(self: Arc<Self>) {}
    }
    let status = ends_soon(|| {
        let (sender, mut reply) = reply_channel::<u32>();
        let owning = Waker::from(Arc::new(OwningTheSender(sender)));
        let mut out = Vec::new();
        assert_eq!(
            reply.poll_events(&mut out, &owning),
            ReplyStatus::Pending,
            "nothing was sent yet"
        );
        drop(owning);
        reply.poll_events(&mut out, &no_wake())
    });
    assert_eq!(status, ReplyStatus::Pending);
}

/// A reply that ended before it was first polled hands over, at that first
/// poll, every event and its end.
#[test]
fn a_reply_ended_before_its_first_poll_hands_over_everything_then() {
    let (sender, mut reply) = reply_channel();
    sender.push(1);
    sender.extend([2, 3]);
    sender.done();
    let mut out = Vec::new();
    assert_eq!(reply.poll_events(&mut out, &no_wake()), ReplyStatus::Done);
    assert_eq!(out, [1, 2, 3]);
}

/// Handing over no events does not wake the caller: there is nothing to
/// take.
#[test]
fn an_empty_extend_or_append_does_not_wake() {
    let (count, waker) = counting();
    let (sender, mut reply) = reply_channel::<u32>();
    assert_eq!(
        reply.poll_events(&mut Vec::new(), &waker),
        ReplyStatus::Pending
    );
    sender.extend(Vec::new());
    sender.append(&mut Vec::new());
    assert_eq!(wakes(&count), 0);
}

/// A worker appends a large batch to a reply whose caller took every event
/// before, and the caller polls it with an empty buffer: the batch's buffer
/// is moved both ways, never copied.
#[test]
fn a_batch_appended_to_a_caller_that_took_everything_is_moved_not_copied() {
    let (sender, mut reply) = reply_channel();
    let mut batch: Vec<u32> = (0..1024).collect();
    let mut out = Vec::new();
    let made = super::allocations(|| {
        sender.append(&mut batch);
        assert_eq!(
            reply.poll_events(&mut out, Waker::noop()),
            ReplyStatus::Pending,
            "the worker has not ended the reply"
        );
    });
    assert_eq!(made, 0);
    assert_eq!(out, (0..1024).collect::<Vec<_>>(), "the batch, in order");
}

/// A worker extends its reply from an iterator that is slow to make its
/// events: the caller's poll meanwhile returns at once, without them.
#[test]
fn a_poll_does_not_wait_for_the_events_an_extend_is_making() {
    let (sender, mut reply) = reply_channel::<u32>();
    let (making, made) = mpsc::channel();
    let (finish, finished) = mpsc::channel::<()>();
    let worker = thread::spawn(move || {
        sender.extend(std::iter::once_with(move || {
            making.send(()).expect("the test waits for the event");
            // Until the test lets it end, or ends itself.
            let _ = finished.recv();
            5
        }));
    });
    made.recv().expect("the worker makes its event");
    let status = ends_soon(move || reply.poll_events(&mut Vec::new(), Waker::noop()));
    assert_eq!(status, ReplyStatus::Pending);
    finish.send(()).expect("the worker waits");
    assert!(worker.join().is_ok());
}

/// Ending a reply wakes a caller whose poll found nothing, so it learns of
/// the end without polling again by itself.
#[test]
fn ending_a_reply_wakes_its_caller() {
    for fail in [false, true] {
        let (count, waker) = counting();
        let (sender, mut reply) = reply_channel::<u32>();
        assert_eq!(
            reply.poll_events(&mut Vec::new(), &waker),
            ReplyStatus::Pending
        );
        if fail {
            sender.fail("stopped".into());
        } else {
            sender.done();
        }
        assert_eq!(wakes(&count), 1, "fail: {fail}");
    }
}

/// A worker that stops without ending its reply ends it with a failure, so
/// that its caller is not left waiting.
#[test]
fn a_dropped_reply_sender_fails_its_reply() {
    let (count, waker) = counting();
    let (sender, mut reply) = reply_channel::<u32>();
    let mut out = Vec::new();
    assert_eq!(reply.poll_events(&mut out, &waker), ReplyStatus::Pending);
    let worker = thread::spawn(move || {
        sender.push(5);
        drop(sender);
    });
    assert!(worker.join().is_ok());
    assert_eq!(wakes(&count), 1);
    assert_eq!(
        reply.poll_events(&mut out, &waker),
        ReplyStatus::Failed("the executor dropped the reply before its end".into())
    );
    assert_eq!(out, [5]);
}

/// A caller that drops its reply does not stop the work: the worker still
/// pushes its events and ends the reply, without a panic.
#[test]
fn a_dropped_reply_leaves_its_reply_sender_free_to_push_and_end() {
    let (sender, reply) = reply_channel::<u32>();
    drop(reply);
    let worker = thread::spawn(move || {
        sender.push(5);
        sender.extend([6, 7]);
        sender.done();
    });
    assert!(worker.join().is_ok());
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
/// later is polled twice (three times if a woken flag left over from an
/// earlier wait wakes the thread early), against more when it spins.
#[test]
fn wait_parks_until_woken_instead_of_spinning() {
    let mut worker = Worker(Duration::from_millis(30));
    let (reply, polls) = counted(
        worker
            .call(on_shot_0(&[]))
            .expect("a `Worker` accepts every batch"),
    );
    let mut out = Vec::new();
    assert_eq!(wait(reply, &mut out), Ok(()));
    assert_eq!(out, [outcome(0, 0)]);
    let parked = polls.load(Ordering::SeqCst);
    assert!((2..=3).contains(&parked), "{parked} polls");

    let (reply, polls) = counted(
        worker
            .call(on_shot_0(&[]))
            .expect("a `Worker` accepts every batch"),
    );
    assert_eq!(spin_wait(reply, &mut out), Ok(()));
    let spun = polls.load(Ordering::SeqCst);
    // The control only shows that the count tells spinning from parking. A
    // spinning driver yields between polls, so how often it polls depends on
    // the machine's load: hundreds of times in 30 ms when idle, 10 to 12 on
    // two cores shared with the rest of the suite; more than parking's three
    // in every run measured.
    assert!(spun > 3, "{spun} polls when spinning");
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

/// `relay` passes an inner reply's events up from a worker thread, and
/// `wait` takes them on the caller's thread, parked between pushes.
#[test]
fn relay_and_wait_pass_events_across_threads() {
    let (inner_sender, inner) = reply_channel();
    let (outer_sender, outer) = reply_channel();
    let worker = thread::spawn(move || relay(inner, outer_sender));
    let feeder = thread::spawn(move || {
        for event in 0..100 {
            inner_sender.push(event);
            thread::yield_now();
        }
        inner_sender.fail("stopped".into());
    });
    let mut out = Vec::new();
    assert_eq!(wait(outer, &mut out), Err("stopped".into()));
    assert_eq!(out, (0..100).collect::<Vec<_>>());
    assert!(worker.join().is_ok() && feeder.join().is_ok());
}

/// An executor whose work runs on a worker, which relays its device's
/// reply: each event the device pushes reaches the caller as it comes,
/// with a wake, before the device's reply ends, not all at its end.
#[test]
fn relay_hands_each_event_to_the_caller_as_it_comes() {
    let (device, inner) = reply_channel();
    let (sender, mut reply) = reply_channel();
    let worker = thread::spawn(move || relay(inner, sender));
    let (signals, waker) = signalling();
    let mut out = Vec::new();
    assert_eq!(reply.poll_events(&mut out, &waker), ReplyStatus::Pending);
    device.push(7);
    signals
        .recv_timeout(PATIENT)
        .expect("relay hands the event over as it comes, which wakes the caller");
    assert_eq!(
        reply.poll_events(&mut out, &waker),
        ReplyStatus::Pending,
        "the device's reply has not ended"
    );
    assert_eq!(out, [7]);
    device.done();
    signals
        .recv_timeout(PATIENT)
        .expect("relay hands the end over, which wakes the caller");
    assert_eq!(reply.poll_events(&mut out, &waker), ReplyStatus::Done);
    assert!(worker.join().is_ok());
}

/// `relay` passes an inner reply's cancellation up as a cancellation, not
/// as a failure: the caller of an executor that relays from a worker tells
/// a call abandoned below it from one that failed.
/// Typed, not a reason of `Failed`: any executor could fail with the same
/// text, and the compiler cannot check a match on a string; tokio
/// (`JoinError::is_cancelled`), Java (`CancellationException`) and Python
/// (`CancelledError`) type it too.
#[test]
fn relay_passes_a_cancellation_up() {
    let (inner_sender, inner) = reply_channel::<u32>();
    let (outer_sender, mut outer) = reply_channel();
    inner_sender.cancel();
    relay(inner, outer_sender);
    assert_eq!(
        outer.poll_events(&mut Vec::new(), Waker::noop()),
        ReplyStatus::Cancelled
    );
}

/// `MappedReply` converts each event and keeps the reply's end.
#[test]
fn a_mapped_reply_converts_each_event_and_keeps_the_end() {
    let mut out = Vec::new();
    let doubled = MappedReply::new(
        ImmediateReply::failed(vec![1, 2], "lost".into()),
        |x: u32| x * 2,
    );
    assert_eq!(wait(doubled, &mut out), Err("lost".into()));
    assert_eq!(out, [2, 4]);
}

/// An executor of one shot whose work is done elsewhere: each call hands its
/// reply's sender to the test, which pushes the outcomes and ends the reply
/// when it likes.
struct Gated {
    shot: ShotId,
    senders: mpsc::Sender<(ShotId, ReplySender<ShotOutcome>)>,
}

impl Executor<InstructionBatch<bool>> for Gated {
    type Reply = IncrementalReply<ShotOutcome>;

    fn call(&mut self, _: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        let (sender, reply) = reply_channel();
        self.senders
            .send((self.shot, sender))
            .map_err(|_| Rejected::Unavailable("the test has ended".into()))?;
        Ok(reply)
    }
}

impl Close for Gated {
    type Report = ();

    fn close(&mut self) {}
}

// --------------------------------------------------------- a driver that polls

/// The waker of one call among several: marks that call ready, then unparks
/// the driver.
struct Flag {
    index: usize,
    ready: Arc<Mutex<Vec<usize>>>,
    driver: Thread,
}

impl Wake for Flag {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.ready
            .lock()
            .expect("no thread panicked holding it")
            .push(self.index);
        self.driver.unpark();
    }
}

/// A driver keeps several calls in flight, sleeps until one of them wakes
/// it, and polls only the calls that woke it.
#[test]
fn a_driver_keeps_several_calls_in_flight_and_sleeps_until_one_wakes_it() {
    let (tx, rx) = mpsc::channel();
    let mut executors: Vec<Gated> = (0..4)
        .map(|shot| Gated {
            shot,
            senders: tx.clone(),
        })
        .collect();
    let mut replies: Vec<Option<IncrementalReply<ShotOutcome>>> = executors
        .iter_mut()
        .map(|executor| {
            let shot = ShotSet::one(executor.shot);
            let batch = InstructionBatchRef {
                shots: &shot,
                instructions: &[],
            };
            Some(
                executor
                    .call(batch)
                    .expect("the test keeps the receiver, so `Gated` accepts"),
            )
        })
        .collect();
    let mut senders: Vec<_> = rx.try_iter().collect();
    assert_eq!(senders.len(), 4);
    senders.sort_by_key(|(j, _)| *j);
    // The calls end in reverse order, 10 ms apart.
    let producer = thread::spawn(move || {
        for (j, sender) in senders.into_iter().rev() {
            thread::sleep(Duration::from_millis(10));
            sender.push(outcome(j, j));
            sender.done();
        }
    });

    let ready = Arc::new(Mutex::new((0..4).collect::<Vec<usize>>()));
    let wakers: Vec<Waker> = (0..4)
        .map(|index| {
            Waker::from(Arc::new(Flag {
                index,
                ready: Arc::clone(&ready),
                driver: thread::current(),
            }))
        })
        .collect();
    let (mut polls, mut ended, mut out) = ([0; 4], Vec::new(), Vec::new());
    let deadline = Instant::now() + PATIENT;
    while ended.len() < 4 {
        let woken = std::mem::take(&mut *ready.lock().expect("no thread panicked holding it"));
        if woken.is_empty() {
            assert!(Instant::now() < deadline, "never woken");
            thread::park_timeout(Duration::from_millis(100));
            continue;
        }
        // Only the calls whose waker was woken are polled.
        for i in woken {
            let Some(reply) = &mut replies[i] else {
                continue;
            };
            polls[i] += 1;
            if reply.poll_events(&mut out, &wakers[i]) != ReplyStatus::Pending {
                ended.push(i);
                replies[i] = None;
            }
        }
    }
    assert!(producer.join().is_ok());
    assert_eq!(ended, [3, 2, 1, 0], "each call ended when its worker did");
    assert_eq!(
        out,
        [outcome(3, 3), outcome(2, 2), outcome(1, 1), outcome(0, 0)]
    );
    assert!(polls.iter().all(|&p| p <= 3), "polls per call: {polls:?}");
}

/// A driver that polls with the no-op waker does its own work between polls
/// and takes the events when it wants: nothing is lost, and it never calls
/// `wait`.
#[test]
fn a_driver_with_a_noop_waker_does_its_own_work_between_polls() {
    let mut worker = Worker(Duration::from_millis(20));
    let mut reply = worker
        .call(on_shot_0(&[]))
        .expect("a `Worker` accepts every batch");
    let (mut out, mut work) = (Vec::new(), 0_u32);
    let deadline = Instant::now() + PATIENT;
    loop {
        match reply.poll_events(&mut out, Waker::noop()) {
            ReplyStatus::Pending => {}
            ReplyStatus::Done => break,
            ReplyStatus::Failed(reason) => panic!("{reason}"),
            ReplyStatus::Cancelled => panic!("cancelled"),
        }
        assert!(Instant::now() < deadline, "never done");
        thread::sleep(Duration::from_millis(1)); // a unit of the driver's own work
        work += 1;
    }
    assert_eq!(out, [outcome(0, 0)]);
    assert!(work >= 1, "the driver worked while the call was pending");
}

// ----------------------------------------------------- one reply, four drivers

/// An executor whose work runs on a thread of its own: `call` returns at
/// once, and the thread pushes, for each shot of the batch, one outcome per
/// instruction (`true` reads One), then ends the reply.
struct OnAThread;

impl Executor<InstructionBatch<bool>> for OnAThread {
    type Reply = IncrementalReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        let (sender, reply) = reply_channel();
        let shots = batch.shots.clone();
        let instructions = batch.instructions.to_vec();
        thread::spawn(move || {
            for &shot in &shots {
                for (tag, &one) in (0..).zip(&instructions) {
                    sender.push(ShotOutcome::new(shot, tag, one.into()));
                }
            }
            sender.done();
        });
        Ok(reply)
    }
}

/// One executor and one reply type, four callers: each takes the same
/// events its own way, and the executor does not know which.
#[test]
fn one_reply_is_taken_four_ways() {
    let batch = [true, false, true];
    let expected: Vec<ShotOutcome> = (0..)
        .zip(batch)
        .map(|(tag, one)| ShotOutcome {
            shot: 0,
            tag,
            value: one.into(),
        })
        .collect();
    let call = || {
        OnAThread
            .call(on_shot_0(&batch))
            .expect("`OnAThread` accepts every batch")
    };

    // Blocking: `wait` parks the thread until the reply ends.
    let mut blocked = Vec::new();
    assert_eq!(wait(call(), &mut blocked), Ok(()));

    // Polling: the caller does its own work between polls.
    let (mut reply, mut polled) = (call(), Vec::new());
    let end = loop {
        match reply.poll_events(&mut polled, Waker::noop()) {
            ReplyStatus::Pending => thread::yield_now(), // the caller's own work
            end => break end,
        }
    };
    assert_eq!(end, ReplyStatus::Done);

    // Async, the whole reply: a `Future`. `block_on` stands in for an async
    // runtime; with this one task to run, it parks the thread between polls.
    let (awaited, end) = block_on(wait_async(call()));
    assert_eq!(end, Ok(()));

    // Async, the events as they come: a `Stream` of event batches.
    let mut events = async_events(call());
    let mut streamed = Vec::new();
    let end = block_on(poll_fn(|context| {
        loop {
            match Pin::new(&mut events).poll_next(context) {
                Poll::Ready(Some(Ok(mut batch))) => streamed.append(&mut batch),
                Poll::Ready(Some(Err(reason))) => return Poll::Ready(Err(reason)),
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
    }));
    assert_eq!(end, Ok(()));

    for events in [blocked, polled, awaited, streamed] {
        assert_eq!(events, expected);
    }
}
