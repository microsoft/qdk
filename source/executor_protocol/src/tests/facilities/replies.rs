// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};
use std::task::{Wake, Waker};
use std::thread;
use std::time::Duration;

use crate::{
    ImmediateReply, MappedReply, Reply, ReplySender, ReplyStatus, relay, reply_channel, wait,
};

use crate::tests::{PATIENT, counted, counting, ends_after, ends_soon, signalling, wakes};

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
        let made = crate::tests::allocations(|| {
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

/// A worker that hands its events over in batches (as `relay` does, one
/// batch per poll below) wakes a caller whose poll found nothing once, at
/// the first batch; the batches after it, until the caller polls again,
/// wait in the reply without a wake.
#[test]
fn batches_of_events_wake_their_caller_once_per_pending_poll() {
    let (count, waker) = counting();
    let (sender, mut reply) = reply_channel();
    let mut out = Vec::new();
    assert_eq!(reply.poll_events(&mut out, &waker), ReplyStatus::Pending);
    sender.append(&mut vec![1]);
    assert_eq!(wakes(&count), 1);
    sender.append(&mut vec![2]);
    sender.extend([3]);
    assert_eq!(wakes(&count), 1, "not polled since the last wake");
    assert_eq!(reply.poll_events(&mut out, &waker), ReplyStatus::Pending);
    sender.extend([4]);
    assert_eq!(wakes(&count), 2);
    sender.extend([5]);
    assert_eq!(wakes(&count), 2, "not polled since the last wake");
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
    // The caller's first poll, before counting: on some platforms (macOS) the
    // first lock of a `Mutex` allocates the lock itself.
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Pending,
        "precondition: nothing was sent yet"
    );
    let made = crate::tests::allocations(|| {
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

/// A worker that relays its device's reply parks between polls, as `wait`
/// does: a device's reply that ends 30 ms later is polled twice (once if
/// it ended before the first poll), where a relay that spun would poll it
/// on and on.
#[test]
fn relay_parks_between_polls() {
    let (inner, polls) = counted(ends_after(Duration::from_millis(30)));
    let (sender, outer) = reply_channel();
    let worker = thread::spawn(move || relay(inner, sender));
    assert_eq!(wait(outer, &mut Vec::new()), Ok(()));
    assert!(worker.join().is_ok());
    let polls = polls.load(Ordering::SeqCst);
    assert!((1..=2).contains(&polls), "{polls} polls");
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

/// A caller that gathers the readouts of several calls in one buffer polls
/// a mapped reply into it: the converted events go after those it held,
/// which stay as they were.
#[test]
fn a_mapped_reply_appends_its_events_to_those_the_caller_holds() {
    let mut out = vec![7];
    let mut doubled = MappedReply::new(ImmediateReply::done(vec![1, 2]), |x: u32| x * 2);
    assert_eq!(
        doubled.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Done,
        "the wrapped reply was made whole"
    );
    assert_eq!(out, [7, 2, 4]);
}

/// A caller polls a mapped reply while the device's reply it wraps is
/// still running, then the device pushes a readout: the caller is woken,
/// since the mapped reply passed the caller's waker to the reply it wraps.
#[test]
fn a_mapped_reply_passes_its_callers_waker_to_the_reply_it_wraps() {
    let (sender, device) = reply_channel::<u32>();
    let mut doubled = MappedReply::new(device, |x: u32| x * 2);
    let (count, waker) = counting();
    assert_eq!(
        doubled.poll_events(&mut Vec::new(), &waker),
        ReplyStatus::Pending,
        "the device's reply is running"
    );
    sender.push(1);
    assert_eq!(wakes(&count), 1, "the push wakes the caller");
}
