// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Wake, Waker};

use crate::{
    Batch, Capabilities, Close, Completion, ErrorKind, Executor, Fold, Immediate, Need, Outcome,
    Rejected, RetryClass, Status, Unmet, Value, need_code, need_of, reason_code, retry_code,
    retry_of, value_code, value_of, wait,
};

struct NoWake;

impl Wake for NoWake {
    fn wake(self: Arc<Self>) {}
}

fn no_wake() -> Waker {
    Waker::from(Arc::new(NoWake))
}

#[test]
fn immediate_appends_its_events_then_ends() {
    let mut out = vec![1];
    assert_eq!(
        Immediate::done(vec![2, 3]).poll_events(&mut out, &no_wake()),
        Status::Done
    );
    assert_eq!(
        Immediate::failed(vec![4], "stopped".into()).poll_events(&mut out, &no_wake()),
        Status::Failed("stopped".into())
    );
    assert_eq!(out, [1, 2, 3, 4]);
}

/// What a test double's two ends share: the events pushed so far, whether
/// the run ended, and the waker of the last pending poll.
#[derive(Default)]
struct Shared {
    events: Vec<u32>,
    ended: bool,
    waker: Option<Waker>,
}

/// A completion whose events another thread pushes, as a worker would.
struct TestDoubleCompletion(Arc<Mutex<Shared>>);

impl Completion for TestDoubleCompletion {
    type Item = u32;

    fn poll_events(&mut self, out: &mut Vec<u32>, waker: &Waker) -> Status {
        let mut shared = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        out.append(&mut shared.events);
        if shared.ended {
            Status::Done
        } else {
            shared.waker = Some(waker.clone());
            Status::Pending
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
    assert_eq!(wait(TestDoubleCompletion(shared), &mut out), Ok(()));
    assert_eq!(out, (0..100).collect::<Vec<_>>());
    assert!(worker.join().is_ok());
}

#[test]
fn wait_returns_the_failure_after_the_events_before_it() {
    let mut out = Vec::new();
    assert_eq!(
        wait(Immediate::failed(vec![7], "lost".into()), &mut out),
        Err("lost".into())
    );
    assert_eq!(out, [7]);
}

/// A terminal of a one-instruction ISA: `true` reads One, `false` reads Zero.
struct TestDoubleExecutor;

impl Executor<Batch<bool>> for TestDoubleExecutor {
    type Response = Immediate<Outcome>;

    fn call(&mut self, batch: &[bool]) -> Result<Self::Response, Rejected> {
        let outcomes = (0u32..)
            .zip(batch)
            .map(|(tag, &one)| Outcome {
                tag,
                value: one.into(),
            })
            .collect();
        Ok(Immediate::done(outcomes))
    }
}

#[test]
fn a_batch_is_a_borrowed_slice_of_the_executors_instructions() {
    let mut out = Vec::new();
    let reply = TestDoubleExecutor.call(&[true, false]);
    assert!(reply.is_ok_and(|reply| wait(reply, &mut out).is_ok()));
    assert_eq!(
        out,
        [
            Outcome {
                tag: 0,
                value: Value::One
            },
            Outcome {
                tag: 1,
                value: Value::Zero
            }
        ]
    );
}

#[test]
fn only_unavailable_and_no_reply_may_be_resent() {
    let unmet = Unmet {
        component: "terminal".into(),
        need: Need::Qubits,
        needed: 9,
        offered: 8,
    };
    let never = [
        Rejected::NoShots,
        Rejected::ShotNotInSession(3),
        Rejected::RepeatedShot(3),
        Rejected::IdReused(4),
        Rejected::Instruction {
            at: 1,
            reason: "qubit 9".into(),
        },
        Rejected::Closed,
        Rejected::SessionFailed("earlier".into()),
        Rejected::Malformed("bytes".into()),
        Rejected::Unmet(unmet),
    ];
    assert!(never.iter().all(|r| r.retry() == RetryClass::Never));
    assert_eq!(
        Rejected::Unavailable("busy".into()).retry(),
        RetryClass::Later
    );
    assert_eq!(
        Rejected::NoReply("timeout".into()).retry(),
        RetryClass::Resend
    );
    let unrecognized = Rejected::Unrecognized {
        retry: RetryClass::Later,
        reason: "newer".into(),
    };
    assert_eq!(unrecognized.retry(), RetryClass::Later);
    assert_eq!(unrecognized.to_string(), "newer");
}

#[test]
fn a_rejection_says_what_was_refused() {
    let unmet = Rejected::Unmet(Unmet {
        component: "terminal".into(),
        need: Need::Qubits,
        needed: 9,
        offered: 8,
    });
    assert_eq!(unmet.to_string(), "terminal offers 8 Qubits, 9 needed");
    let instruction = Rejected::Instruction {
        at: 2,
        reason: "qubit 9 is beyond the 8 qubits".into(),
    };
    assert_eq!(
        instruction.to_string(),
        "instruction 2: qubit 9 is beyond the 8 qubits"
    );
}

#[test]
fn reason_codes_are_fixed() {
    let unmet = Unmet {
        component: String::new(),
        need: Need::Feedback,
        needed: 1,
        offered: 0,
    };
    let reasons = [
        (Rejected::NoShots, 0),
        (Rejected::ShotNotInSession(1), 1),
        (Rejected::RepeatedShot(1), 2),
        (Rejected::IdReused(1), 3),
        (
            Rejected::Instruction {
                at: 0,
                reason: String::new(),
            },
            4,
        ),
        (Rejected::Closed, 5),
        (Rejected::Unavailable(String::new()), 6),
        (Rejected::Malformed(String::new()), 7),
        (Rejected::Unmet(unmet), 8),
        (Rejected::SessionFailed(String::new()), 9),
        (Rejected::NoReply(String::new()), 10),
        (
            Rejected::Unrecognized {
                retry: RetryClass::Never,
                reason: String::new(),
            },
            255,
        ),
    ];
    for (rejected, code) in reasons {
        assert_eq!(reason_code(&rejected), code, "{rejected:?}");
    }
}

#[test]
fn each_need_retry_class_and_value_decodes_back_to_itself() {
    let needs = [
        (Need::Qubits, 0),
        (Need::Feedback, 1),
        (Need::Resets, 2),
        (Need::States, 3),
        (Need::Replayable, 4),
        (Need::Probabilities, 5),
    ];
    for (need, code) in needs {
        assert_eq!(need_code(need), code);
        assert_eq!(need_of(code), Some(need));
    }
    assert_eq!(need_of(6), None, "an unknown need is not guessed");
    for (retry, code) in [
        (RetryClass::Never, 0),
        (RetryClass::Later, 1),
        (RetryClass::Resend, 2),
    ] {
        assert_eq!(retry_code(retry), code);
        assert_eq!(retry_of(code), Some(retry));
    }
    assert_eq!(retry_of(3), None);
    let values = [
        (Value::Zero, 0),
        (Value::One, 1),
        (Value::Error(ErrorKind::Loss), 2),
        (Value::Error(ErrorKind::Other), 3),
    ];
    for (value, code) in values {
        assert_eq!(value_code(value), code);
        assert_eq!(value_of(code), Some(value));
    }
    assert_eq!(value_of(4), None);
}

#[test]
fn default_capabilities_state_no_limit() {
    let offered = Capabilities::default();
    assert_eq!(offered.qubits, None);
    assert!(offered.feedback);
    assert_eq!(offered.states, None);
    assert!(offered.replayable);
    let bounded = Capabilities::default()
        .with_qubits(Some(8))
        .with_feedback(false)
        .with_states(Some(1))
        .with_replayable(false);
    assert_eq!(
        (
            bounded.qubits,
            bounded.feedback,
            bounded.states,
            bounded.replayable
        ),
        (Some(8), false, Some(1), false)
    );
}

/// A report that counts, as a simulator's would.
#[derive(Debug, Default, PartialEq)]
struct Count(u64);

impl Fold for Count {
    fn fold(&mut self, other: Self) {
        self.0 += other.0;
    }
}

/// An executor that counts its calls and reports them once.
#[derive(Default)]
struct TestDoubleCounter {
    calls: u64,
    closed: bool,
}

impl Executor<Batch<bool>> for TestDoubleCounter {
    type Response = Immediate<Outcome>;

    fn call(&mut self, _: &[bool]) -> Result<Self::Response, Rejected> {
        if self.closed {
            return Err(Rejected::Closed);
        }
        self.calls += 1;
        Ok(Immediate::done(Vec::new()))
    }
}

impl Close for TestDoubleCounter {
    type Report = Count;

    fn close(&mut self) -> Count {
        self.closed = true;
        Count(std::mem::take(&mut self.calls))
    }
}

#[test]
fn a_closed_executor_rejects_and_reports_once() {
    let mut executor = TestDoubleCounter::default();
    assert!(executor.call(&[]).is_ok());
    assert!(executor.call(&[true]).is_ok());
    let mut total = executor.close();
    assert_eq!(executor.call(&[]).err(), Some(Rejected::Closed));
    total.fold(executor.close());
    assert_eq!(total, Count(2));
}
