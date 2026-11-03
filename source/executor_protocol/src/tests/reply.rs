// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod asynchronous;
mod blocking;

use std::future::poll_fn;
use std::pin::Pin;
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Poll, Wake, Waker};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use futures_core::Stream;

use crate::{
    Close, Executor, IncrementalReply, InstructionBatch, InstructionBatchRef, Rejected, Reply,
    ReplySender, ReplyStatus, ShotId, ShotOutcome, ShotSet, async_events, reply_channel, wait,
    wait_async,
};

use super::{PATIENT, Worker, block_on, on_shot_0, outcome};

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

// --------------------------------------------------------- a caller that polls

/// The waker of one call among several: marks that call ready, then unparks
/// the caller.
struct Flag {
    index: usize,
    ready: Arc<Mutex<Vec<usize>>>,
    caller: Thread,
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
        self.caller.unpark();
    }
}

/// A caller keeps several calls in flight, sleeps until one of them wakes
/// it, and polls only the calls that woke it.
#[test]
fn a_caller_keeps_several_calls_in_flight_and_sleeps_until_one_wakes_it() {
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
                caller: thread::current(),
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

/// A caller that polls with the no-op waker does its own work between polls
/// and takes the events when it wants: nothing is lost, and it never calls
/// `wait`.
#[test]
fn a_caller_with_a_noop_waker_does_its_own_work_between_polls() {
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
        thread::sleep(Duration::from_millis(1)); // a unit of the caller's own work
        work += 1;
    }
    assert_eq!(out, [outcome(0, 0)]);
    assert!(work >= 1, "the caller worked while the call was pending");
}

// ----------------------------------------------------- one reply, four callers

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
        .map(|(tag, one)| ShotOutcome::new(0, tag, one.into()))
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
