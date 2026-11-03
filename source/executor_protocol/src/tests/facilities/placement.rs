// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::task::Waker;
use std::thread;

use super::placed_executor::{
    Op, Threads, ended, holding, polled_now, recording, seen, send, values,
};
use crate::{
    Executor, InstructionBatchRef, OwnThread, PlacementJob, Reply, ReplyStatus, ShotSet, Value,
};

/// A device's executor on a thread of its own (a device driver that
/// blocks, under a caller that awaits): every call runs on that thread,
/// one thread for all calls.
#[test]
fn on_its_own_thread_an_executor_runs_every_call_on_one_thread_not_the_callers() {
    let threads = Threads::default();
    let mut placed = recording(&threads).run_on(OwnThread);
    for _ in 0..3 {
        let (_, end) = send(&mut placed, &[0], &[Op::Flip]).expect("an open executor runs a call");
        end.expect("the logic does not fail");
    }
    let seen = seen(&threads);
    assert_eq!(seen.len(), 3, "one logic run per call");
    assert!(seen.iter().all(|&id| id == seen[0]), "one thread: {seen:?}");
    assert_ne!(seen[0], thread::current().id());
}

/// An executor on a pool's threads (`.run_on(|job| pool.spawn(job))`):
/// the placement is handed a job when calls are queued and none is
/// running, and that job runs every call queued, in order, so that a pool
/// never runs two calls of one executor at once.
#[test]
fn a_placement_is_handed_one_job_which_runs_every_call_queued_in_order() {
    let (placement, jobs) = holding();
    let mut placed = recording(&Threads::default()).run_on(placement);
    let shot = ShotSet::one(0);
    let batch = |instructions| InstructionBatchRef {
        shots: &shot,
        instructions,
    };
    let mut replies: Vec<_> = [&[Op::Flip][..], &[Op::Read(1)], &[Op::Read(2)]]
        .into_iter()
        .map(|ops| {
            placed
                .call(batch(ops))
                .expect("an open executor queues a call")
        })
        .collect();
    let mut out = Vec::new();
    assert_eq!(
        replies[0].poll_events(&mut out, Waker::noop()),
        ReplyStatus::Pending,
        "nothing ran yet"
    );
    let job = jobs
        .try_recv()
        .expect("the first call hands a job to the placement");
    assert!(jobs.try_recv().is_err(), "one job for the calls queued");
    job();
    for reply in &mut replies {
        assert_eq!(
            reply.poll_events(&mut out, Waker::noop()),
            ReplyStatus::Done
        );
    }
    assert_eq!(
        values(&out),
        [(0, 1, Value::One), (0, 2, Value::One)],
        "the flip ran first"
    );
    let reply = placed
        .call(batch(&[Op::Flip, Op::Read(3)]))
        .expect("an open executor queues a call");
    (jobs
        .try_recv()
        .expect("a call after the job ended hands a new job"))();
    let (out, end) = ended(reply);
    end.expect("the logic does not fail");
    assert_eq!(values(&out), [(0, 3, Value::Zero)]);
}

/// An executor on a placement that runs each job it is handed inline
/// (`|job: PlacementJob| job()`), as a test or a single-threaded tool
/// may: a call returns only once its work ran, on the caller's thread,
/// and its reply has ended.
#[test]
fn on_a_placement_that_runs_its_job_inline_a_call_returns_with_its_work_done() {
    let threads = Threads::default();
    let mut placed = recording(&threads).run_on(|job: PlacementJob| job());
    let reply = placed
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Flip, Op::Read(0)],
        })
        .expect("an open executor runs a call");
    assert_eq!(seen(&threads), [thread::current().id()]);
    let (outcomes, status) = polled_now(reply);
    assert_eq!(
        (values(&outcomes), status),
        (vec![(0, 0, Value::One)], ReplyStatus::Done)
    );
}
