// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An executor's lifecycle refuses every call once closed, though its
//! pending replies end as if it were open, and every call after a failure,
//! but not after a refusal; a reply bound to it fails the executor when it
//! fails or is cancelled, and ends at an earlier call's failure, even if it
//! failed itself, which wakes it while it is pending (at least once, but not
//! if it ended or was dropped before the failure), each of several replies
//! bound for one call on its own, so that its next poll ends it, also when
//! the failure races the polls on other threads, and leaves the events it
//! handed over before; a later call's failure leaves it alone, and a
//! failure of its own call, its own or another part's, does not wake it.
//! The earliest failed call is the executor's failure, and of one call's
//! failures, the first. The crate's ensemble double, built on it, keeps the
//! conformance kit's rules, reporting once included.

use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Barrier, mpsc};
use std::task::Waker;
use std::thread;

use super::placed_executor::PanicsWhenWoken;
use crate::reply::CANCELLATION;
use crate::tests::{
    SharedState, bits_set, block_on, counted, counting, ends_soon, flip_and_toss, on_shot_0, shots,
    wakes,
};
use crate::{
    Close, Executor, ExecutorLifecycle, ImmediateReply, IncrementalReply, InstructionBatch,
    InstructionBatchRef, InstructionCall, LifecycleBoundReply, Operand, Rejected, Reply,
    ReplySender, ReplyStatus, ShotOutcome, ShotSet, conformance, reply_channel, wait, wait_async,
};

/// Admits a call on `lifecycle` and binds its reply, which the sender
/// ends.
fn bound_call(
    lifecycle: &mut ExecutorLifecycle,
) -> (ReplySender<u32>, LifecycleBoundReply<IncrementalReply<u32>>) {
    lifecycle
        .admit()
        .expect("the executor is open and no call failed");
    let (sender, reply) = reply_channel();
    (sender, lifecycle.bind(reply))
}

/// Admits a call on `lifecycle` whose shots go to two devices, and binds
/// each part's reply, which its device's sender ends.
fn bound_call_of_two_parts(
    lifecycle: &mut ExecutorLifecycle,
) -> [(ReplySender<u32>, LifecycleBoundReply<IncrementalReply<u32>>); 2] {
    lifecycle
        .admit()
        .expect("the executor is open and no call failed");
    [(), ()].map(|()| {
        let (sender, reply) = reply_channel();
        (sender, lifecycle.bind(reply))
    })
}

/// A reply whose work never ends, and which keeps no waker: a panicking
/// waker given to it is woken only by the lifecycle.
struct Unending;

impl Reply for Unending {
    type Item = u32;

    fn poll_events(&mut self, _: &mut Vec<u32>, _: &Waker) -> ReplyStatus {
        ReplyStatus::Pending
    }
}

/// Admits a call on `lifecycle` and binds its reply, which never ends.
fn unending_call(lifecycle: &mut ExecutorLifecycle) -> LifecycleBoundReply<Unending> {
    lifecycle
        .admit()
        .expect("the executor is open and no call failed");
    lifecycle.bind(Unending)
}

/// Polls `reply` once into `out`, with no waker to wake.
fn poll<R: Reply>(reply: &mut R, out: &mut Vec<R::Item>) -> ReplyStatus {
    reply.poll_events(out, Waker::noop())
}

/// An executor closed by its caller refuses every later call with
/// `Closed`.
#[test]
fn a_closed_executor_refuses_every_call() {
    let mut lifecycle = ExecutorLifecycle::default();
    lifecycle.admit().expect("an open executor admits a call");
    lifecycle.close();
    assert_eq!(lifecycle.admit(), Err(Rejected::Closed));
    assert_eq!(lifecycle.admit(), Err(Rejected::Closed));
}

/// A call's device stops while the call runs: the executor refuses every
/// later call, naming the failure.
#[test]
fn a_failed_executor_refuses_every_later_call() {
    let mut lifecycle = ExecutorLifecycle::default();
    lifecycle.admit().expect("an open executor admits a call");
    lifecycle.fail("the device stopped");
    let refused = Err(Rejected::EarlierFailure("the device stopped".into()));
    assert_eq!(lifecycle.admit(), refused);
    assert_eq!(lifecycle.admit(), refused);
}

/// An executor that failed, then was closed, says that it is closed: the
/// caller learns that it may not call it at all.
#[test]
fn a_closed_executor_says_closed_even_after_it_failed() {
    let mut lifecycle = ExecutorLifecycle::default();
    lifecycle.admit().expect("an open executor admits a call");
    lifecycle.fail("the device stopped");
    lifecycle.close();
    assert_eq!(lifecycle.admit(), Err(Rejected::Closed));
}

/// A caller closes its executor while a call's work still runs on a
/// worker: the call's reply is left alone, and ends as the worker ends it,
/// with its events.
#[test]
fn a_bound_reply_pending_at_close_ends_as_its_work_does() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (worker, mut reply) = bound_call(&mut lifecycle);
    lifecycle.close();
    worker.push(7);
    worker.done();
    let mut out = Vec::new();
    assert_eq!(poll(&mut reply, &mut out), ReplyStatus::Done);
    assert_eq!(out, [7]);
}

/// A call's work runs on a worker, which fails after `call` returned: the
/// reply ends with the worker's failure, as an unbound one would.
#[test]
fn a_bound_reply_that_fails_ends_with_its_own_failure() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (sender, mut reply) = bound_call(&mut lifecycle);
    sender.fail("lost".into());
    assert_eq!(
        poll(&mut reply, &mut Vec::new()),
        ReplyStatus::Failed("lost".into())
    );
}

/// A call's work runs on a worker, whose own executor was dropped before
/// the work ran: the reply ends cancelled, as an unbound one would, so that
/// the caller tells a call abandoned below the executor from one that
/// failed.
/// Typed, not a reason of `Failed`: any executor could fail with the same
/// text, and the compiler cannot check a match on a string; tokio
/// (`JoinError::is_cancelled`), Java (`CancellationException`) and Python
/// (`CancelledError`) type it too.
#[test]
fn a_bound_reply_that_is_cancelled_ends_cancelled() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (sender, mut reply) = bound_call(&mut lifecycle);
    sender.cancel();
    assert_eq!(poll(&mut reply, &mut Vec::new()), ReplyStatus::Cancelled);
}

/// A call's work runs on a worker, whose own executor was dropped before
/// the work ran: the executor fails as its caller polls the cancelled
/// reply, and refuses the next call, as after a failure. Chosen because a
/// cancelled call counts as a failed one unless a rule says otherwise: the
/// executor that was to run the work is gone, so a later call has nothing
/// to run on; only an executor that dropped its parts itself knows that
/// nothing failed.
#[test]
fn a_bound_reply_that_is_cancelled_fails_the_executor() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (sender, mut reply) = bound_call(&mut lifecycle);
    sender.cancel();
    poll(&mut reply, &mut Vec::new());
    assert_eq!(
        lifecycle.admit(),
        Err(Rejected::EarlierFailure(CANCELLATION.to_owned()))
    );
}

/// Two calls' work runs on workers; the second's worker fails after `call`
/// returned, while the first's still runs: the executor fails as its caller
/// polls the second reply, and refuses the next call, though the first
/// reply is pending.
#[test]
fn a_bound_reply_that_fails_fails_the_executor() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (_first, mut first_reply) = bound_call(&mut lifecycle);
    let (second, mut second_reply) = bound_call(&mut lifecycle);
    let status = poll(&mut first_reply, &mut Vec::new());
    assert_eq!(status, ReplyStatus::Pending, "the first call's work runs");
    second.fail("lost".into());
    let status = poll(&mut second_reply, &mut Vec::new());
    assert_eq!(
        status,
        ReplyStatus::Failed("lost".into()),
        "the second reply ends as its worker failed"
    );
    assert_eq!(
        lifecycle.admit(),
        Err(Rejected::EarlierFailure("lost".into()))
    );
}

/// Two calls fail, and their caller polls the later one first: the
/// executor's failure is the earlier call's, which left the state the
/// later one ran on.
#[test]
fn the_earliest_failed_call_is_the_executors_failure() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let (second, mut second_reply) = bound_call(&mut lifecycle);
    second.fail("second".into());
    poll(&mut second_reply, &mut Vec::new());
    first.fail("first".into());
    poll(&mut first_reply, &mut Vec::new());
    assert_eq!(
        lifecycle.admit(),
        Err(Rejected::EarlierFailure("first".into()))
    );
}

/// Two calls are in flight; the second has an event ready when the first
/// fails: the second ends with the first's failure, without its event,
/// which ran on the state the failure left.
#[test]
fn a_bound_reply_ends_at_an_earlier_calls_failure() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let (second, mut second_reply) = bound_call(&mut lifecycle);
    second.push(7);
    first.fail("lost".into());
    poll(&mut first_reply, &mut Vec::new());
    let mut out = Vec::new();
    assert_eq!(
        poll(&mut second_reply, &mut out),
        ReplyStatus::Failed("an earlier call failed: lost".into())
    );
    assert_eq!(out, Vec::<u32>::new());
}

/// Two calls are in flight and the second fails: the first, which ran
/// before it, ends as its own work does, with its events.
#[test]
fn a_bound_reply_is_unaffected_by_a_later_calls_failure() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let (second, mut second_reply) = bound_call(&mut lifecycle);
    second.fail("lost".into());
    poll(&mut second_reply, &mut Vec::new());
    first.push(7);
    first.done();
    let mut out = Vec::new();
    assert_eq!(poll(&mut first_reply, &mut out), ReplyStatus::Done);
    assert_eq!(out, [7]);
}

/// A caller waits on the second of two calls when the first fails: the
/// failure wakes it, though the second call's work has no news, since its
/// reply can now end.
#[test]
fn an_earlier_failure_wakes_a_pending_bound_reply() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let (_second, mut second_reply) = bound_call(&mut lifecycle);
    let (count, waker) = counting();
    let status = second_reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(status, ReplyStatus::Pending, "the second call's work runs");
    first.fail("lost".into());
    poll(&mut first_reply, &mut Vec::new());
    assert_ne!(wakes(&count), 0);
}

/// A caller keeps four calls in flight; the third sends its shots to two
/// devices and binds each part's reply. A task of the caller waits on each
/// later reply, each with its own waker, and device B's part then ends. The
/// first call fails: the failure wakes every reply still pending, whatever
/// its call and whichever parts ended, so that none waits for news that
/// will not come.
#[test]
fn an_earlier_failure_wakes_every_pending_bound_reply_of_the_later_calls() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let (_second, mut second_reply) = bound_call(&mut lifecycle);
    let [(_device_a, mut part_a), (device_b, mut part_b)] = bound_call_of_two_parts(&mut lifecycle);
    let (_fourth, mut fourth_reply) = bound_call(&mut lifecycle);
    let waiting = |reply: &mut LifecycleBoundReply<_>, name| {
        let (count, waker) = counting();
        let status = reply.poll_events(&mut Vec::new(), &waker);
        assert_eq!(status, ReplyStatus::Pending, "{name}'s work runs");
        (name, count)
    };
    let woken = [
        waiting(&mut second_reply, "the second call"),
        waiting(&mut part_a, "part A of the third call"),
        waiting(&mut fourth_reply, "the fourth call"),
    ];
    waiting(&mut part_b, "part B of the third call");
    device_b.done();
    let status = poll(&mut part_b, &mut Vec::new());
    assert_eq!(status, ReplyStatus::Done, "device B finished its shots");
    first.fail("lost".into());
    poll(&mut first_reply, &mut Vec::new());
    for (name, count) in woken {
        assert_ne!(wakes(&count), 0, "the task waiting on {name} is woken");
    }
}

/// A caller keeps three calls in flight and waits on the later two, the
/// second with a waker that panics when woken (the caller's bug); the
/// first call fails, and its reply is polled: the failure wakes the third
/// call's reply too, so that its task does not wait for news that will
/// not come.
#[test]
fn an_earlier_failure_wakes_every_later_reply_though_a_waker_panics() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let mut second_reply = unending_call(&mut lifecycle);
    let mut third_reply = unending_call(&mut lifecycle);
    let panicking = Waker::from(Arc::new(PanicsWhenWoken));
    let status = second_reply.poll_events(&mut Vec::new(), &panicking);
    assert_eq!(status, ReplyStatus::Pending, "the second call's work runs");
    let (count, waker) = counting();
    let status = third_reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(status, ReplyStatus::Pending, "the third call's work runs");
    first.fail("lost".into());
    panic::catch_unwind(AssertUnwindSafe(|| poll(&mut first_reply, &mut Vec::new())))
        .expect_err("the second call's waker panics");
    assert_eq!(wakes(&count), 1);
}

/// The scene above, with the second call alone: the panic of its waker
/// comes out of the poll of the first call's reply, which recorded the
/// failure.
#[test]
fn a_later_replys_waker_panic_comes_out_of_the_poll_that_recorded_the_failure() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let mut second_reply = unending_call(&mut lifecycle);
    let panicking = Waker::from(Arc::new(PanicsWhenWoken));
    let status = second_reply.poll_events(&mut Vec::new(), &panicking);
    assert_eq!(status, ReplyStatus::Pending, "the second call's work runs");
    first.fail("lost".into());
    let polled = panic::catch_unwind(AssertUnwindSafe(|| poll(&mut first_reply, &mut Vec::new())));
    let panic = polled.expect_err("the second call's waker panicked");
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"the caller's waker panics")
    );
}

/// A caller waits on the second of two calls, and the first call's failure
/// wakes it: its next poll ends the reply with that failure, as
/// `EarlierFailure` is displayed, though the second call's work still runs.
#[test]
fn a_bound_reply_woken_by_an_earlier_failure_ends_at_its_next_poll() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let (_second, mut second_reply) = bound_call(&mut lifecycle);
    let (count, waker) = counting();
    let status = second_reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(status, ReplyStatus::Pending, "the second call's work runs");
    first.fail("lost".into());
    poll(&mut first_reply, &mut Vec::new());
    assert_ne!(wakes(&count), 0, "the first call's failure woke the caller");
    let earlier = Rejected::EarlierFailure("lost".into()).to_string();
    assert_eq!(
        second_reply.poll_events(&mut Vec::new(), &waker),
        ReplyStatus::Failed(earlier)
    );
}

/// A later call's work that, as it is polled, polls the failing earlier
/// call's reply (on another thread in a real caller), so that the earlier
/// failure is recorded during its poll; it gives one event at each poll.
struct PollsTheFailingCall {
    failing: LifecycleBoundReply<IncrementalReply<u32>>,
    next: u32,
}

impl Reply for PollsTheFailingCall {
    type Item = u32;

    fn poll_events(&mut self, out: &mut Vec<u32>, _: &Waker) -> ReplyStatus {
        poll(&mut self.failing, &mut Vec::new());
        out.push(self.next);
        self.next += 1;
        ReplyStatus::Pending
    }
}

/// A caller polls the second of two calls just as the first call's failure
/// is recorded, and the second call's work is left pending: its next poll
/// ends it with that failure, and hands over no more events, though the
/// work below gives one at each poll.
#[test]
fn a_reply_left_pending_as_an_earlier_failure_is_recorded_ends_at_its_next_poll() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, first_reply) = bound_call(&mut lifecycle);
    lifecycle
        .admit()
        .expect("the executor is open and no call failed");
    let mut second_reply = lifecycle.bind(PollsTheFailingCall {
        failing: first_reply,
        next: 0,
    });
    first.fail("lost".into());
    let status = poll(&mut second_reply, &mut Vec::new());
    assert_eq!(
        status,
        ReplyStatus::Pending,
        "the failure is recorded during the poll, which leaves the reply pending"
    );
    let mut out = Vec::new();
    let earlier = Rejected::EarlierFailure("lost".into()).to_string();
    assert_eq!(
        (poll(&mut second_reply, &mut out), out),
        (ReplyStatus::Failed(earlier), vec![])
    );
}

/// A reply that tells, after each of its polls, that it was polled.
struct TellsPolls<R>(R, mpsc::Sender<()>);

impl<R: Reply> Reply for TellsPolls<R> {
    type Item = R::Item;

    fn poll_events(&mut self, out: &mut Vec<R::Item>, waker: &Waker) -> ReplyStatus {
        let status = self.0.poll_events(out, waker);
        // Only the first message is listened to; later ones may go nowhere.
        let _ = self.1.send(());
        status
    }
}

/// A caller waits on the second of two calls with `wait`, its thread
/// parked once the reply is pending, and the first call fails on another
/// thread: `wait` returns the earlier failure at its next poll, rather
/// than polling in a loop.
#[test]
fn wait_on_a_bound_reply_returns_at_the_poll_after_an_earlier_failure() {
    let (end, polls) = ends_soon(|| {
        let mut lifecycle = ExecutorLifecycle::default();
        let (first, mut first_reply) = bound_call(&mut lifecycle);
        let (_second, second_reply) = bound_call(&mut lifecycle);
        let (polled, was_polled) = mpsc::channel();
        let (reply, polls) = counted(TellsPolls(second_reply, polled));
        let failing = thread::spawn(move || {
            was_polled
                .recv()
                .expect("the caller polls the second reply");
            first.fail("lost".into());
            poll(&mut first_reply, &mut Vec::new())
        });
        let end = wait(reply, &mut Vec::new());
        let failed = failing.join().expect("the failing thread does not panic");
        assert_eq!(failed, ReplyStatus::Failed("lost".into()), "call 1 failed");
        (end, polls.load(Ordering::SeqCst))
    });
    let earlier = Rejected::EarlierFailure("lost".into()).to_string();
    assert_eq!(end, Err(earlier), "the reply ends with the earlier failure");
    assert_eq!(polls, 2, "one poll pending, one that ends the reply");
}

/// A caller keeps three calls in flight, the second sent to two devices,
/// and waits on each later reply from a thread of its own, two with `wait`
/// and one with `wait_async`, while the first call fails on another thread:
/// a few hundred times, the failure coming before, during or after the
/// polls. Every reply ends with the earlier failure, so no waiting thread
/// is left parked, and `wait` returns at the poll after the failure.
#[test]
fn an_earlier_failure_racing_the_polls_wakes_and_ends_every_waiting_bound_reply() {
    type Bound = LifecycleBoundReply<IncrementalReply<u32>>;
    fn waits(
        reply: Bound,
        start: &Arc<Barrier>,
    ) -> thread::JoinHandle<(Result<(), String>, usize)> {
        let start = Arc::clone(start);
        thread::spawn(move || {
            let (reply, polls) = counted(reply);
            start.wait();
            let end = wait(reply, &mut Vec::new());
            (end, polls.load(Ordering::SeqCst))
        })
    }
    fn awaits(reply: Bound, start: &Arc<Barrier>) -> thread::JoinHandle<Result<(), String>> {
        let start = Arc::clone(start);
        thread::spawn(move || {
            start.wait();
            block_on(wait_async(reply)).1
        })
    }
    let earlier = Rejected::EarlierFailure("lost".into()).to_string();
    ends_soon(move || {
        for round in 0..300 {
            let mut lifecycle = ExecutorLifecycle::default();
            let (first, mut first_reply) = bound_call(&mut lifecycle);
            let [(_device_a, part_a), (_device_b, part_b)] =
                bound_call_of_two_parts(&mut lifecycle);
            let (_third, third_reply) = bound_call(&mut lifecycle);
            let start = Arc::new(Barrier::new(4));
            let waiting = [waits(part_a, &start), waits(part_b, &start)];
            let awaiting = awaits(third_reply, &start);
            let failing = {
                let start = Arc::clone(&start);
                thread::spawn(move || {
                    start.wait();
                    // A delay that differs from round to round, so that the
                    // failure falls before, during and after the polls.
                    for _ in 0..(round % 40) * 10 {
                        std::hint::spin_loop();
                    }
                    first.fail("lost".into());
                    poll(&mut first_reply, &mut Vec::new())
                })
            };
            let failed = failing.join().expect("the failing thread does not panic");
            assert_eq!(failed, ReplyStatus::Failed("lost".into()), "call 1 failed");
            for waiter in waiting {
                let (end, polls) = waiter.join().expect("a waiting thread does not panic");
                assert_eq!(end, Err(earlier.clone()), "round {round}");
                assert!(polls <= 2, "round {round}: {polls} polls under `wait`");
            }
            let end = awaiting.join().expect("the awaiting thread does not panic");
            assert_eq!(end, Err(earlier.clone()), "round {round}, `wait_async`");
        }
    });
}

/// A caller keeps four calls in flight and polls each of the three later
/// replies in a loop from a thread of its own, without parking, giving each
/// poll a new waker, while the first call fails on another thread: two
/// thousand times, the failure coming before, during or after a poll. Each
/// waker a reply was left with as the failure came is woken, by the failure,
/// which took it from the list, or by the reply, which saw the failure as it
/// listed it: no wake is lost on either side.
///
/// The failing thread tells where the failure is. A poller whose pending
/// poll returned once the failure began waits for the failure's `poll` to
/// return, then checks that this poll's waker was woken. It checks no
/// waker of an earlier poll: the reply was polled again, and that poll may
/// have seen the failure and taken the waker off the list.
#[test]
fn an_earlier_failure_racing_the_polls_wakes_the_waker_each_reply_was_left_with() {
    type Bound = LifecycleBoundReply<IncrementalReply<u32>>;
    /// Where a round's first call's failure is: `0` before, `1` while it
    /// is recorded, `2` once its `poll` returned.
    type Failure = Arc<AtomicU8>;
    // Enough rounds that a gap of a few instructions, such as one between
    // recording the failure and taking the wakers off the list, is hit in
    // nearly every run, within about a second.
    const ROUNDS: usize = 2000;
    const POLLERS: usize = 3;
    let start = Arc::new(Barrier::new(POLLERS + 1));
    let failures: Arc<Vec<Failure>> = Arc::new((0..ROUNDS).map(|_| Failure::default()).collect());
    let mut firsts = Vec::new();
    let mut later: Vec<Vec<(ReplySender<u32>, Bound)>> = (0..POLLERS).map(|_| Vec::new()).collect();
    for _ in 0..ROUNDS {
        let mut lifecycle = ExecutorLifecycle::default();
        firsts.push(bound_call(&mut lifecycle));
        for replies in &mut later {
            replies.push(bound_call(&mut lifecycle));
        }
    }
    let earlier = Rejected::EarlierFailure("lost".into()).to_string();
    ends_soon(move || {
        let polling: Vec<_> = later
            .into_iter()
            .map(|replies| {
                let (start, failures) = (Arc::clone(&start), Arc::clone(&failures));
                thread::spawn(move || {
                    let mut ends = Vec::new();
                    for ((_sender, mut reply), failure) in replies.into_iter().zip(failures.iter())
                    {
                        start.wait();
                        let mut lost = false;
                        let end = loop {
                            let (count, waker) = counting();
                            let status = reply.poll_events(&mut Vec::new(), &waker);
                            if status != ReplyStatus::Pending {
                                break status;
                            }
                            if failure.load(Ordering::SeqCst) != 0 {
                                while failure.load(Ordering::SeqCst) != 2 {
                                    thread::yield_now();
                                }
                                lost |= wakes(&count) == 0;
                            }
                        };
                        ends.push((end, lost));
                    }
                    ends
                })
            })
            .collect();
        let failing = {
            let (start, failures) = (Arc::clone(&start), Arc::clone(&failures));
            thread::spawn(move || {
                let mut ends = Vec::new();
                for (round, ((first, mut first_reply), failure)) in
                    firsts.into_iter().zip(failures.iter()).enumerate()
                {
                    start.wait();
                    // A delay that differs from round to round, so that the
                    // failure falls before, during and after a poll.
                    for _ in 0..(round % 40) * 10 {
                        std::hint::spin_loop();
                    }
                    first.fail("lost".into());
                    failure.store(1, Ordering::SeqCst);
                    ends.push(poll(&mut first_reply, &mut Vec::new()));
                    failure.store(2, Ordering::SeqCst);
                }
                ends
            })
        };
        let failed = failing.join().expect("the failing thread does not panic");
        for (round, failed) in failed.into_iter().enumerate() {
            assert_eq!(failed, ReplyStatus::Failed("lost".into()), "round {round}");
        }
        for poller in polling {
            let ends = poller.join().expect("a polling thread does not panic");
            for (round, (end, lost)) in ends.into_iter().enumerate() {
                assert_eq!(end, ReplyStatus::Failed(earlier.clone()), "round {round}");
                assert!(!lost, "round {round}: a wake was lost");
            }
        }
    });
}

/// A caller polls the second of two calls with one waker, then, from
/// another task, with another, before the first fails: the failure wakes
/// the waker of the last poll, not the one it replaced.
#[test]
fn an_earlier_failure_wakes_only_the_waker_of_the_last_poll() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let (_second, mut second_reply) = bound_call(&mut lifecycle);
    let (old_count, old) = counting();
    let (new_count, new) = counting();
    let status = second_reply.poll_events(&mut Vec::new(), &old);
    assert_eq!(status, ReplyStatus::Pending, "the second call's work runs");
    let status = second_reply.poll_events(&mut Vec::new(), &new);
    assert_eq!(
        status,
        ReplyStatus::Pending,
        "the second call's work still runs"
    );
    first.fail("lost".into());
    poll(&mut first_reply, &mut Vec::new());
    assert_eq!(wakes(&old_count), 0, "the replaced waker is not woken");
    assert_ne!(wakes(&new_count), 0);
}

/// A caller waited on the second of two calls, whose work then ended: the
/// first call's failure, which comes after, does not wake the caller, which
/// has nothing more to poll.
#[test]
fn an_ended_bound_reply_is_not_woken_by_an_earlier_failure() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let (second, mut second_reply) = bound_call(&mut lifecycle);
    let (count, waker) = counting();
    let status = second_reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(status, ReplyStatus::Pending, "the second call's work runs");
    second.done();
    let status = second_reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(status, ReplyStatus::Done, "the second call's work ended");
    let woken = wakes(&count);
    first.fail("lost".into());
    poll(&mut first_reply, &mut Vec::new());
    assert_eq!(wakes(&count), woken);
}

/// A caller waited on the second of two calls, then dropped its reply,
/// giving up on it: the first call's failure, which comes after, does not
/// wake the caller for the reply it dropped.
#[test]
fn a_dropped_bound_reply_is_not_woken_by_an_earlier_failure() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let (_second, mut second_reply) = bound_call(&mut lifecycle);
    let (count, waker) = counting();
    let status = second_reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(status, ReplyStatus::Pending, "the second call's work runs");
    drop(second_reply);
    first.fail("lost".into());
    poll(&mut first_reply, &mut Vec::new());
    assert_eq!(wakes(&count), 0);
}

/// A call's device stops before its `call` returns, and the executor
/// records it; the call's reply, bound, waits for its worker to report what
/// ran before the stop. Its caller polls it while it waits: the call's own
/// failure does not wake the caller, who would poll it in a busy loop.
#[test]
fn a_bound_reply_is_not_woken_by_its_own_calls_failure() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (_worker, mut reply) = bound_call(&mut lifecycle);
    lifecycle.fail("the device stopped");
    let (count, waker) = counting();
    for _ in 0..2 {
        let status = reply.poll_events(&mut Vec::new(), &waker);
        assert_eq!(status, ReplyStatus::Pending, "the worker has not reported");
    }
    assert_eq!(wakes(&count), 0);
}

/// A call's work runs on a worker, and its caller waits on the bound reply;
/// the worker fails, which wakes the caller: the poll that ends the reply
/// with that failure wakes it no more, since the caller has nothing left
/// to poll.
#[test]
fn a_bound_reply_that_fails_is_not_woken_by_its_own_failure() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (worker, mut reply) = bound_call(&mut lifecycle);
    let (count, waker) = counting();
    let status = reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(status, ReplyStatus::Pending, "the worker runs the call");
    worker.fail("lost".into());
    let woken = wakes(&count);
    let status = reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(
        status,
        ReplyStatus::Failed("lost".into()),
        "the reply ends as its worker failed"
    );
    assert_eq!(wakes(&count), woken);
}

/// An executor sends the shots of a call to two devices and binds each
/// part's reply, which a task of its caller awaits. Device A fails while
/// device B still runs: part A's failure, of the same call, does not wake
/// the task waiting on part B, which goes on as device B does; the reply
/// that gathers the parts ends the call.
#[test]
fn a_bound_reply_is_not_woken_when_another_reply_bound_for_its_call_fails() {
    let mut lifecycle = ExecutorLifecycle::default();
    let [(device_a, mut part_a), (_device_b, mut part_b)] = bound_call_of_two_parts(&mut lifecycle);
    let (count_b, waker_b) = counting();
    let status = part_b.poll_events(&mut Vec::new(), &waker_b);
    assert_eq!(status, ReplyStatus::Pending, "device B runs its shots");
    device_a.fail("lost".into());
    let status = poll(&mut part_a, &mut Vec::new());
    assert_eq!(
        status,
        ReplyStatus::Failed("lost".into()),
        "part A ends as device A failed"
    );
    assert_eq!(wakes(&count_b), 0);
}

/// A caller keeps three calls in flight and polls each of them in turn. The
/// third fails first, which leaves the second pending, and the caller polls
/// the second again; then the first fails: that failure wakes the second's
/// caller, though a later call's failure came in between.
#[test]
fn a_bound_reply_between_two_failed_calls_is_woken_by_the_earlier_one() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let (_second, mut second_reply) = bound_call(&mut lifecycle);
    let (third, mut third_reply) = bound_call(&mut lifecycle);
    let (count, waker) = counting();
    let status = second_reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(status, ReplyStatus::Pending, "the second call's work runs");
    third.fail("third".into());
    let status = poll(&mut third_reply, &mut Vec::new());
    assert_eq!(
        status,
        ReplyStatus::Failed("third".into()),
        "the third call's work failed"
    );
    let status = second_reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(
        status,
        ReplyStatus::Pending,
        "a later call's failure does not end the second call's reply"
    );
    let woken = wakes(&count);
    first.fail("first".into());
    poll(&mut first_reply, &mut Vec::new());
    assert!(wakes(&count) > woken);
}

/// Two calls are in flight; the second's device stops before its `call`
/// returns, so the executor records the failure there: the first call's
/// reply, which ran before it, ends as its own work does, with its events.
#[test]
fn a_bound_reply_is_unaffected_by_a_failure_recorded_in_a_later_call() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    lifecycle
        .admit()
        .expect("no failure is recorded before a reply is polled");
    lifecycle.fail("the device stopped");
    first.push(7);
    first.done();
    let mut out = Vec::new();
    assert_eq!(poll(&mut first_reply, &mut out), ReplyStatus::Done);
    assert_eq!(out, [7]);
}

/// Two calls are in flight; the second's device stops before its `call`
/// returns, and then the first call's reply fails as it is polled: the
/// executor's failure is the first call's, which left the state the second
/// ran on, though it was recorded last.
#[test]
fn the_earliest_failed_call_is_the_executors_failure_though_a_later_one_failed_in_call() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    lifecycle
        .admit()
        .expect("no failure is recorded before a reply is polled");
    lifecycle.fail("second");
    first.fail("first".into());
    poll(&mut first_reply, &mut Vec::new());
    assert_eq!(
        lifecycle.admit(),
        Err(Rejected::EarlierFailure("first".into()))
    );
}

/// A call's device stops before its `call` returns, and the executor
/// records it; the call's reply, bound, then fails with its worker's own
/// account of the stop: the executor's failure stays the first one
/// recorded, which says what went wrong first.
#[test]
fn the_first_of_a_calls_failures_is_the_executors_failure() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (worker, mut reply) = bound_call(&mut lifecycle);
    lifecycle.fail("the device stopped");
    worker.fail("the worker lost its device".into());
    let status = poll(&mut reply, &mut Vec::new());
    assert_eq!(
        status,
        ReplyStatus::Failed("the worker lost its device".into()),
        "the reply ends as its worker says"
    );
    assert_eq!(
        lifecycle.admit(),
        Err(Rejected::EarlierFailure("the device stopped".into()))
    );
}

/// The work of a later call, which polls an earlier call's reply first at
/// each of its own polls: the later call's reply learns of the earlier
/// call's failure during its own poll.
struct PollsFirst {
    earlier: LifecycleBoundReply<IncrementalReply<u32>>,
    /// The events to hand over, one per poll; the poll that hands over the
    /// last ends the reply.
    events: Vec<u32>,
    /// How the poll that hands over the last event ends the reply.
    ends_with: ReplyStatus,
}

/// Polls the earlier call's reply, then hands over the next of its own
/// events.
impl Reply for PollsFirst {
    type Item = u32;

    fn poll_events(&mut self, out: &mut Vec<u32>, waker: &Waker) -> ReplyStatus {
        let _ = self.earlier.poll_events(&mut Vec::new(), waker);
        out.push(self.events.remove(0));
        if self.events.is_empty() {
            self.ends_with.clone()
        } else {
            ReplyStatus::Pending
        }
    }
}

/// The second call's reply ends in the poll that learns of the first
/// call's failure: it ends with that failure, without the event of that
/// poll, since no later poll would end it.
#[test]
fn a_bound_reply_that_ends_as_an_earlier_call_fails_ends_with_that_failure() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, first_reply) = bound_call(&mut lifecycle);
    lifecycle
        .admit()
        .expect("no failure is recorded before a reply is polled");
    let mut second_reply = lifecycle.bind(PollsFirst {
        earlier: first_reply,
        events: vec![7],
        ends_with: ReplyStatus::Done,
    });
    first.fail("lost".into());
    let mut out = Vec::new();
    assert_eq!(
        poll(&mut second_reply, &mut out),
        ReplyStatus::Failed("an earlier call failed: lost".into())
    );
    assert_eq!(out, Vec::<u32>::new());
}

/// The second call's work fails in the poll that learns of the first
/// call's failure: the reply ends with the first call's failure, which left
/// the state the second call failed on, not with its own.
#[test]
fn a_bound_reply_that_fails_as_an_earlier_call_fails_ends_with_the_earlier_failure() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, first_reply) = bound_call(&mut lifecycle);
    lifecycle
        .admit()
        .expect("no failure is recorded before a reply is polled");
    let mut second_reply = lifecycle.bind(PollsFirst {
        earlier: first_reply,
        events: vec![7],
        ends_with: ReplyStatus::Failed("its device stopped".into()),
    });
    first.fail("lost".into());
    assert_eq!(
        poll(&mut second_reply, &mut Vec::new()),
        ReplyStatus::Failed("an earlier call failed: lost".into())
    );
}

/// A caller waits on the second of two calls, whose work learns of the
/// first call's failure during the poll and is left pending: the failure
/// came before the reply listed its caller's waker, so the reply wakes its
/// caller itself, and the caller's next poll ends it.
#[test]
fn a_bound_reply_left_pending_as_an_earlier_call_fails_wakes_its_caller() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, first_reply) = bound_call(&mut lifecycle);
    lifecycle
        .admit()
        .expect("no failure is recorded before a reply is polled");
    let mut second_reply = lifecycle.bind(PollsFirst {
        earlier: first_reply,
        events: vec![3, 7],
        ends_with: ReplyStatus::Done,
    });
    first.fail("lost".into());
    let (count, waker) = counting();
    let status = second_reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(status, ReplyStatus::Pending, "the second call's work runs");
    assert_ne!(wakes(&count), 0);
}

/// A caller keeps three calls in flight. The third call's work learns of
/// the second call's failure during a poll and is left pending; its next
/// poll ends it with that failure. The first call's failure, which comes
/// after, does not wake the third call's caller, who has nothing more to
/// poll.
#[test]
fn a_bound_reply_ended_by_an_earlier_failure_is_not_woken_by_a_still_earlier_one() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, mut first_reply) = bound_call(&mut lifecycle);
    let (second, second_reply) = bound_call(&mut lifecycle);
    lifecycle
        .admit()
        .expect("no failure is recorded before a reply is polled");
    let mut third_reply = lifecycle.bind(PollsFirst {
        earlier: second_reply,
        events: vec![3, 7],
        ends_with: ReplyStatus::Done,
    });
    second.fail("lost".into());
    let (count, waker) = counting();
    let status = third_reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(status, ReplyStatus::Pending, "the third call's work runs");
    let status = third_reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(
        status,
        ReplyStatus::Failed("an earlier call failed: lost".into()),
        "the second call's failure ends the third call's reply"
    );
    let woken = wakes(&count);
    first.fail("lost earlier".into());
    poll(&mut first_reply, &mut Vec::new());
    assert_eq!(wakes(&count), woken);
}

/// A caller waits on the second of two calls, gathering its events: the
/// reply handed over an event at its first poll, and ends at its second,
/// the poll that learns of the first call's failure. The event the caller
/// already holds stays with it; only the second poll's is dropped.
#[test]
fn a_bound_reply_that_ends_as_an_earlier_call_fails_keeps_the_events_of_its_earlier_polls() {
    let mut lifecycle = ExecutorLifecycle::default();
    let (first, first_reply) = bound_call(&mut lifecycle);
    lifecycle
        .admit()
        .expect("no failure is recorded before a reply is polled");
    let mut second_reply = lifecycle.bind(PollsFirst {
        earlier: first_reply,
        events: vec![3, 7],
        ends_with: ReplyStatus::Done,
    });
    let mut out = Vec::new();
    let status = poll(&mut second_reply, &mut out);
    assert_eq!(status, ReplyStatus::Pending, "the second call's work runs");
    first.fail("lost".into());
    let status = poll(&mut second_reply, &mut out);
    assert_eq!(
        status,
        ReplyStatus::Failed("an earlier call failed: lost".into()),
        "the poll that learns of the first call's failure ends the reply with it"
    );
    assert_eq!(out, [3]);
}

/// The executor of `ExecutorLifecycle`'s example, whose docs copy it: its
/// shots share one counter; a `true` counts, a `false` is where its device
/// stops, after the instructions before it ran.
#[derive(Default)]
struct Counter {
    count: usize,
    lifecycle: ExecutorLifecycle,
}

impl Executor<InstructionBatch<bool>> for Counter {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        self.lifecycle.admit()?;
        for &counts in batch.instructions {
            if !counts {
                // The count moved: a later call would run on a count its
                // caller never made.
                self.lifecycle.fail("the device stopped");
                return Ok(ImmediateReply::failed(
                    Vec::new(),
                    "the device stopped".into(),
                ));
            }
            self.count += batch.shots.len();
        }
        Ok(ImmediateReply::done(Vec::new()))
    }
}

impl Close for Counter {
    type Report = ();

    fn close(&mut self) {
        self.lifecycle.close();
    }
}

/// The example of `ExecutorLifecycle`'s docs: a counter whose device stops
/// in a call refuses the next call.
#[test]
fn a_counter_refuses_calls_after_its_device_stops() {
    let mut counter = Counter::default();
    let shots = ShotSet::one(0);
    let reply = counter
        .call(InstructionBatchRef {
            shots: &shots,
            instructions: &[true, false],
        })
        .expect("an open counter runs a call");
    assert_eq!(
        wait(reply, &mut Vec::new()),
        Err("the device stopped".into())
    );
    let next = InstructionBatchRef {
        shots: &shots,
        instructions: &[true],
    };
    assert_eq!(
        counter.call(next).err(),
        Some(Rejected::EarlierFailure("the device stopped".into()))
    );
}

/// An executor that hands each call's batch to a worker thread, and binds
/// the call's reply to its lifecycle: a `false` is where the worker's
/// device stops, which the executor learns of only as the reply is polled.
#[derive(Default)]
struct WorkerExecutor {
    lifecycle: ExecutorLifecycle,
}

impl Executor<InstructionBatch<bool>> for WorkerExecutor {
    type Reply = LifecycleBoundReply<IncrementalReply<ShotOutcome>>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        self.lifecycle.admit()?;
        let (sender, reply) = reply_channel();
        let stops = batch.instructions.contains(&false);
        thread::spawn(move || {
            if stops {
                sender.fail("the device stopped".into());
            } else {
                sender.done();
            }
        });
        Ok(self.lifecycle.bind(reply))
    }
}

impl Close for WorkerExecutor {
    type Report = ();

    fn close(&mut self) {
        self.lifecycle.close();
    }
}

/// An executor on a worker whose device stops in a call learns of it as
/// its caller waits on the reply, and from then on refuses every call, by
/// the conformance kit's check.
#[test]
fn an_executor_that_learns_of_a_failure_from_its_reply_refuses_later_calls() {
    conformance::refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut WorkerExecutor::default(),
        on_shot_0(&[true, false]),
        &[on_shot_0(&[true]), on_shot_0(&[true])],
    );
}

/// An ensemble executor built on its lifecycle refuses a batch with a call
/// outside its set, then runs the corrected batch, by the conformance kit's
/// check: a refused call ran nothing, though its lifecycle admitted it.
#[test]
fn a_shared_state_executor_accepts_calls_after_a_refusal() {
    let shots = shots(&[0]);
    let batches = flip_and_toss(&bits_set());
    let mut outside = batches[0].clone();
    outside.push(InstructionCall::new(
        batches[0][0].opcode,
        [Operand::Block(0), Operand::Block(1)],
    ));
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<InstructionCall>, _>(
        &mut SharedState::new(3),
        InstructionBatchRef {
            shots: &shots,
            instructions: &outside,
        },
        InstructionBatchRef {
            shots: &shots,
            instructions: &batches[0],
        },
    );
}

/// An ensemble executor built on its lifecycle refuses calls once closed,
/// by the conformance kit's check. The executor is the crate's test double:
/// this guards the compose tests, which run it as an executor that keeps
/// the kit's rules.
#[test]
fn a_shared_state_executor_refuses_calls_once_closed() {
    let (shots, batches) = (shots(&[0]), flip_and_toss(&bits_set()));
    conformance::rejects_calls_once_closed::<InstructionBatch<InstructionCall>, _>(
        &mut SharedState::new(3),
        InstructionBatchRef {
            shots: &shots,
            instructions: &batches[0],
        },
    );
}

/// An ensemble executor built on its lifecycle that ran a call reports its
/// one state at its first close only, by the conformance kit's check: a
/// caller that folds every report it receives counts the state once. The
/// executor is the crate's test double: this guards the compose test that
/// counts its report (`a_job_composes_with_any_executor`).
#[test]
fn a_shared_state_executor_reports_once() {
    let (shots, batches) = (shots(&[0]), flip_and_toss(&bits_set()));
    let mut executor = SharedState::new(3);
    executor
        .call(InstructionBatchRef {
            shots: &shots,
            instructions: &batches[0],
        })
        .expect("an open shared state runs a batch of its set");
    conformance::reports_once(&mut executor);
}

/// An ensemble executor built on its lifecycle, whose device stops in a
/// call on shot 0, refuses every later call, on shot 0 or on others, by the
/// conformance kit's check.
#[test]
fn a_shared_state_executor_refuses_calls_after_its_device_stops() {
    let (hit, others) = (shots(&[0]), shots(&[1, 2]));
    let batches = flip_and_toss(&bits_set());
    let mut executor = SharedState {
        fails_at: Some(1),
        ..SharedState::new(3)
    };
    conformance::refuses_calls_after_a_failure::<InstructionBatch<InstructionCall>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &hit,
            instructions: &batches[0],
        },
        &[
            InstructionBatchRef {
                shots: &hit,
                instructions: &batches[1],
            },
            InstructionBatchRef {
                shots: &others,
                instructions: &batches[0],
            },
        ],
    );
}

/// An ensemble executor built on its lifecycle refuses a call outside its
/// set at its index, by the conformance kit's check.
#[test]
fn a_shared_state_executor_refuses_calls_outside_its_set() {
    let flip = flip_and_toss(&bits_set())[0][0].clone();
    conformance::refuses_calls_outside_its_set(
        &mut SharedState::new(3),
        &shots(&[0, 1]),
        &bits_set(),
        &flip,
    );
}

/// An ensemble executor whose state carries over: a second call reads the
/// flip of the first, by the conformance kit's check of `resumable_shots`,
/// run on one shot. It offers no `resumable_shots` all the same: its shots
/// share one state, so a call changes the shots it does not name (the
/// kit's check of that rule is not run on it), and on two shots the check
/// of `resumable_shots` also checks that a shot new to the executor reads
/// as made when a call names it with shots an earlier call named, which the
/// shared state breaks the same way.
#[test]
fn a_shared_state_executor_continues_its_shots_from_one_call_to_the_next() {
    let flip_and_read = &flip_and_toss(&bits_set())[0];
    conformance::continues_a_shot_from_one_call_to_the_next(
        || SharedState::new(3),
        &shots(&[0]),
        &flip_and_read[..1],
        &flip_and_read[1..],
    );
}

/// An ensemble executor built after another flipped the bit its shots
/// share reads it as made: the conformance kit's check that a new
/// executor's shots start as made, whatever an earlier executor left,
/// which asks neither `resumable_shots` nor `replayable`, so it applies to an
/// ensemble whose shots share one state too.
#[test]
fn a_new_shared_state_executor_starts_its_shots_as_made() {
    let flip_and_read = &flip_and_toss(&bits_set())[0];
    conformance::starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || SharedState::new(3),
        &shots(&[0, 1]),
        &flip_and_read[..1],
        &flip_and_read[1..],
    );
}
