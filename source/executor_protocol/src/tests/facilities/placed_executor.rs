// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Wake, Waker};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use crate::tests::facilities::executor_fn::{
    Freed, Op as DrawingOp, PowerDownFailed, PowersDownBadly, ReleasesBadly, draw_value,
    freed_so_far, powering_down_badly, read_drawn_bits,
};
use crate::tests::{Count, PATIENT, block_on, ends_soon, outcome, signalling};
use crate::{
    CallInput, Capabilities, Close, Describe, Executor, ExecutorFn, ExecutorLifecycle,
    ExecutorPlacement, ExecutorReport, Fold, ImmediateReply, IncrementalReply, InstructionBatch,
    InstructionBatchRef, LogP, MakeState, NoCheck, OnePerShot, Outcome, Outcomes, OwnThread,
    PlacedExecutor, PlacementJob, Rejected, Reply, ReplySender, ReplyStatus, ShotContext, ShotId,
    ShotOutcome, ShotSet, TypedInstruction, Value, conformance, executor_fn, reply_channel, wait,
    wait_async,
};

/// An instruction of the tests' one-bit device.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::tests) enum Op {
    /// Flips the bit.
    Flip,
    /// Reads the bit, under a tag.
    Read(u32),
    /// Reads the bit's complement, under a tag: another instruction than
    /// `Read` that reads, as the conformance kit's tag checks need; of the
    /// same bit, the device's only one.
    ReadInverted(u32),
    /// Where the device stops: the logic fails.
    Stop,
    /// Where the logic has a bug: it panics.
    Panic,
}

impl TypedInstruction for Op {}

/// The threads the logic ran on, once per shot of each call, in the
/// order it ran.
pub(in crate::tests) type Threads = Arc<Mutex<Vec<ThreadId>>>;

/// A shot's state: one bit, zero when made, and where the logic records
/// the thread it runs on.
pub(in crate::tests) struct Bit {
    pub(in crate::tests) one: bool,
    pub(in crate::tests) threads: Threads,
}

/// The logic: records its thread, flips and reads the bit; fails at
/// `Stop`, after the readouts before it; panics at `Panic`.
pub(in crate::tests) fn run_bit(
    bit: &mut Bit,
    input: &CallInput<'_, Op>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    bit.threads
        .lock()
        .expect("no test thread panics holding it")
        .push(thread::current().id());
    for op in input.instructions() {
        match *op {
            Op::Flip => bit.one = !bit.one,
            Op::Read(tag) => outcomes.push(Outcome::new(tag, bit.one.into())),
            Op::ReadInverted(tag) => outcomes.push(Outcome::new(tag, (!bit.one).into())),
            Op::Stop => return Err("the device stopped".to_owned()),
            Op::Panic => panic!("the logic has a bug"),
        }
    }
    Ok(())
}

/// The tests' device's logic.
pub(in crate::tests) type RunBit =
    fn(&mut Bit, &CallInput<'_, Op>, &mut Outcomes<'_>) -> Result<(), String>;

/// The tests' device's log probability: the bit's value.
pub(in crate::tests) type BitLogP = fn(&Bit) -> f64;

/// The device's executor, one state, whose logic records in `threads`
/// the thread it runs on; its log probability is the bit's value. It
/// promises what the bit keeps: each shot from one call to the next, and
/// the same readouts alone as among others.
pub(in crate::tests) fn recording(
    threads: &Threads,
) -> ExecutorFn<Op, Bit, impl MakeState<Bit> + use<>, RunBit, NoCheck<Op>, BitLogP> {
    let threads = Arc::clone(threads);
    executor_fn(
        Capabilities::default()
            .with_resumable_shots(true)
            .with_replayable(true),
        move |_| {
            Ok(Bit {
                one: false,
                threads: Arc::clone(&threads),
            })
        },
        run_bit as RunBit,
    )
    .log_probability((|bit| f64::from(u8::from(bit.one))) as BitLogP)
}

/// The reply of `executor` to `ops` on `shots`, once it ended: its events
/// and its end; or the refusal.
pub(in crate::tests) fn send(
    executor: &mut impl Executor<InstructionBatch<Op>, Reply: Send + 'static>,
    shots: &[ShotId],
    ops: &[Op],
) -> Result<(Vec<ShotOutcome>, Result<(), String>), Rejected> {
    let shots = ShotSet::new(shots.to_vec()).expect("the tests name each shot once");
    let reply = executor.call(InstructionBatchRef {
        shots: &shots,
        instructions: ops,
    })?;
    Ok(ended(reply))
}

/// The events and the end of `reply`, waited for on a thread of its own,
/// so that a reply that never ends fails the test instead of hanging it.
pub(in crate::tests) fn ended(
    reply: impl Reply<Item = ShotOutcome> + Send + 'static,
) -> (Vec<ShotOutcome>, Result<(), String>) {
    ends_soon(move || {
        let mut outcomes = Vec::new();
        let end = wait(reply, &mut outcomes);
        (outcomes, end)
    })
}

/// The outcomes and the status of one poll of `reply`, now, with no waker:
/// how a test sees that a reply ended already.
pub(in crate::tests) fn polled_now(
    mut reply: impl Reply<Item = ShotOutcome>,
) -> (Vec<ShotOutcome>, ReplyStatus) {
    let mut outcomes = Vec::new();
    let status = reply.poll_events(&mut outcomes, Waker::noop());
    (outcomes, status)
}

/// The `(shot, tag, value)` of each of `outcomes`.
pub(in crate::tests) fn values(outcomes: &[ShotOutcome]) -> Vec<(ShotId, u32, Value)> {
    outcomes.iter().map(|o| (o.shot, o.tag, o.value)).collect()
}

/// The threads recorded so far.
pub(in crate::tests) fn seen(threads: &Threads) -> Vec<ThreadId> {
    threads
        .lock()
        .expect("no test thread panics holding it")
        .clone()
}

/// A placement that keeps the jobs it is handed, for the test to run; its
/// copies, one per shot under a state per shot, keep theirs in the same
/// queue.
pub(in crate::tests) fn holding() -> (
    impl FnMut(PlacementJob) + Clone + Send + 'static,
    mpsc::Receiver<PlacementJob>,
) {
    let (jobs, held) = mpsc::channel();
    (
        move |job| {
            jobs.send(job).expect("the test holds the jobs' receiver");
        },
        held,
    )
}

/// The states' report, as threads: for each state, the thread its first
/// call ran on, and the thread its report was made on.
#[derive(Debug, Default)]
pub(in crate::tests) struct ReportedOn(pub(in crate::tests) Vec<(ThreadId, ThreadId)>);

impl Fold for ReportedOn {
    fn fold(&mut self, other: Self) {
        self.0.extend(other.0);
    }
}

/// The report of `bit`'s state, made on this thread.
pub(in crate::tests) fn reported_on(bit: &Bit) -> ReportedOn {
    let ran_on = *seen(&bit.threads).first().expect("the state ran a call");
    ReportedOn(vec![(ran_on, thread::current().id())])
}

/// The state closure of a device whose state must be reported where its
/// calls ran: a bit, zero when made, that records its own calls' threads.
pub(in crate::tests) fn own_bit(_: ShotContext) -> Result<Bit, Rejected> {
    Ok(Bit {
        one: false,
        threads: Threads::default(),
    })
}

/// A device's state that holds the device until it is freed (dropped),
/// which it then says; each call it runs says it started, then waits at a
/// gate the test opens ([`gated_device`]).
pub(in crate::tests) struct HeldDevice {
    bit: Bit,
    started: mpsc::Sender<()>,
    gate: Arc<Mutex<mpsc::Receiver<()>>>,
    freed: mpsc::Sender<()>,
}

impl Drop for HeldDevice {
    fn drop(&mut self) {
        let _ = self.freed.send(());
    }
}

/// A [`gated_device`]'s executor.
pub(in crate::tests) type GatedDevice = ExecutorFn<Op, HeldDevice, MakeHeld, RunHeld>;

/// A [`gated_device`]'s states, made boxed so that its type has a name.
pub(in crate::tests) type MakeHeld =
    Box<dyn FnMut(ShotContext) -> Result<HeldDevice, Rejected> + Send>;

/// The logic of a [`gated_device`]: [`run_bit`]'s, once the gate opens.
pub(in crate::tests) type RunHeld =
    fn(&mut HeldDevice, &CallInput<'_, Op>, &mut Outcomes<'_>) -> Result<(), String>;

/// Says it started, waits at the gate, then runs [`run_bit`].
fn run_held(
    device: &mut HeldDevice,
    input: &CallInput<'_, Op>,
    out: &mut Outcomes<'_>,
) -> Result<(), String> {
    device
        .started
        .send(())
        .expect("the test outlives the device");
    (device.gate.lock().expect("no call panics holding the gate"))
        // Longer than a test waits, so that a call the test did not let
        // run fails the test before the call does.
        .recv_timeout(PATIENT * 2)
        .expect("the test opens the gate for each call it lets run");
    run_bit(&mut device.bit, input, out)
}

/// The test's handles on a [`gated_device`]: each call says it started,
/// then waits for the test to open the gate once; the state says when it
/// is freed.
pub(in crate::tests) struct Gate {
    pub(in crate::tests) started: mpsc::Receiver<()>,
    pub(in crate::tests) open: mpsc::Sender<()>,
    pub(in crate::tests) freed: mpsc::Receiver<()>,
}

/// A device whose every call, once started, waits at a gate the test opens
/// (a long call, such as a device's run of a whole batch), and whose state
/// says when it is freed, with the test's handles on both; its logic is
/// [`run_bit`]'s.
pub(in crate::tests) fn gated_device() -> (Gate, GatedDevice) {
    let (started, has_started) = mpsc::channel();
    let (open, gate) = mpsc::channel();
    let gate = Arc::new(Mutex::new(gate));
    let (freed, was_freed) = mpsc::channel();
    let make: MakeHeld = Box::new(move |_| {
        Ok(HeldDevice {
            bit: Bit {
                one: false,
                threads: Threads::default(),
            },
            started: started.clone(),
            gate: Arc::clone(&gate),
            freed: freed.clone(),
        })
    });
    let executor = executor_fn(Capabilities::default(), make, run_held as RunHeld);
    let handles = Gate {
        started: has_started,
        open,
        freed: was_freed,
    };
    (handles, executor)
}

/// A simulator of a whole run placed whole, `.shots(..).run_on(..)`: the
/// placement is applied last, outside, so every shot runs on its one thread.
#[test]
fn placed_after_one_per_shot_every_shot_runs_on_the_one_thread() {
    let threads = Threads::default();
    let mut placed = recording(&threads).shots(OnePerShot).run_on(OwnThread);
    let (outcomes, end) = send(&mut placed, &[0, 1, 2], &[Op::Flip, Op::Read(0)])
        .expect("an open executor runs a call");
    end.expect("the logic does not fail");
    assert_eq!(
        values(&outcomes),
        [(0, 0, Value::One), (1, 0, Value::One), (2, 0, Value::One)]
    );
    let seen = seen(&threads);
    assert_eq!(seen.len(), 3, "one logic run per shot");
    assert!(seen.iter().all(|&id| id == seen[0]), "one thread: {seen:?}");
    assert_ne!(seen[0], thread::current().id());
}

/// A run's executor moved to the caller's thread, and a reply waited on
/// a thread of its own: a placed executor and its replies are `Send`.
#[test]
fn a_placed_executor_and_its_replies_move_to_other_threads() {
    let threads = Threads::default();
    let mut placed = recording(&threads).run_on(OwnThread);
    let (_placed, reply) = thread::spawn(move || {
        let shot = ShotSet::one(0);
        let reply = placed
            .call(InstructionBatchRef {
                shots: &shot,
                instructions: &[Op::Flip, Op::Read(3)],
            })
            .expect("an open executor queues a call");
        // Kept until its reply ends: dropped, it would cancel the call.
        (placed, reply)
    })
    .join()
    .expect("the caller's thread does not panic");
    let (outcomes, end) = ended(reply);
    end.expect("the logic does not fail");
    assert_eq!(values(&outcomes), [(0, 3, Value::One)]);
}

/// A caller that awaits (`run.await`) over an executor on its own thread:
/// the reply wakes the task when the logic has run.
#[test]
fn an_async_task_awaits_the_reply_of_an_executor_on_its_own_thread() {
    let (go, gate) = mpsc::channel::<()>();
    let mut placed = executor_fn(
        Capabilities::default(),
        |_| {
            Ok(Bit {
                one: false,
                threads: Threads::default(),
            })
        },
        move |bit: &mut Bit, input: &CallInput<'_, Op>, outcomes: &mut Outcomes<'_>| {
            gate.recv_timeout(PATIENT).expect("the test opens the gate");
            run_bit(bit, input, outcomes)
        },
    )
    .run_on(OwnThread);
    let shot = ShotSet::one(0);
    let reply = placed
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Flip, Op::Read(1)],
        })
        .expect("an open executor queues a call");
    let opener = thread::spawn(move || {
        thread::sleep(Duration::from_millis(10));
        go.send(()).expect("the logic waits at the gate");
    });
    let (outcomes, end) = ends_soon(move || block_on(wait_async(reply)));
    opener.join().expect("the opener does not panic");
    assert_eq!((values(&outcomes), end), (vec![(0, 1, Value::One)], Ok(())));
}

/// A run that ends while its executor's calls are still queued: `close`
/// waits for them, then reports what the executor it placed reports, and
/// the replies end with every outcome.
#[test]
fn closing_waits_for_the_calls_queued_then_gives_the_placed_executors_report() {
    let threads = Threads::default();
    let mut placed = recording(&threads).run_on(|job: PlacementJob| {
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            job();
        });
    });
    let shot = ShotSet::one(0);
    let replies: Vec<_> = (0..3)
        .map(|_| {
            placed
                .call(InstructionBatchRef {
                    shots: &shot,
                    instructions: &[Op::Flip, Op::Read(0)],
                })
                .expect("an open executor queues a call")
        })
        .collect();
    let report = ends_soon(move || placed.close());
    let mut log_p = LogP::default();
    log_p.add(0, 1.0);
    assert_eq!(
        report,
        ExecutorReport {
            calls: 3,
            log_p,
            states: (),
        }
    );
    for (reply, value) in replies
        .into_iter()
        .zip([Value::One, Value::Zero, Value::One])
    {
        let mut out = Vec::new();
        let mut reply = reply;
        assert_eq!(
            reply.poll_events(&mut out, Waker::noop()),
            ReplyStatus::Done,
            "ended before close returned"
        );
        assert_eq!(values(&out), [(0, 0, value)]);
    }
}

/// A caller that wants only the report sends three calls and drops their
/// replies at once: a dropped reply does not stop its call's work, so
/// `close` waits for the three calls queued, and reports them.
#[test]
fn closing_waits_for_the_calls_whose_replies_were_dropped() {
    let mut placed = recording(&Threads::default()).run_on(|job: PlacementJob| {
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            job();
        });
    });
    let shot = ShotSet::one(0);
    for _ in 0..3 {
        let reply = placed
            .call(InstructionBatchRef {
                shots: &shot,
                instructions: &[Op::Flip, Op::Read(0)],
            })
            .expect("an open executor queues a call");
        drop(reply);
    }
    let report = ends_soon(move || placed.close());
    assert_eq!(report.calls, 3);
}

/// An executor that ran a call on the caller's thread, then was placed
/// and closed with no further call (a run that ends at once): `close`
/// gives the placed executor's report, its call and its state's log
/// probability, on the caller's thread: the placement is handed no job.
#[test]
fn closing_before_any_call_gives_the_placed_executors_report() {
    let mut one_state = recording(&Threads::default());
    let shot = ShotSet::one(0);
    let reply = one_state
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Flip],
        })
        .expect("an open executor runs a call");
    ended(reply).1.expect("the logic does not fail");
    let (placement, jobs) = holding();
    let mut placed = one_state.run_on(placement);
    let mut log_p = LogP::default();
    log_p.add(0, 1.0);
    assert_eq!(
        ends_soon(move || placed.close()),
        ExecutorReport {
            calls: 1,
            log_p,
            states: (),
        }
    );
    assert!(jobs.try_recv().is_err(), "no job was handed");
}

/// A device bound to a thread (a GPU's context, a device driver's handle): the
/// device's `executor`, whose close records in `closed_on` the thread it
/// runs on.
struct ThreadBound<E> {
    executor: E,
    closed_on: Threads,
}

impl<E: Executor<InstructionBatch<Op>>> Executor<InstructionBatch<Op>> for ThreadBound<E> {
    type Reply = E::Reply;
    fn call(&mut self, batch: InstructionBatchRef<'_, Op>) -> Result<E::Reply, Rejected> {
        self.executor.call(batch)
    }
}

impl<E: Describe> Describe for ThreadBound<E> {
    fn capabilities(&self) -> Capabilities {
        self.executor.capabilities()
    }
}

impl<E: Close> Close for ThreadBound<E> {
    type Report = E::Report;
    fn close(&mut self) -> E::Report {
        self.closed_on
            .lock()
            .expect("no test thread panics holding it")
            .push(thread::current().id());
        self.executor.close()
    }
}

/// A device bound to a thread, on a thread of its own, closed once it
/// ran a call: `close` closes it on that thread, where its call ran, not
/// on the caller's, so that it releases what it holds where it made it.
#[test]
fn once_called_closing_closes_the_executor_it_places_on_its_thread() {
    let (threads, closed_on) = (Threads::default(), Threads::default());
    let device = ThreadBound {
        executor: recording(&threads),
        closed_on: Arc::clone(&closed_on),
    };
    let mut placed = PlacedExecutor::new(device, OwnThread, ExecutorLifecycle::default(), drop);
    let (_, end) = send(&mut placed, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let caller = ends_soon(move || {
        placed.close();
        thread::current().id()
    });
    let ran_on = seen(&threads);
    assert_ne!(ran_on, [caller], "the call ran on its own thread");
    assert_eq!(seen(&closed_on), ran_on, "closed once, where its call ran");
}

/// A device bound to a thread, placed on a thread of its own and closed
/// before any call (a run that ends at once): the placement was never
/// started, so `close` closes it on the caller's thread, where it is.
#[test]
fn before_any_call_closing_closes_the_executor_it_places_on_the_callers_thread() {
    let closed_on = Threads::default();
    let device = ThreadBound {
        executor: recording(&Threads::default()),
        closed_on: Arc::clone(&closed_on),
    };
    let mut placed = PlacedExecutor::new(device, OwnThread, ExecutorLifecycle::default(), drop);
    let caller = ends_soon(move || {
        placed.close();
        thread::current().id()
    });
    assert_eq!(seen(&closed_on), [caller]);
}

/// A one-state executor called on its thread with a shot it does not
/// hold: its refusal comes after the call was queued, too late to refuse
/// it, so the reply fails with it.
#[test]
fn a_call_its_executor_refuses_on_its_thread_fails_its_reply() {
    let mut placed = recording(&Threads::default()).run_on(OwnThread);
    let (_, end) = send(&mut placed, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let (outcomes, end) =
        send(&mut placed, &[1], &[Op::Read(0)]).expect("an open executor queues a call");
    assert!(outcomes.is_empty());
    assert_eq!(end, Err(Rejected::ShotNotHeld(1).to_string()));
}

/// After a refusal on its thread, the executor refuses every later call:
/// the calls before it moved the state, and the refused one did not run.
#[test]
fn after_a_refusal_on_its_thread_it_refuses_every_later_call() {
    let mut placed = recording(&Threads::default()).run_on(OwnThread);
    let (_, end) = send(&mut placed, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let (_, end) = send(&mut placed, &[1], &[Op::Read(0)]).expect("an open executor queues a call");
    end.expect_err("shot 1 is not held");
    assert_eq!(
        send(&mut placed, &[0], &[Op::Read(0)]).err(),
        Some(Rejected::EarlierFailure(
            Rejected::ShotNotHeld(1).to_string()
        ))
    );
}

/// A waker that, woken, tells the test (`woken`) and holds the thread
/// that woke it until the test resumes it, or for [`PATIENT`]; a second
/// wake finds the test gone on, and does not wait.
struct Pauses {
    woken: mpsc::Sender<()>,
    resume: Mutex<mpsc::Receiver<()>>,
}

impl Wake for Pauses {
    fn wake(self: Arc<Self>) {
        if self.woken.send(()).is_ok() {
            let resume = self
                .resume
                .lock()
                .expect("no test thread panics holding it");
            let _ = resume.recv_timeout(PATIENT);
        }
    }
}

/// A [`Pauses`] waker, the receiver that tells it was woken, and the
/// sender that resumes it.
fn pausing() -> (Waker, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (woken, told) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let pauses = Waker::from(Arc::new(Pauses {
        woken,
        resume: Mutex::new(resumed),
    }));
    (pauses, told, resume)
}

/// A caller woken on its executor's thread by a reply that fails (a
/// refusal there), which calls again at once from the waker: the executor
/// is failed already when the reply is, so that call is refused. The
/// waker holds the executor's thread until that call returned.
#[test]
fn a_caller_woken_by_a_failed_reply_finds_the_executor_failed() {
    let (placement, jobs) = holding();
    let mut placed = recording(&Threads::default()).run_on(placement);
    let mut read_on = |j| {
        placed.call(InstructionBatchRef {
            shots: &ShotSet::one(j),
            instructions: &[Op::Read(0)],
        })
    };
    let first = read_on(0).expect("an open executor queues a call");
    let mut refused = read_on(1).expect("an open executor queues a call");
    let (pauses, told, resume) = pausing();
    let mut out = Vec::new();
    assert_eq!(
        refused.poll_events(&mut out, &pauses),
        ReplyStatus::Pending,
        "queued, not run"
    );
    let job = jobs.try_recv().expect("the first call hands a job");
    let thread = thread::spawn(job);
    told.recv_timeout(PATIENT)
        .expect("the refusal on the thread wakes the caller");
    let again = read_on(0).err();
    resume.send(()).expect("the waker waits to resume");
    thread.join().expect("the job does not panic");
    ended(first).1.expect("the logic does not fail");
    assert_eq!(
        again,
        Some(Rejected::EarlierFailure(
            Rejected::ShotNotHeld(1).to_string()
        ))
    );
}

/// The executor's contract after a failed logic, through the
/// conformance kit.
#[test]
fn after_a_failed_call_it_refuses_every_later_call() {
    let mut placed = recording(&Threads::default()).run_on(OwnThread);
    ends_soon(move || {
        let shot = ShotSet::one(0);
        conformance::refuses_calls_after_a_failure(
            &mut placed,
            InstructionBatchRef {
                shots: &shot,
                instructions: &[Op::Stop],
            },
            &[
                InstructionBatchRef {
                    shots: &shot,
                    instructions: &[Op::Read(0)],
                },
                InstructionBatchRef {
                    shots: &shot,
                    instructions: &[Op::Read(0)],
                },
            ],
        );
    });
}

/// A caller sends a call that stops the device, then a read before it
/// waits for the first reply: the read is refused, naming the stop, when
/// the failure is recorded already, or else queued behind the stop on the
/// executor's thread, and then ends with the failure, handing over
/// nothing. The conformance kit's check.
#[test]
fn calls_in_flight_end_at_an_earlier_failure() {
    let mut placed = recording(&Threads::default()).run_on(OwnThread);
    ends_soon(move || {
        let shot = ShotSet::one(0);
        conformance::ends_calls_in_flight_at_an_earlier_failure(
            &mut placed,
            InstructionBatchRef {
                shots: &shot,
                instructions: &[Op::Stop],
            },
            InstructionBatchRef {
                shots: &shot,
                instructions: &[Op::Read(0)],
            },
        );
    });
}

/// A caller that keeps several calls in flight: a call queued behind one
/// that fails does not run, since the state is part-way through the
/// failed one; its reply ends with that failure.
#[test]
fn a_call_queued_behind_a_failed_one_fails_without_running() {
    let threads = Threads::default();
    let (placement, jobs) = holding();
    let mut placed = recording(&threads).run_on(placement);
    let shot = ShotSet::one(0);
    let failing = placed
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Read(0), Op::Stop],
        })
        .expect("an open executor queues a call");
    let behind = placed
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Read(1)],
        })
        .expect("nothing failed yet when it is queued");
    (jobs.try_recv().expect("the first call hands a job"))();
    let (out, end) = ended(failing);
    assert_eq!(end, Err("the device stopped".to_owned()));
    assert_eq!(
        values(&out),
        [(0, 0, Value::Zero)],
        "the outcomes before the failure"
    );
    let (out, end) = ended(behind);
    assert_eq!(
        end,
        Err("an earlier call failed: the device stopped".to_owned())
    );
    assert!(out.is_empty());
    assert_eq!(seen(&threads).len(), 1, "only the failing call ran");
}

/// A caller that keeps three calls in flight, the first of which fails:
/// the two queued behind it never reach the executor it places, whose
/// report it gives, so the report counts the one call that did.
#[test]
fn its_report_counts_only_the_calls_that_reached_the_executor_it_places() {
    let (placement, jobs) = holding();
    let mut placed = recording(&Threads::default()).run_on(placement);
    let shot = ShotSet::one(0);
    let replies: Vec<_> = [&[Op::Stop][..], &[Op::Flip], &[Op::Flip]]
        .into_iter()
        .map(|instructions| {
            let reply = placed.call(InstructionBatchRef {
                shots: &shot,
                instructions,
            });
            reply.expect("nothing failed yet when it is queued")
        })
        .collect();
    (jobs.try_recv().expect("the first call hands a job"))();
    for reply in replies {
        ended(reply).1.expect_err("the first call failed");
    }
    let closing = thread::spawn(move || placed.close());
    let job = (jobs.recv_timeout(PATIENT)).expect("the close hands a job");
    job();
    let report = ends_soon(move || closing.join().expect("the close does not panic"));
    assert_eq!(report.calls, 1);
}

/// A caller that keeps several calls in flight, one of which names a shot
/// the executor does not hold: the calls queued behind it fail without
/// running, as behind any failure.
#[test]
fn a_call_queued_behind_one_refused_on_its_thread_fails_without_running() {
    let threads = Threads::default();
    let (placement, jobs) = holding();
    let mut placed = recording(&threads).run_on(placement);
    let mut read_on = |shot: &ShotSet| {
        let reply = placed.call(InstructionBatchRef {
            shots: shot,
            instructions: &[Op::Read(0)],
        });
        reply.expect("nothing failed yet when it is queued")
    };
    let first = read_on(&ShotSet::one(0));
    let refused = read_on(&ShotSet::one(1));
    let behind = read_on(&ShotSet::one(0));
    (jobs.try_recv().expect("the first call hands a job"))();
    ended(first).1.expect("the logic does not fail");
    let refusal = Rejected::ShotNotHeld(1).to_string();
    assert_eq!(ended(refused).1, Err(refusal.clone()));
    let (out, end) = ended(behind);
    assert_eq!(end, Err(Rejected::EarlierFailure(refusal).to_string()));
    assert!(out.is_empty());
    assert_eq!(seen(&threads).len(), 1, "only the first call ran");
}

/// A logic with a bug panics on the executor's thread: the panic is a
/// failure, as a logic's error is: its call fails with it, and the
/// executor refuses every later call, naming it.
#[test]
fn a_panic_on_the_executors_thread_is_a_failure() {
    let mut placed = recording(&Threads::default()).run_on(OwnThread);
    let (_, end) = send(&mut placed, &[0], &[Op::Panic]).expect("an open executor queues a call");
    let reason = "the executor panicked during the call".to_owned();
    assert_eq!(end, Err(reason.clone()));
    assert_eq!(
        send(&mut placed, &[0], &[Op::Read(0)]).err(),
        Some(Rejected::EarlierFailure(reason))
    );
}

/// A logic with a bug, on a pool's thread: the panic stays in the job,
/// so that the pool's thread goes on serving its other executors.
#[test]
fn a_panic_on_the_executors_thread_does_not_reach_its_placement() {
    let (placement, jobs) = holding();
    let mut placed = recording(&Threads::default()).run_on(placement);
    let shot = ShotSet::one(0);
    let reply = placed
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Panic],
        })
        .expect("an open executor queues a call");
    (jobs.try_recv().expect("the call hands a job"))();
    let (_, end) = ended(reply);
    end.expect_err("the logic panicked");
}

/// A caller that keeps several calls in flight on a pool's thread, one of
/// which panics: the placement stops, so the call queued behind it fails
/// as never run, and `close` does not wait for it.
#[test]
fn a_panic_stops_the_calls_queued_behind_it() {
    let threads = Threads::default();
    let (placement, jobs) = holding();
    let mut placed = recording(&threads).run_on(placement);
    let shot = ShotSet::one(0);
    let mut call = |instructions: &[Op]| {
        let reply = placed.call(InstructionBatchRef {
            shots: &shot,
            instructions,
        });
        reply.expect("nothing failed yet when it is queued")
    };
    let panics = call(&[Op::Panic]);
    let behind = call(&[Op::Read(0)]);
    let job = jobs.try_recv().expect("the first call hands a job");
    thread::spawn(job)
        .join()
        .expect("the panic stays in the job");
    ended(panics).1.expect_err("the logic panicked");
    let (out, end) = ended(behind);
    assert_eq!(
        end,
        Err("the call never ran: its placement stopped".to_owned())
    );
    assert!(out.is_empty());
    assert_eq!(seen(&threads).len(), 1, "only the panicking call ran");
    let report = ends_soon(move || placed.close());
    assert_eq!(report, ExecutorReport::default());
}

/// An instruction of a [`fragile_device`]: its logic panics at it when
/// `panics`, and its `bug` says whether copying it, or dropping a copy of
/// it, panics. The caller's own instruction drops quietly, so that only the
/// copy a placed executor makes of the batch, for its job, panics.
#[derive(Debug)]
struct Fragile {
    panics: bool,
    bug: Bug,
    copy: bool,
}

/// Where a [`Fragile`]'s own code has a bug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bug {
    None,
    InCopy,
    InCopysDrop,
}

impl Fragile {
    fn panicking() -> Self {
        Self {
            panics: true,
            bug: Bug::None,
            copy: false,
        }
    }

    fn panicking_when_dropped() -> Self {
        Self {
            panics: false,
            bug: Bug::InCopysDrop,
            copy: false,
        }
    }

    fn panicking_when_copied() -> Self {
        Self {
            panics: false,
            bug: Bug::InCopy,
            copy: false,
        }
    }

    fn quiet() -> Self {
        Self {
            panics: false,
            bug: Bug::None,
            copy: false,
        }
    }
}

impl Clone for Fragile {
    fn clone(&self) -> Self {
        assert!(self.bug != Bug::InCopy, "the instruction's copy has a bug");
        Self {
            panics: self.panics,
            bug: self.bug,
            copy: true,
        }
    }
}

impl Drop for Fragile {
    fn drop(&mut self) {
        assert!(
            !(self.copy && self.bug == Bug::InCopysDrop),
            "the instruction's drop has a bug"
        );
    }
}

impl TypedInstruction for Fragile {}

/// A [`fragile_device`]'s executor.
type FragileDevice = ExecutorFn<
    Fragile,
    (),
    fn(ShotContext) -> Result<(), Rejected>,
    fn(&mut (), &CallInput<'_, Fragile>, &mut Outcomes<'_>) -> Result<(), String>,
>;

/// A device of no state whose logic panics at a [`Fragile`] that says
/// so, and runs any other.
fn fragile_device() -> FragileDevice {
    fn run(_: &mut (), input: &CallInput<'_, Fragile>, _: &mut Outcomes<'_>) -> Result<(), String> {
        let panics = input.instructions().iter().any(|op| op.panics);
        assert!(!panics, "the logic has a bug");
        Ok(())
    }
    executor_fn(Capabilities::default(), |_| Ok(()), run)
}

/// The reply of a call of `instructions` on shot 0 of `executor`.
fn queued(
    executor: &mut PlacedExecutor<FragileDevice, impl ExecutorPlacement>,
    instructions: &[Fragile],
) -> IncrementalReply<ShotOutcome> {
    executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions,
        })
        .expect("an open executor queues a call")
}

/// A caller keeps calls in flight on a pool's thread: the first panics,
/// which stops the placement, and the one queued behind it holds an
/// instruction whose drop has a bug, which panics as the placement stops
/// and drops that call. The panic is absorbed in the job, after the panic
/// hook reported it: none reaches the pool's thread, which goes on serving
/// its other executors (rayon's `spawn` aborts the process on a panic that
/// reaches it).
#[test]
fn an_instructions_drop_that_panics_as_the_placement_stops_does_not_reach_its_thread() {
    let (placement, jobs) = holding();
    let mut placed = fragile_device().run_on(placement);
    let panics = queued(&mut placed, &[Fragile::panicking()]);
    let _behind = queued(&mut placed, &[Fragile::panicking_when_dropped()]);
    let job = jobs.try_recv().expect("the first call hands a job");
    thread::spawn(job)
        .join()
        .expect("no panic reaches the placement's thread");
    ended(panics).1.expect_err("the logic panicked");
}

/// A caller's call runs on a pool's thread, and ends; then the copy of its
/// batch, which its job held, is dropped there, and an instruction's drop
/// has a bug, which panics. The panic is absorbed: the placement goes on
/// serving, and the caller's next call runs.
#[test]
fn an_instructions_drop_that_panics_after_its_call_ran_leaves_the_placement_serving() {
    let (placement, jobs) = holding();
    let mut placed = fragile_device().run_on(placement);
    let first = queued(&mut placed, &[Fragile::panicking_when_dropped()]);
    let job = jobs.try_recv().expect("the first call hands a job");
    thread::spawn(job)
        .join()
        .expect("no panic reaches the placement's thread");
    ended(first).1.expect("the first call ran");
    let next = queued(&mut placed, &[Fragile::quiet()]);
    let job = jobs.try_recv().expect("the next call hands a job");
    job();
    assert_eq!(ended(next).1, Ok(()));
}

/// A caller doing importance sampling calls three shots, each on a thread of
/// its own, whose logic weighs each shot; an instruction's drop has a
/// bug, which panics as each shot's job drops its copy of the batch, after
/// the first call ran. The panics are absorbed, and the report keeps that
/// call, the next one, and each shot's log probability: the weights are
/// not lost.
#[test]
fn an_instructions_drop_that_panics_after_its_call_ran_leaves_it_in_the_report() {
    let device = fragile_device().log_probability((|(): &()| -1.0) as fn(&()) -> f64);
    let mut per_shot = device.run_on(OwnThread).shots(OnePerShot);
    let shots = ShotSet::new(vec![0, 1, 2]).expect("the shots are distinct");
    for instruction in [Fragile::panicking_when_dropped(), Fragile::quiet()] {
        let reply = per_shot
            .call(InstructionBatchRef {
                shots: &shots,
                instructions: &[instruction],
            })
            .expect("an open executor queues a call");
        ended(reply).1.expect("the call ran");
    }
    let mut log_p = LogP::default();
    for shot in 0..3 {
        log_p.add(shot, -1.0);
    }
    assert_eq!(
        ends_soon(move || per_shot.close()),
        ExecutorReport {
            calls: 2,
            log_p,
            states: (),
        }
    );
}

/// A logic with a bug panics on a pool's thread, at an instruction
/// whose drop has a bug too: the copy of the batch is dropped as the
/// logic's panic unwinds, and its panic is absorbed, where a second
/// panic would abort the process. The call fails as any logic that
/// panics, and the executor refuses the next call, naming it.
#[test]
fn an_instructions_drop_that_panics_as_its_calls_panic_unwinds_fails_it_as_that_panic() {
    let (placement, jobs) = holding();
    let mut placed = fragile_device().run_on(placement);
    let both = Fragile {
        panics: true,
        bug: Bug::InCopysDrop,
        copy: false,
    };
    let reply = queued(&mut placed, &[both]);
    let job = jobs.try_recv().expect("the call hands a job");
    thread::spawn(job)
        .join()
        .expect("no panic reaches the placement's thread");
    let reason = "the executor panicked during the call".to_owned();
    assert_eq!(ended(reply).1, Err(reason.clone()));
    assert_eq!(
        placed
            .call(InstructionBatchRef {
                shots: &ShotSet::one(0),
                instructions: &[Fragile::quiet()],
            })
            .err(),
        Some(Rejected::EarlierFailure(reason))
    );
}

/// A caller's batch holds two instructions of a type whose drop has a bug,
/// as one usually does for every value of its type. The call runs on a
/// pool's thread, and ends; then its job drops the copy of the batch, and
/// both drops panic. Both panics are absorbed, where a second panic during
/// the first one's unwind would abort the process: the placement goes on
/// serving, and the caller's next call runs.
#[test]
fn two_instructions_whose_drops_panic_in_one_batch_are_both_absorbed() {
    let (placement, jobs) = holding();
    let mut placed = fragile_device().run_on(placement);
    let first = queued(
        &mut placed,
        &[
            Fragile::panicking_when_dropped(),
            Fragile::panicking_when_dropped(),
        ],
    );
    let job = jobs.try_recv().expect("the first call hands a job");
    thread::spawn(job)
        .join()
        .expect("no panic reaches the placement's thread");
    ended(first).1.expect("the first call ran");
    let next = queued(&mut placed, &[Fragile::quiet()]);
    let job = jobs.try_recv().expect("the next call hands a job");
    job();
    assert_eq!(ended(next).1, Ok(()));
}

/// A logic with a bug panics on a pool's thread, in a batch of two
/// instructions whose drops have a bug too: the copy of the batch is
/// dropped as the logic's panic unwinds, and both panics are absorbed,
/// where either would abort the process. The call fails as any logic
/// that panics.
#[test]
fn two_instructions_whose_drops_panic_as_their_calls_panic_unwinds_are_both_absorbed() {
    let (placement, jobs) = holding();
    let mut placed = fragile_device().run_on(placement);
    let both = Fragile {
        panics: true,
        bug: Bug::InCopysDrop,
        copy: false,
    };
    let reply = queued(&mut placed, &[both, Fragile::panicking_when_dropped()]);
    let job = jobs.try_recv().expect("the call hands a job");
    thread::spawn(job)
        .join()
        .expect("no panic reaches the placement's thread");
    assert_eq!(
        ended(reply).1,
        Err("the executor panicked during the call".to_owned())
    );
}

/// A caller's batch holds an instruction whose drop has a bug, then one
/// whose copy has a bug: the placed executor's copy of the batch panics
/// part-way, inside `call`, and the copy made before it is dropped as
/// that panic unwinds. Its drop's panic is absorbed, where a second panic
/// would abort the process: the caller of `call` gets the copy's panic.
#[test]
fn a_copy_that_panics_part_way_absorbs_the_drops_of_the_copies_made_before_it() {
    let (placement, _jobs) = holding();
    let mut placed = fragile_device().run_on(placement);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        queued(
            &mut placed,
            &[
                Fragile::panicking_when_dropped(),
                Fragile::panicking_when_copied(),
            ],
        );
    }))
    .expect_err("the copy of the batch panicked");
    assert_eq!(
        panicked.downcast_ref::<&str>(),
        Some(&"the instruction's copy has a bug")
    );
}

/// A pool that was shut down drops each job it is handed, on the caller's
/// thread, inside `call`: the call's copy of the batch is dropped with the
/// job, and an instruction's drop that panics there is absorbed. `call`
/// returns, and the call fails as never run.
#[test]
fn an_instructions_drop_that_panics_as_the_placement_drops_its_job_does_not_reach_the_caller() {
    let mut placed = fragile_device().run_on(|job: PlacementJob| drop(job));
    let reply = queued(&mut placed, &[Fragile::panicking_when_dropped()]);
    assert_eq!(
        ended(reply).1,
        Err("the call never ran: its placement stopped".to_owned())
    );
}

/// A full pool panics as it is handed a job, which it drops as its panic
/// unwinds, with the call's copy of the batch: an instruction's drop that
/// panics there is absorbed, where a second panic would abort the process,
/// and the caller of `call` gets the pool's panic.
#[test]
fn an_instructions_drop_that_panics_as_the_placement_panics_leaves_the_placements_panic() {
    let mut placed = fragile_device().run_on(|job: PlacementJob| {
        let _held = job;
        panic!("the pool is full");
    });
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        queued(&mut placed, &[Fragile::panicking_when_dropped()]);
    }))
    .expect_err("the placement panicked as it was handed the job");
    assert_eq!(panicked.downcast_ref::<&str>(), Some(&"the pool is full"));
}

/// A pool is shut down after it served a call: the job of the wait for
/// the calls, as a shot strategy's close starts with, is dropped unrun,
/// which stops the placement and fails no call. A call made then, whose
/// instruction's drop has a bug, is dropped on the caller's thread, inside
/// `call`: the panic is absorbed, `call` returns, and the call fails as
/// never run.
#[test]
fn an_instructions_drop_that_panics_in_a_call_after_the_placement_stopped_does_not_reach_the_caller()
 {
    let mut served = 0;
    let mut placed = fragile_device().run_on(move |job: PlacementJob| {
        served += 1;
        if served == 1 {
            job();
        }
    });
    ended(queued(&mut placed, &[Fragile::quiet()]))
        .1
        .expect("the first call ran");
    placed.wait_for_calls();
    let reply = queued(&mut placed, &[Fragile::panicking_when_dropped()]);
    assert_eq!(
        ended(reply).1,
        Err("the call never ran: its placement stopped".to_owned())
    );
}

/// The system cannot make a thread for an executor's placement, at the
/// first call, whose instruction's drop has a bug: the call's copy of the
/// batch is dropped as the placement's panic unwinds, and its panic is
/// absorbed, where a second panic would abort the process; the caller of
/// `call` gets the placement's panic.
#[test]
fn an_instructions_drop_that_panics_as_the_placement_panics_as_it_starts_leaves_that_panic() {
    let mut placed = fragile_device().run_on(CannotMakeAThread);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        queued(&mut placed, &[Fragile::panicking_when_dropped()]);
    }))
    .expect_err("the placement panicked as it started");
    assert_eq!(
        panicked.downcast_ref::<&str>(),
        Some(&"the system cannot make a thread")
    );
}

/// A device whose reply has a bug: it panics when polled on the
/// executor's thread, after its call returned.
struct PanicsWhenPolled;

impl Reply for PanicsWhenPolled {
    type Item = ShotOutcome;
    fn poll_events(&mut self, _: &mut Vec<ShotOutcome>, _: &Waker) -> ReplyStatus {
        panic!("the reply has a bug")
    }
}

/// The executor of the device whose reply has a bug.
struct DeviceOfPanickingReplies;

impl Executor<InstructionBatch<Op>> for DeviceOfPanickingReplies {
    type Reply = PanicsWhenPolled;
    fn call(&mut self, _: InstructionBatchRef<'_, Op>) -> Result<PanicsWhenPolled, Rejected> {
        Ok(PanicsWhenPolled)
    }
}

impl Describe for DeviceOfPanickingReplies {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

/// A device whose reply has a bug, polled on the executor's thread once
/// the job ran: the panic failed the executor, so a call made after the
/// job ended is refused, rather than accepted only to fail unrun.
#[test]
fn a_reply_that_panics_on_the_executors_thread_fails_the_executor() {
    let (placement, jobs) = holding();
    let mut placed = PlacedExecutor::new(
        DeviceOfPanickingReplies,
        placement,
        ExecutorLifecycle::default(),
        drop,
    );
    let shot = ShotSet::one(0);
    let mut read = || {
        placed.call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Read(0)],
        })
    };
    let reply = read().expect("an open executor queues a call");
    (jobs.try_recv().expect("the call hands a job"))();
    ended(reply).1.expect_err("the reply panicked");
    assert_eq!(
        read().err(),
        Some(Rejected::EarlierFailure(
            "the executor panicked during the call".to_owned()
        ))
    );
}

/// A device whose calls are cancelled below it, as by an executor it
/// places in turn, dropped: each reply ends cancelled.
struct DeviceOfCancelledCalls;

impl Executor<InstructionBatch<Op>> for DeviceOfCancelledCalls {
    type Reply = IncrementalReply<ShotOutcome>;
    fn call(&mut self, _: InstructionBatchRef<'_, Op>) -> Result<Self::Reply, Rejected> {
        let (sender, reply) = reply_channel();
        sender.cancel();
        Ok(reply)
    }
}

impl Describe for DeviceOfCancelledCalls {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

/// A placed device's call is cancelled below it: the placed executor fails
/// and refuses the next call, as after a failure, with a reason that names
/// no drop, since this executor was not dropped. Chosen because a
/// cancelled call counts as a failed one unless a rule says otherwise: the
/// placed executor was not dropped, so the cancellation came from below,
/// and the device has nothing left to run on.
#[test]
fn a_call_cancelled_below_it_fails_the_placed_executor() {
    let (placement, jobs) = holding();
    let mut placed = PlacedExecutor::new(
        DeviceOfCancelledCalls,
        placement,
        ExecutorLifecycle::default(),
        drop,
    );
    let shot = ShotSet::one(0);
    let mut read = || {
        placed.call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Read(0)],
        })
    };
    let _cancelled = read().expect("an open executor queues a call");
    (jobs.try_recv().expect("the call hands a job"))();
    assert_eq!(
        read().err(),
        Some(Rejected::EarlierFailure(
            "the call was cancelled before it ran to its end".to_owned()
        ))
    );
}

/// A placed device's call is cancelled below it: the placed executor's
/// reply ends `ReplyStatus::Cancelled` too, so that the caller tells the
/// call from one that failed.
/// Typed, not a reason of `Failed`: any executor could fail with the same
/// text, and the compiler cannot check a match on a string; tokio
/// (`JoinError::is_cancelled`), Java (`CancellationException`) and Python
/// (`CancelledError`) type it too.
#[test]
fn a_call_cancelled_below_it_ends_cancelled() {
    let (placement, jobs) = holding();
    let mut placed = PlacedExecutor::new(
        DeviceOfCancelledCalls,
        placement,
        ExecutorLifecycle::default(),
        drop,
    );
    let reply = placed
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Read(0)],
        })
        .expect("an open executor queues a call");
    (jobs.try_recv().expect("the call hands a job"))();
    assert_eq!(polled_now(reply), (Vec::new(), ReplyStatus::Cancelled));
}

/// A device whose work is done elsewhere (a link to the device, a
/// device driver's own thread): each call hands its reply's sender to the test,
/// which pushes the outcomes and ends the reply when it likes.
struct DeviceOfRepliesFromElsewhere(mpsc::Sender<ReplySender<ShotOutcome>>);

impl Executor<InstructionBatch<Op>> for DeviceOfRepliesFromElsewhere {
    type Reply = IncrementalReply<ShotOutcome>;
    fn call(&mut self, _: InstructionBatchRef<'_, Op>) -> Result<Self::Reply, Rejected> {
        let (sender, reply) = reply_channel();
        (self.0.send(sender)).map_err(|_| Rejected::Unavailable("the test has ended".into()))?;
        Ok(reply)
    }
}

impl Describe for DeviceOfRepliesFromElsewhere {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

/// A placed device whose outcomes come one by one, its call run on a thread
/// its placement spawns: each outcome reaches the caller as the device
/// hands it over, with a wake, before the device's reply ends, not all at
/// its end.
#[test]
fn each_event_reaches_the_caller_as_the_executor_it_places_hands_it_over() {
    let (senders, devices) = mpsc::channel();
    let mut placed = PlacedExecutor::new(
        DeviceOfRepliesFromElsewhere(senders),
        |job: PlacementJob| {
            thread::spawn(job);
        },
        ExecutorLifecycle::default(),
        drop,
    );
    let mut reply = placed
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Read(0), Op::Read(1)],
        })
        .expect("an open executor queues a call");
    let device = devices
        .recv_timeout(PATIENT)
        .expect("the call runs on its placement");
    let (signals, waker) = signalling();
    let mut out = Vec::new();
    assert_eq!(reply.poll_events(&mut out, &waker), ReplyStatus::Pending);
    device.push(outcome(0, 0));
    signals
        .recv_timeout(PATIENT)
        .expect("the event is handed over as it comes, which wakes the caller");
    assert_eq!(
        reply.poll_events(&mut out, &waker),
        ReplyStatus::Pending,
        "the device's reply has not ended"
    );
    assert_eq!(values(&out), [(0, 0, Value::One)]);
    device.push(outcome(0, 1));
    device.done();
    let (rest, end) = ended(reply);
    end.expect("the device's reply ends done");
    assert_eq!(values(&rest), [(0, 1, Value::One)]);
}

/// Known issue, pinned as it is (open): a reply that panics when polled on
/// the executor's thread fails as dropped, not with the panic's reason,
/// since its sender is dropped as the panic unwinds; and it stops the
/// placement, so the call queued behind it fails as never run. No executor
/// of the crate replies so: `PlacedExecutor::new` is crate-internal.
#[test]
fn a_reply_that_panics_fails_as_dropped_and_stops_the_placement() {
    let (placement, jobs) = holding();
    let mut placed = PlacedExecutor::new(
        DeviceOfPanickingReplies,
        placement,
        ExecutorLifecycle::default(),
        drop,
    );
    let shot = ShotSet::one(0);
    let mut read = || {
        let reply = placed.call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Read(0)],
        });
        reply.expect("nothing failed yet when it is queued")
    };
    let panics = read();
    let behind = read();
    (jobs.try_recv().expect("the first call hands a job"))();
    assert_eq!(
        (ended(panics).1, ended(behind).1),
        (
            Err("the executor dropped the reply before its end".to_owned()),
            Err("the call never ran: its placement stopped".to_owned())
        )
    );
}

/// An instruction whose copy has a bug: cloning it panics, as a placed
/// executor does when it copies a batch into its call's job.
#[derive(Debug)]
pub(in crate::tests) struct CopyPanics;

impl Clone for CopyPanics {
    fn clone(&self) -> Self {
        panic!("the instruction's copy has a bug")
    }
}

/// A batch whose copy panics inside the placed executor's `call`, before
/// the call is queued: the panic reaches the caller, and, as a `Mutex` is
/// poisoned, the executor refuses every later call, naming the panic, as
/// after a panic on its thread.
#[test]
fn after_a_panic_inside_call_it_refuses_every_later_call() {
    let mut placed = executor_fn(
        Capabilities::default(),
        |_| Ok(()),
        |(): &mut (), _: &CallInput<'_, CopyPanics>, _: &mut Outcomes<'_>| Ok(()),
    )
    .run_on(OwnThread);
    let shot = ShotSet::one(0);
    let mut call = |instructions: &[CopyPanics]| {
        placed
            .call(InstructionBatchRef {
                shots: &shot,
                instructions,
            })
            .map(drop)
    };
    panic::catch_unwind(AssertUnwindSafe(|| call(&[CopyPanics])))
        .expect_err("the copy's panic reaches the caller");
    assert_eq!(
        call(&[]),
        Err(Rejected::EarlierFailure(
            "the executor panicked during the call".to_owned()
        ))
    );
}

/// A wrapper never forgets a refusal: an executor whose call failed, then
/// placed, refuses the next call at `call`, with the failure, rather than
/// accept it and fail its reply on its thread. Chosen because a wrapper
/// continues what it wraps, failures included, as iterator adapters and
/// Tower's `Buffer` do. A device stops in a call on shot 0, then is placed
/// on a thread of its own.
#[test]
fn placed_after_a_failure_it_refuses_at_call() {
    let mut one_state = recording(&Threads::default());
    let (_, end) = send(&mut one_state, &[0], &[Op::Stop]).expect("an open executor runs a call");
    end.expect_err("the device stops");
    let mut placed = one_state.run_on(OwnThread);
    assert_eq!(
        send(&mut placed, &[0], &[Op::Flip]).err(),
        Some(Rejected::EarlierFailure("the device stopped".to_owned()))
    );
}

/// A wrapper never forgets a refusal: an executor closed, then placed,
/// refuses the next call at `call` as closed, rather than accept it and
/// fail its reply on its thread. Chosen because a wrapper continues what it
/// wraps, failures included, as iterator adapters and Tower's `Buffer` do.
#[test]
fn placed_after_close_it_refuses_at_call() {
    let mut one_state = recording(&Threads::default());
    one_state.close();
    let mut placed = one_state.run_on(OwnThread);
    assert_eq!(
        send(&mut placed, &[0], &[Op::Flip]).err(),
        Some(Rejected::Closed)
    );
}

/// A placement whose pool is gone, which panics as it is handed a job: the
/// panic reaches the caller, and the placement stopped, so every later
/// call is refused as never run, not as a panic of the executor's.
#[test]
fn after_its_placement_panics_it_refuses_every_later_call_as_never_run() {
    let mut placed =
        recording(&Threads::default()).run_on(|_: PlacementJob| panic!("the pool is gone"));
    let shot = ShotSet::one(0);
    let mut read = || {
        placed.call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Read(0)],
        })
    };
    panic::catch_unwind(AssertUnwindSafe(&mut read))
        .err()
        .expect("the placement's panic reaches the caller");
    assert_eq!(
        read().err(),
        Some(Rejected::EarlierFailure(
            "the call never ran: its placement stopped".to_owned()
        ))
    );
}

/// A placement that cannot start, as `OwnThread` when the system cannot
/// make a thread.
struct CannotMakeAThread;

impl ExecutorPlacement for CannotMakeAThread {
    fn start(self) -> Box<dyn FnMut(PlacementJob) + Send> {
        panic!("the system cannot make a thread")
    }
}

/// A device's executor placed on a thread of its own, which the system
/// cannot make at the first call: the panic reaches the caller, and the
/// placement never started, so every later call is refused as never run,
/// not as a panic of the executor's.
#[test]
fn after_its_placement_panics_as_it_starts_it_refuses_every_later_call_as_never_run() {
    let mut placed = recording(&Threads::default()).run_on(CannotMakeAThread);
    let shot = ShotSet::one(0);
    let mut read = || {
        placed.call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Read(0)],
        })
    };
    panic::catch_unwind(AssertUnwindSafe(&mut read))
        .err()
        .expect("the placement's panic reaches the caller");
    assert_eq!(
        read().err(),
        Some(Rejected::EarlierFailure(
            "the call never ran: its placement stopped".to_owned()
        ))
    );
}

/// An executor that ran a call on the caller's thread, then was placed on
/// a thread the system cannot make: its placement never took it, so
/// `close` closes it where it is, and the report is whole, its call and
/// its state's log probability included.
#[test]
fn closed_after_its_placement_panics_as_it_starts_it_gives_the_whole_report() {
    let mut one_state = recording(&Threads::default());
    let (_, end) = send(&mut one_state, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let mut placed = one_state.run_on(CannotMakeAThread);
    panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut placed, &[0], &[Op::Read(0)]).map(drop)
    }))
    .expect_err("the placement's panic reaches the caller");
    let mut log_p = LogP::default();
    log_p.add(0, 1.0);
    assert_eq!(
        ends_soon(move || placed.close()),
        ExecutorReport {
            calls: 1,
            log_p,
            states: (),
        }
    );
}

/// The other side of the rule above: an executor that ran a call on the
/// caller's thread, then was placed on a pool that is gone, which panics
/// as it is handed the first job: the placement started and stopped, so
/// `close` reports nothing.
#[test]
fn closed_after_its_placement_panics_as_it_is_handed_a_job_it_reports_nothing() {
    let mut one_state = recording(&Threads::default());
    let (_, end) = send(&mut one_state, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let mut placed = one_state.run_on(|_: PlacementJob| panic!("the pool is gone"));
    panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut placed, &[0], &[Op::Read(0)]).map(drop)
    }))
    .expect_err("the placement's panic reaches the caller");
    assert_eq!(ends_soon(move || placed.close()), ExecutorReport::default());
}

/// A placement that drops a job unrun (a pool shut down): the calls it
/// held fail, and `close` does not wait for them.
#[test]
fn a_job_its_placement_drops_fails_the_calls_it_held() {
    let (placement, jobs) = holding();
    let mut placed = recording(&Threads::default()).run_on(placement);
    let shot = ShotSet::one(0);
    let reply = placed
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Read(0)],
        })
        .expect("an open executor queues a call");
    drop(jobs.try_recv().expect("the call hands a job"));
    let (_, end) = ended(reply);
    assert_eq!(
        end,
        Err("the call never ran: its placement stopped".to_owned())
    );
    let report = ends_soon(move || placed.close());
    assert_eq!(report, ExecutorReport::default());
}

/// A pool shut down while it held a job: the placement stopped, so every
/// later call is refused, naming why the calls it held failed.
#[test]
fn after_its_placement_drops_a_job_it_refuses_every_later_call() {
    let (placement, jobs) = holding();
    let mut placed = recording(&Threads::default()).run_on(placement);
    let shot = ShotSet::one(0);
    let mut read = || {
        placed.call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Read(0)],
        })
    };
    let reply = read().expect("an open executor queues a call");
    drop(jobs.try_recv().expect("the call hands a job"));
    ended(reply).1.expect_err("its job was dropped unrun");
    assert_eq!(
        read().err(),
        Some(Rejected::EarlierFailure(
            "the call never ran: its placement stopped".to_owned()
        ))
    );
}

/// `executor`, a device's, dropped (a caller that gave up) while its first
/// call on shot 0 runs, held at the gate, and a second call is queued
/// behind it: the replies of the call running and of the call queued.
pub(in crate::tests) fn dropped_while_a_call_runs<E: Executor<InstructionBatch<Op>>>(
    mut executor: E,
    gate: &Gate,
) -> (E::Reply, E::Reply) {
    let shot = ShotSet::one(0);
    let running = executor
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Flip, Op::Read(0)],
        })
        .expect("an open executor queues a call");
    gate.started
        .recv_timeout(PATIENT)
        .expect("the first call starts");
    let queued = executor
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Flip, Op::Read(1)],
        })
        .expect("an open executor queues a call");
    drop(executor);
    (running, queued)
}

/// A caller gives up on a device on a thread of its own while a call runs,
/// another queued behind it. Dropping a placed executor cancels the calls
/// not yet started: the call queued never runs, and its reply ends
/// `ReplyStatus::Cancelled` as the executor is dropped.
///
/// Chosen so that a dropped executor frees its states after the call in
/// progress at most, as dropping tokio's `JoinSet`, Java's `shutdownNow`
/// and Python's `shutdown(cancel_futures=True)` cancel the work not yet
/// started. Typed, not a reason of `Failed`: any executor could fail with
/// the same text, and the compiler cannot check a match on a string; tokio
/// (`JoinError::is_cancelled`), Java (`CancellationException`) and Python
/// (`CancelledError`) type it too.
#[test]
fn dropped_it_cancels_the_calls_not_yet_started() {
    let (gate, device) = gated_device();
    let (_, queued) = dropped_while_a_call_runs(device.run_on(OwnThread), &gate);
    assert_eq!(polled_now(queued), (Vec::new(), ReplyStatus::Cancelled));
    gate.open
        .send(())
        .expect("the call running waits at the gate");
    (gate.freed.recv_timeout(PATIENT)).expect("the state is freed, after which nothing runs");
    assert!(
        gate.started.try_recv().is_err(),
        "the call queued never started"
    );
}

/// Dropping a placed executor does not stop the call running: it finishes,
/// and its reply holds its outcomes, since a call cannot be stopped
/// mid-way; only the calls not yet started are cancelled, as with tokio's
/// `JoinSet`, Java's `shutdownNow` and Python's `cancel_futures`, so that a
/// dropped executor frees its states after the call in progress at most.
/// A caller gives up on a device on a thread of its own while a call runs.
#[test]
fn dropped_it_lets_the_call_running_finish() {
    let (gate, device) = gated_device();
    let (running, _) = dropped_while_a_call_runs(device.run_on(OwnThread), &gate);
    gate.open
        .send(())
        .expect("the call running waits at the gate");
    let (out, end) = ended(running);
    end.expect("the call running finishes");
    assert_eq!(values(&out), [(0, 0, Value::One)]);
}

/// A caller's waker that panics when woken.
pub(in crate::tests) struct PanicsWhenWoken;

impl Wake for PanicsWhenWoken {
    fn wake(self: Arc<Self>) {
        panic!("the caller's waker panics");
    }
}

/// The events and the end of `reply`, a call held at its device's gate:
/// polled with a waker that panics, then the gate opened (`open`), then
/// polled with it until it ends, so that the executor finds that waker
/// whenever it wakes the caller (a caller waiting in `wait` would replace
/// it with its own).
fn polled_with_a_panicking_waker(
    mut reply: impl Reply<Item = ShotOutcome>,
    open: impl FnOnce(),
) -> (Vec<ShotOutcome>, ReplyStatus) {
    let panicking = Waker::from(Arc::new(PanicsWhenWoken));
    let mut outcomes = Vec::new();
    assert_eq!(
        reply.poll_events(&mut outcomes, &panicking),
        ReplyStatus::Pending,
        "held at the gate"
    );
    open();
    let deadline = Instant::now() + PATIENT;
    loop {
        let status = reply.poll_events(&mut outcomes, &panicking);
        if status != ReplyStatus::Pending {
            return (outcomes, status);
        }
        assert!(Instant::now() < deadline, "the reply never ended");
        thread::sleep(Duration::from_millis(1));
    }
}

/// A placed executor of a device, `device`, on its own thread, with a call
/// running, held at the gate, and one queued behind it whose caller polled
/// it with a waker that panics: the executor, the call running's reply and
/// the queued one's.
fn with_a_panicking_waker_queued(
    device: GatedDevice,
    gate: &Gate,
) -> (
    PlacedExecutor<GatedDevice, OwnThread>,
    IncrementalReply<ShotOutcome>,
    IncrementalReply<ShotOutcome>,
) {
    let mut executor = device.run_on(OwnThread);
    let shot = ShotSet::one(0);
    let mut call = |tag| {
        executor
            .call(InstructionBatchRef {
                shots: &shot,
                instructions: &[Op::Flip, Op::Read(tag)],
            })
            .expect("an open executor queues a call")
    };
    let running = call(0);
    gate.started
        .recv_timeout(PATIENT)
        .expect("the first call starts");
    let mut awaited = call(1);
    let panicking = Waker::from(Arc::new(PanicsWhenWoken));
    assert_eq!(
        awaited.poll_events(&mut Vec::new(), &panicking),
        ReplyStatus::Pending,
        "queued behind the call running"
    );
    (executor, running, awaited)
}

/// A caller's waker that panics as `drop` cancels its call does not keep
/// that call, or the calls queued after it, from being cancelled: their
/// replies end `ReplyStatus::Cancelled`, not failed as never run (`its
/// placement stopped`, a wrong reason: the placement runs on), and the
/// executor it places is still dropped on its placement. Chosen so that
/// one caller's bug does not mislead the others. A caller gives up on a
/// device while a call runs and two are queued, one awaited by a task
/// whose waker has a bug.
#[test]
fn a_waker_that_panics_as_drop_cancels_its_call_leaves_the_others_cancelled() {
    let (gate, device) = gated_device();
    let (mut executor, running, awaited) = with_a_panicking_waker_queued(device, &gate);
    let shot = ShotSet::one(0);
    let after = executor
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Flip, Op::Read(2)],
        })
        .expect("an open executor queues a call");
    drop(executor);
    assert_eq!(polled_now(awaited), (Vec::new(), ReplyStatus::Cancelled));
    assert_eq!(polled_now(after), (Vec::new(), ReplyStatus::Cancelled));
    gate.open
        .send(())
        .expect("the call running waits at the gate");
    ended(running).1.expect("the call running finishes");
    (gate.freed.recv_timeout(PATIENT)).expect("the executor it places is dropped on its placement");
}

/// `drop` raises no panic of a caller's waker woken as it cancels the
/// caller's call: the panic is the caller's bug, and a destructor must not
/// fail (Rust API Guidelines, C-DTOR-FAIL), since a panic out of a `drop`
/// run during an unwind aborts the process. A caller gives up on a device
/// while a call runs and another is queued, awaited by a task whose waker
/// has a bug.
#[test]
fn dropped_it_raises_no_panic_of_a_callers_waker() {
    let (gate, device) = gated_device();
    let (executor, running, _awaited) = with_a_panicking_waker_queued(device, &gate);
    panic::catch_unwind(AssertUnwindSafe(|| drop(executor)))
        .expect("`drop` raises no caller's panic");
    gate.open
        .send(())
        .expect("the call running waits at the gate");
    ended(running).1.expect("the call running finishes");
}

/// Dropped as a panic unwinds (a caller's bug, the executor one of its
/// locals), with a call queued whose caller's waker panics, a placed
/// executor still cancels that call, and the process goes on: the
/// waker's panic, absorbed, does not abort it, as a second panic out of a
/// `drop` during an unwind would (C-DTOR-FAIL).
#[test]
fn dropped_as_a_panic_unwinds_it_cancels_a_call_whose_callers_waker_panics() {
    let (gate, device) = gated_device();
    let (executor, running, awaited) = with_a_panicking_waker_queued(device, &gate);
    let unwound = panic::catch_unwind(AssertUnwindSafe(move || {
        let _executor = executor;
        panic!("the caller has a bug");
    }));
    let payload = unwound.expect_err("the caller's panic unwinds");
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"the caller has a bug"),
        "the caller's own panic, not the waker's"
    );
    assert_eq!(polled_now(awaited), (Vec::new(), ReplyStatus::Cancelled));
    gate.open
        .send(())
        .expect("the call running waits at the gate");
    ended(running).1.expect("the call running finishes");
}

/// An instruction whose drop has a bug, in a call `drop` cancels, does not
/// keep the calls queued after it from being cancelled: their replies end
/// `ReplyStatus::Cancelled`, not failed as never run (`its placement
/// stopped`, a wrong reason: the placement runs on), as with a caller's
/// waker that panics. A caller gives up on a device on a pool's thread
/// with two calls queued, the first holding that instruction.
#[test]
fn an_instructions_drop_that_panics_as_drop_cancels_its_call_leaves_the_others_cancelled() {
    let (placement, _jobs) = holding();
    let mut placed = fragile_device().run_on(placement);
    let fragile = queued(&mut placed, &[Fragile::panicking_when_dropped()]);
    let after = queued(&mut placed, &[Fragile::quiet()]);
    // Whether `drop` raises the panic is another test's rule.
    let _ = panic::catch_unwind(AssertUnwindSafe(|| drop(placed)));
    assert_eq!(polled_now(fragile), (Vec::new(), ReplyStatus::Cancelled));
    assert_eq!(polled_now(after), (Vec::new(), ReplyStatus::Cancelled));
}

/// `drop` raises no panic of an instruction's drop as it cancels the call
/// that holds it: the instruction is the caller's code, and a destructor
/// must not fail (C-DTOR-FAIL). A caller gives up on a device with a call
/// queued that holds an instruction whose drop has a bug.
#[test]
fn dropped_it_raises_no_panic_of_an_instructions_drop() {
    let (placement, _jobs) = holding();
    let mut placed = fragile_device().run_on(placement);
    let _fragile = queued(&mut placed, &[Fragile::panicking_when_dropped()]);
    panic::catch_unwind(AssertUnwindSafe(|| drop(placed)))
        .expect("`drop` raises no instruction's panic");
}

/// Dropped as a panic unwinds (a caller's bug, the executor one of its
/// locals), with a call queued that holds an instruction whose drop has a
/// bug, a placed executor still cancels that call, and the process goes
/// on: the instruction's panic, absorbed, does not abort it, as a second
/// panic out of a `drop` during an unwind would (C-DTOR-FAIL).
#[test]
fn dropped_as_a_panic_unwinds_it_cancels_a_call_whose_instruction_panics_when_dropped() {
    let (placement, _jobs) = holding();
    let mut placed = fragile_device().run_on(placement);
    let fragile = queued(&mut placed, &[Fragile::panicking_when_dropped()]);
    let unwound = panic::catch_unwind(AssertUnwindSafe(move || {
        let _placed = placed;
        panic!("the caller has a bug");
    }));
    let payload = unwound.expect_err("the caller's panic unwinds");
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"the caller has a bug"),
        "the caller's own panic, not the instruction's"
    );
    assert_eq!(polled_now(fragile), (Vec::new(), ReplyStatus::Cancelled));
}

/// A caller's waker that panics when woken on the executor's thread, as
/// the call hands over its events, is the caller's bug, not the
/// executor's: the call ends as it would have, with its outcomes, and the
/// executor stays open, taking later calls. A task awaits a device's call
/// with a waker that has a bug.
#[test]
fn a_callers_waker_that_panics_on_the_executors_thread_does_not_fail_it() {
    let (gate, device) = gated_device();
    let mut executor = device.run_on(OwnThread);
    let reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Flip, Op::Read(0)],
        })
        .expect("an open executor queues a call");
    gate.started.recv_timeout(PATIENT).expect("the call starts");
    let (out, end) = polled_with_a_panicking_waker(reply, || {
        gate.open.send(()).expect("the call waits at the gate");
    });
    assert_eq!(
        (values(&out), end),
        (vec![(0, 0, Value::One)], ReplyStatus::Done),
        "the call ends as it would have"
    );
    gate.open
        .send(())
        .expect("the device outlives the test's calls");
    let (_, later) = send(&mut executor, &[0], &[Op::Flip]).expect("a later call is accepted");
    later.expect("the later call runs");
}

/// A logic that panics on the executor's thread fails its call (`the
/// executor panicked during the call`) though its caller's waker panics as
/// the failure wakes it, during the logic's unwind: that panic is
/// absorbed, so the process does not abort, and the reply ends with the
/// logic's failure. A task awaits a buggy logic's call with a waker
/// that has a bug too.
#[test]
fn a_logic_that_panics_fails_its_call_though_its_callers_waker_panics() {
    let (gate, device) = gated_device();
    let mut executor = device.run_on(OwnThread);
    let reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Panic],
        })
        .expect("an open executor queues a call");
    gate.started.recv_timeout(PATIENT).expect("the call starts");
    let open_the_gate = || gate.open.send(()).expect("the call waits at the gate");
    assert_eq!(
        polled_with_a_panicking_waker(reply, open_the_gate),
        (
            Vec::new(),
            ReplyStatus::Failed("the executor panicked during the call".to_owned())
        )
    );
}

/// Runs its closure when dropped: a caller's cleanup guard.
struct OnDrop<F: FnMut()>(F);

impl<F: FnMut()> Drop for OnDrop<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

/// A call that returns normally does not fail the executor though its
/// thread is unwinding: a placement that runs each job inline, called by
/// a caller's cleanup guard as the caller's panic unwinds, runs a call
/// that succeeds, and a later call is accepted, as when not placed. Its
/// thread's unwind is not the call's panic.
#[test]
fn a_call_that_returns_as_its_thread_unwinds_does_not_fail_the_executor() {
    let threads = Threads::default();
    let mut placed = recording(&threads).run_on(|job: PlacementJob| job());
    let mut cleanup = None;
    let unwound = panic::catch_unwind(AssertUnwindSafe(|| {
        let _guard = OnDrop(|| cleanup = Some(send(&mut placed, &[0], &[Op::Flip])));
        panic!("the caller has a bug");
    }));
    unwound.expect_err("the caller's panic unwinds");
    let (_, end) = (cleanup.expect("the guard ran")).expect("the cleanup call is accepted");
    end.expect("the cleanup call succeeds");
    let (_, later) = send(&mut placed, &[0], &[Op::Flip]).expect("a later call is accepted");
    later.expect("the later call runs");
}

/// A dropped placed executor frees its states after the call running,
/// without `close`, since it cancels the calls queued behind it, as
/// dropping tokio's `JoinSet`, Java's `shutdownNow` and Python's
/// `cancel_futures` cancel the work not yet started: a device that a state
/// holds is free soon after a caller gives up, not after every call it
/// queued. Not before, though: the call running uses the state. A caller
/// gives up on a device on a thread of its own while a call runs, another
/// queued behind it.
#[test]
fn dropped_it_frees_its_states_after_the_call_running_without_close() {
    let (gate, device) = gated_device();
    let _replies = dropped_while_a_call_runs(device.run_on(OwnThread), &gate);
    assert!(
        gate.freed.try_recv().is_err(),
        "held while the call running uses it"
    );
    gate.open
        .send(())
        .expect("the call running waits at the gate");
    (gate.freed.recv_timeout(PATIENT)).expect("freed after the call running, without close");
}

/// A simulator of a whole run placed whole, `.shots(..).run_on(..)`,
/// dropped while a call runs: the call queued behind it is cancelled as
/// when not given a state per shot, its reply ended
/// `ReplyStatus::Cancelled`, so that the states are freed after the call in
/// progress at most, as dropping tokio's `JoinSet`, Java's `shutdownNow`
/// and Python's `cancel_futures` cancel the work not yet started.
/// Typed, not a reason of `Failed`: any executor could fail with the same
/// text, and the compiler cannot check a match on a string; tokio
/// (`JoinError::is_cancelled`), Java (`CancellationException`) and Python
/// (`CancelledError`) type it too.
#[test]
fn placed_after_one_per_shot_dropped_it_cancels_the_calls_not_yet_started() {
    let (gate, device) = gated_device();
    let placed = device.shots(OnePerShot).run_on(OwnThread);
    let (_, queued) = dropped_while_a_call_runs(placed, &gate);
    assert_eq!(polled_now(queued), (Vec::new(), ReplyStatus::Cancelled));
    gate.open
        .send(())
        .expect("the call running waits at the gate");
    (gate.freed.recv_timeout(PATIENT)).expect("the state is freed, after which nothing runs");
    assert!(
        gate.started.try_recv().is_err(),
        "the call queued never started"
    );
}

/// A device's state that must be dropped where its calls ran (a device's
/// driver): dropped, it sends the thread it is dropped on.
struct BoundToItsThread {
    bit: Bit,
    dropped_on: mpsc::Sender<ThreadId>,
}

impl Drop for BoundToItsThread {
    fn drop(&mut self) {
        let _ = self.dropped_on.send(thread::current().id());
    }
}

/// A device whose states are [`BoundToItsThread`], each sending to
/// `dropped_on`.
fn bound_to_its_thread(
    dropped_on: mpsc::Sender<ThreadId>,
) -> ExecutorFn<
    Op,
    BoundToItsThread,
    impl FnMut(ShotContext) -> Result<BoundToItsThread, Rejected>,
    RunBound,
> {
    executor_fn(
        Capabilities::default(),
        move |_| {
            Ok(BoundToItsThread {
                bit: Bit {
                    one: false,
                    threads: Threads::default(),
                },
                dropped_on: dropped_on.clone(),
            })
        },
        (|state, input, out| run_bit(&mut state.bit, input, out)) as RunBound,
    )
}

/// The logic of a [`bound_to_its_thread`] device: [`run_bit`]'s.
type RunBound =
    fn(&mut BoundToItsThread, &CallInput<'_, Op>, &mut Outcomes<'_>) -> Result<(), String>;

/// A device that must run where its calls ran (a device's driver), on a
/// pool's threads, dropped without closing: its state is dropped by a last job
/// on the pool, where its calls ran, not on the thread that drops it.
#[test]
fn dropped_it_drops_the_executor_it_places_on_its_placement() {
    let (dropped_on, dropped) = mpsc::channel();
    let (placement, jobs) = holding();
    let mut placed = bound_to_its_thread(dropped_on).run_on(placement);
    let shot = ShotSet::one(0);
    let reply = placed
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Flip],
        })
        .expect("an open executor queues a call");
    let job = jobs.try_recv().expect("the call hands a job");
    thread::spawn(job).join().expect("the job does not panic");
    ended(reply).1.expect("the logic does not fail");
    drop(placed);
    let pool = thread::spawn(move || {
        for job in jobs.try_iter() {
            job();
        }
        thread::current().id()
    })
    .join()
    .expect("the jobs do not panic");
    assert_eq!(
        dropped.recv_timeout(PATIENT),
        Ok(pool),
        "dropped on the pool's thread, not the caller's"
    );
}

/// A device that must run where its calls ran (a device's driver), on a
/// placement that runs each job on a worker and waits for it (as a
/// synchronous dispatch to a device's queue does), dropped without closing:
/// its state is dropped by the last job, on the worker, though that job
/// ends before the drop returns. Chosen so that the executor it places is
/// dropped where its calls ran whatever the placement's timing, not by
/// whichever thread lets it go last.
#[test]
fn dropped_it_drops_the_executor_it_places_in_its_last_job_however_soon_it_ends() {
    let (dropped_on, dropped) = mpsc::channel();
    let (ran_on, worker) = mpsc::channel();
    let placement = move |job: PlacementJob| {
        let ran_on = ran_on.clone();
        thread::spawn(move || {
            job();
            let _ = ran_on.send(thread::current().id());
        })
        .join()
        .expect("the job does not panic");
    };
    let mut placed = bound_to_its_thread(dropped_on).run_on(placement);
    let (_, end) = send(&mut placed, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    drop(placed);
    let last_job = worker.try_iter().last().expect("the drop ran a job");
    assert_eq!(
        dropped.recv_timeout(PATIENT),
        Ok(last_job),
        "dropped on the last job's worker, not the caller's thread"
    );
}

/// A device that must run where its calls ran (a device's driver), on a
/// thread of its own, closed by another thread: its state's report is
/// made on its own thread, where its calls ran, not on the thread that
/// closes it.
#[test]
fn closed_it_reports_the_executor_it_places_on_its_placement() {
    let mut placed = executor_fn(Capabilities::default(), own_bit, run_bit as RunBit)
        .state_report(reported_on)
        .run_on(OwnThread);
    let (_, end) = send(&mut placed, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let report = ends_soon(move || placed.close());
    let [(ran_on, reported)] = report.states.0[..] else {
        panic!("its one state is reported once: {:?}", report.states);
    };
    assert_eq!(
        reported, ran_on,
        "reported on its own thread, not on the closer's"
    );
}

/// Known issue, pinned as it is: a device that must run where its calls
/// ran (a device's driver), on a pool's threads, whose logic panicked in
/// its second call, which stopped the placement: dropped, its state, kept
/// from the first call, is dropped on the thread that drops it, since no
/// job runs any more.
#[test]
fn once_stopped_it_drops_the_executor_it_places_where_it_is_dropped() {
    let (dropped_on, dropped) = mpsc::channel();
    let (placement, jobs) = holding();
    let mut placed = bound_to_its_thread(dropped_on).run_on(placement);
    let shot = ShotSet::one(0);
    let first = placed
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Flip],
        })
        .expect("an open executor queues a call");
    let job = jobs.try_recv().expect("the call hands a job");
    thread::spawn(job).join().expect("the first call runs");
    ended(first).1.expect("the first call keeps the state");
    let reply = placed
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Panic],
        })
        .expect("an open executor queues a call");
    let job = jobs.try_recv().expect("the call hands a job");
    thread::spawn(job)
        .join()
        .expect("the job keeps the logic's panic");
    ended(reply).1.expect_err("the logic panicked");
    drop(placed);
    assert_eq!(
        dropped.recv_timeout(PATIENT),
        Ok(thread::current().id()),
        "dropped on the caller's thread, not the pool's"
    );
}

/// A device's executor placed whole on a thread of its own,
/// `.shots(OnePerShot).run_on(OwnThread)`, whose logic panicked in
/// a call on shot 0 after shots 0 and 1 had run, which stopped the
/// placement: `close` has freed the states of the executor it places when
/// it returns, so that the device can be built again at once, as when no
/// call panicked, though no job runs any more.
#[test]
fn once_stopped_its_close_frees_the_states_of_the_executor_it_places() {
    let (dropped_on, dropped) = mpsc::channel();
    let mut placed = bound_to_its_thread(dropped_on)
        .shots(OnePerShot)
        .run_on(OwnThread);
    let (_, end) = send(&mut placed, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let (_, end) = send(&mut placed, &[0], &[Op::Panic]).expect("an open executor runs a call");
    end.expect_err("the logic panicked");
    let placed = ends_soon(move || {
        placed.close();
        placed
    });
    assert_eq!(
        dropped.try_iter().count(),
        2,
        "both states are freed when `close` returns, before the drop"
    );
    drop(placed);
}

/// The logic of a device whose driver has a bug: it panics at `Panic`,
/// whatever the state.
fn panics_at_panic<S>(
    _: &mut S,
    input: &CallInput<'_, Op>,
    _: &mut Outcomes<'_>,
) -> Result<(), String> {
    assert!(
        !input.instructions().contains(&Op::Panic),
        "the device's driver panics"
    );
    Ok(())
}

/// A device whose states fail to power down (a device driver's bug),
/// placed whole on a pool's threads, `.shots(OnePerShot).run_on(..)`,
/// once it ran shots 0 and 1, then panicked in a call on shot 0, which
/// stopped the placement: no job runs any more, and none is left running.
fn stopped_holding_states_that_power_down_badly(
    freed: &Freed,
) -> impl Executor<InstructionBatch<Op>> + Close {
    let (placement, jobs) = holding();
    let mut placed = executor_fn(
        Capabilities::default(),
        powering_down_badly(freed, None),
        panics_at_panic,
    )
    .shots(OnePerShot)
    .run_on(placement);
    for (shots, op) in [(vec![0, 1], Op::Flip), (vec![0], Op::Panic)] {
        let reply = placed
            .call(InstructionBatchRef {
                shots: &ShotSet::new(shots).expect("the shots are distinct"),
                instructions: &[op],
            })
            .expect("an open executor queues a call");
        let job = jobs.try_recv().expect("the call hands a job");
        thread::spawn(job)
            .join()
            .expect("the job keeps the logic's panic");
        let (_, end) = ended(reply);
        assert_eq!(end.is_err(), op == Op::Panic, "only the second call fails");
    }
    placed
}

/// Once its placement stopped, `close` frees the states of the executor
/// it places where it is called (both of a stopped device's): the first
/// one's drop panic, shot 0's, reaches the closer, as when not placed.
#[test]
fn once_stopped_its_close_raises_the_first_panic_of_a_states_drop() {
    let freed = Freed::default();
    let mut placed = stopped_holding_states_that_power_down_badly(&freed);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| drop(placed.close())))
        .expect_err("the states' drops panic");
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0, 1]);
}

/// Once its placement stopped, dropping it unclosed frees the states of
/// the executor it places (both of a stopped device's), and raises no
/// panic of their drops: `drop` never panics, so that a placed executor
/// dropped during an unwind, or as part of another, cannot abort the
/// process; only `close` raises such a panic.
#[test]
fn once_stopped_its_drop_frees_the_states_raising_no_panic_of_their_drops() {
    let freed = Freed::default();
    let placed = stopped_holding_states_that_power_down_badly(&freed);
    panic::catch_unwind(AssertUnwindSafe(|| drop(placed)))
        .expect("`drop` raises no panic of a state's drop");
    assert_eq!(freed_so_far(&freed), [0, 1]);
}

/// A device whose state fails to power down ran a call before it was
/// placed, so it holds its state where it is: dropped before its first
/// call, the placed device frees that state there, and raises no panic of
/// its drop, as once its placement started.
#[test]
fn before_its_first_call_its_drop_frees_the_state_raising_no_panic_of_its_drop() {
    let freed = Freed::default();
    let mut device = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        panics_at_panic,
    );
    let reply = device
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Flip],
        })
        .expect("an open executor runs a call");
    drop(reply);
    let placed = device.run_on(OwnThread);
    panic::catch_unwind(AssertUnwindSafe(|| drop(placed)))
        .expect("`drop` raises no panic of a state's drop");
    assert_eq!(freed_so_far(&freed), [0]);
}

/// The same device, placed, freed before its first call by a shot
/// strategy that waits for its states to be freed (`drop_and_wait`, as at
/// close): it frees the state here, and raises its drop's panic, as
/// `close` does, not as `drop`.
#[test]
fn before_its_first_call_its_drop_and_wait_raises_the_panic_of_a_states_drop() {
    let freed = Freed::default();
    let mut device = executor_fn(
        Capabilities::default(),
        powering_down_badly(&freed, None),
        panics_at_panic,
    );
    let reply = device
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Flip],
        })
        .expect("an open executor runs a call");
    drop(reply);
    let placed = device.run_on(OwnThread);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| placed.drop_and_wait()))
        .expect_err("the state's drop panics");
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0]);
}

/// A device whose state fails to power down (a device driver's bug), running
/// `logic`, on a pool that keeps the jobs it is handed for the test to
/// run, once its first call, a `Flip` on shot 0, ran: the state is held on
/// the pool's side.
fn holding_a_state_that_powers_down_badly<P>(
    freed: &Freed,
    logic: P,
) -> (
    impl Executor<InstructionBatch<Op>, Reply: Reply<Item = ShotOutcome>>,
    mpsc::Receiver<PlacementJob>,
)
where
    P: FnMut(&mut PowersDownBadly, &CallInput<'_, Op>, &mut Outcomes<'_>) -> Result<(), String>
        + Send
        + 'static,
{
    let (placement, jobs) = holding();
    let mut placed = executor_fn(
        Capabilities::default(),
        powering_down_badly(freed, None),
        logic,
    )
    .run_on(placement);
    let first = placed
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Flip],
        })
        .expect("an open executor queues a call");
    let job = jobs.try_recv().expect("the call hands a job");
    thread::spawn(job).join().expect("the first call runs");
    ended(first).1.expect("the first call keeps the state");
    (placed, jobs)
}

/// A device whose state fails to power down, on a pool, dropped while its
/// second call's job is handed but not run; the pool then drops that job
/// unrun (it shuts down): the job, the last to hold the executor it
/// places, frees it on the pool's thread, where the state's drop panic is
/// absorbed, after the panic hook reported it, so that it does not reach
/// the pool's thread.
#[test]
fn a_states_drop_that_panics_as_a_job_dropped_unrun_frees_it_stays_off_the_placements_thread() {
    let freed = Freed::default();
    let (mut placed, jobs) = holding_a_state_that_powers_down_badly(&freed, panics_at_panic);
    drop(
        placed
            .call(InstructionBatchRef {
                shots: &ShotSet::one(0),
                instructions: &[Op::Flip],
            })
            .expect("an open executor queues a call"),
    );
    let job = jobs.try_recv().expect("the call hands a job");
    drop(placed);
    assert_eq!(freed_so_far(&freed), [], "the job holds the state");
    thread::spawn(move || drop(job))
        .join()
        .expect("the state's drop panic stays off the pool's thread");
    assert_eq!(freed_so_far(&freed), [0]);
}

/// A device whose state fails to power down, on a pool, whose logic
/// panics in its second call (two bugs); the caller, woken as that call
/// fails, drops the executor while the job is still ending (the waker
/// holds it there until then): the job, ending, is the last to hold the
/// executor it places, and frees it on the pool's thread, where the
/// state's drop panic is absorbed.
#[test]
fn a_states_drop_that_panics_as_a_stopping_job_frees_it_stays_off_the_placements_thread() {
    let freed = Freed::default();
    let (mut placed, jobs) = holding_a_state_that_powers_down_badly(&freed, panics_at_panic);
    let mut failing = placed
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Panic],
        })
        .expect("an open executor queues a call");
    let (pauses, told, resume) = pausing();
    assert_eq!(
        failing.poll_events(&mut Vec::new(), &pauses),
        ReplyStatus::Pending,
        "queued, not run"
    );
    let job = thread::spawn(jobs.try_recv().expect("the call hands a job"));
    told.recv_timeout(PATIENT)
        .expect("the call's failure wakes the caller");
    drop(placed);
    assert_eq!(freed_so_far(&freed), [], "the job holds the state");
    resume.send(()).expect("the waker waits to resume");
    job.join()
        .expect("the state's drop panic stays off the pool's thread");
    assert_eq!(freed_so_far(&freed), [0]);
}

/// A device whose state fails to power down, on a pool, dropped while its
/// second call runs, a call whose logic then panics (two bugs): the
/// job, which stops as the call fails, is the last to hold the executor
/// it places, and frees it on the pool's thread, where the state's drop
/// panic is absorbed.
#[test]
fn a_states_drop_that_panics_as_a_job_ending_after_drop_frees_it_stays_off_the_placements_thread() {
    let freed = Freed::default();
    let (started, running) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let released = Mutex::new(released);
    let (mut placed, jobs) = holding_a_state_that_powers_down_badly(
        &freed,
        move |_: &mut PowersDownBadly, input: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            if input.instructions().contains(&Op::Panic) {
                let _ = started.send(());
                let released = released.lock().expect("nothing panics holding the lock");
                let _ = released.recv_timeout(PATIENT);
                panic!("the device's driver panics");
            }
            Ok(())
        },
    );
    let failing = placed
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Panic],
        })
        .expect("an open executor queues a call");
    let job = thread::spawn(jobs.try_recv().expect("the call hands a job"));
    running.recv_timeout(PATIENT).expect("the second call runs");
    drop(placed);
    assert_eq!(freed_so_far(&freed), [], "the job holds the state");
    release.send(()).expect("the logic waits to go on");
    job.join()
        .expect("the state's drop panic stays off the pool's thread");
    assert_eq!(freed_so_far(&freed), [0]);
    drop(failing);
}

/// An implementer's own executor, not made with the crate's facilities,
/// whose drop fails to power its device down (a bug of its own): it
/// counts itself dropped, then panics with `PowerDownFailed(0)`. Its
/// calls run nothing.
struct DevicePoweringDownBadly(Arc<AtomicUsize>);

impl Executor<InstructionBatch<Op>> for DevicePoweringDownBadly {
    type Reply = ImmediateReply<ShotOutcome>;
    fn call(&mut self, _: InstructionBatchRef<'_, Op>) -> Result<Self::Reply, Rejected> {
        Ok(ImmediateReply::done(Vec::new()))
    }
}

impl Describe for DevicePoweringDownBadly {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Drop for DevicePoweringDownBadly {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
        panic::panic_any(PowerDownFailed(0));
    }
}

/// An implementer's executor whose own drop panics, placed on a pool that
/// keeps the jobs it is handed, is dropped while its first call's job is
/// handed but not run; the pool then drops that job unrun (it shuts
/// down): the job, the last to hold the executor it places, frees it on
/// the pool's thread, where its drop's panic is absorbed, after the panic
/// hook reported it, so that it does not reach the pool's thread: the
/// job's release absorbs it, not the executor's own drop, as the crate's
/// executors' do.
#[test]
fn an_executors_drop_that_panics_as_a_job_dropped_unrun_frees_it_stays_off_the_placements_thread() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let (placement, jobs) = holding();
    let mut placed = PlacedExecutor::new(
        DevicePoweringDownBadly(Arc::clone(&dropped)),
        placement,
        ExecutorLifecycle::default(),
        drop,
    );
    drop(
        placed
            .call(InstructionBatchRef {
                shots: &ShotSet::one(0),
                instructions: &[Op::Flip],
            })
            .expect("an open executor queues a call"),
    );
    let job = jobs.try_recv().expect("the call hands a job");
    drop(placed);
    assert_eq!(
        dropped.load(Ordering::SeqCst),
        0,
        "the job holds the executor"
    );
    thread::spawn(move || drop(job))
        .join()
        .expect("the executor's drop panic stays off the pool's thread");
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}

/// A device on a pool, whose logic holds a handle on its connection
/// that fails to release, and panics in a call (two bugs), which stops
/// the placement: `close` frees the executor it places where it is
/// called, releasing the handle, and raises no panic of its drop, which
/// is not a state's: only a state's drop panic leaves `close`.
#[test]
fn once_stopped_its_close_frees_what_the_closures_capture_raising_no_panic_of_their_drops() {
    let (handle, released) = ReleasesBadly::new();
    let handle = Arc::new(handle);
    let (placement, jobs) = holding();
    let mut placed = executor_fn(
        Capabilities::default(),
        own_bit,
        move |_: &mut Bit, input: &CallInput<'_, Op>, _: &mut Outcomes<'_>| {
            let _held = &handle;
            assert!(
                !input.instructions().contains(&Op::Panic),
                "the device's driver panics"
            );
            Ok(())
        },
    )
    .run_on(placement);
    let reply = placed
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Panic],
        })
        .expect("an open executor queues a call");
    thread::spawn(jobs.try_recv().expect("the call hands a job"))
        .join()
        .expect("the job keeps the logic's panic");
    ended(reply).1.expect_err("the call panicked");
    panic::catch_unwind(AssertUnwindSafe(|| drop(placed.close())))
        .expect("`close` raises no panic of a capture's drop");
    assert_eq!(released.load(Ordering::SeqCst), 1);
    drop(placed);
    assert_eq!(
        released.load(Ordering::SeqCst),
        1,
        "released once, by `close`"
    );
}

/// A device placed by a closure that holds a handle on the pool it hands
/// jobs to, which fails to release (a bug of the placement's), dropped
/// before its first call: its drop releases the handle and raises no panic
/// of its drop.
#[test]
fn before_its_first_call_its_drop_frees_what_its_placement_captures_raising_no_panic_of_its_drop() {
    let (handle, released) = ReleasesBadly::new();
    let placed =
        executor_fn(Capabilities::default(), own_bit, run_bit).run_on(move |job: PlacementJob| {
            let _held = &handle;
            job();
        });
    panic::catch_unwind(AssertUnwindSafe(|| drop(placed)))
        .expect("`drop` raises no panic of a capture's drop");
    assert_eq!(released.load(Ordering::SeqCst), 1);
}

/// The same device, dropped once its placement ran a call: its drop
/// releases the handle the started placement holds, and raises no panic
/// of its drop.
#[test]
fn once_started_its_drop_frees_what_its_placement_captures_raising_no_panic_of_its_drop() {
    let (handle, released) = ReleasesBadly::new();
    let mut placed =
        executor_fn(Capabilities::default(), own_bit, run_bit).run_on(move |job: PlacementJob| {
            let _held = &handle;
            job();
        });
    ended(
        placed
            .call(InstructionBatchRef {
                shots: &ShotSet::one(0),
                instructions: &[Op::Flip],
            })
            .expect("an open executor queues a call"),
    )
    .1
    .expect("the call runs");
    panic::catch_unwind(AssertUnwindSafe(|| drop(placed)))
        .expect("`drop` raises no panic of a capture's drop");
    assert_eq!(released.load(Ordering::SeqCst), 1);
}

/// The report closure of a device on a thread of its own has a bug: it
/// panics as `close` makes the report there. `close` re-raises the panic
/// in its caller, its payload unchanged, as the same executor not placed
/// would panic there: work moved to another thread reports its panic to the
/// caller that waits for it, as `std::thread::scope` and rayon do, the
/// least surprise for a Rust user.
#[test]
fn a_panic_making_the_report_at_close_reaches_the_closer() {
    let mut placed = recording(&Threads::default())
        .state_report(|_: &Bit| -> Count { panic!("the report has a bug") })
        .run_on(OwnThread);
    let (_, end) = send(&mut placed, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let raised = ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| placed.close()))
            .map(drop)
            .map_err(|payload| payload.downcast_ref::<&str>().copied())
    });
    assert_eq!(raised, Err(Some("the report has a bug")));
}

/// The report closure of a device on a thread of its own panicked as the
/// first `close` made the report, and the panic reached the closer, who
/// closes again: the second `close` returns an empty report, as every
/// second `close` does, neither waiting for ever nor panicking again.
#[test]
fn after_a_panic_at_close_a_second_close_reports_nothing() {
    let mut placed = recording(&Threads::default())
        .state_report(|_: &Bit| -> Count { panic!("the report has a bug") })
        .run_on(OwnThread);
    let (_, end) = send(&mut placed, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    let report = ends_soon(move || {
        panic::catch_unwind(AssertUnwindSafe(|| placed.close()))
            .expect_err("the report's panic reaches the closer");
        placed.close()
    });
    assert_eq!(report, ExecutorReport::default());
}

/// What a device's closure holds that must be dropped where its calls ran
/// (a handle on the device's driver): it sends the thread it is dropped on.
struct DroppedOn(mpsc::Sender<ThreadId>);

impl Drop for DroppedOn {
    fn drop(&mut self) {
        let _ = self.0.send(thread::current().id());
    }
}

/// A device on a pool's threads, whose report closure holds a handle on
/// the device's driver, which must be dropped where its calls ran, and
/// panics at close: the panic reaches the closer, but does not stop the
/// placement, so, dropped, the executor it places is dropped on the pool,
/// the closure's handle with it, not on the thread that drops it.
#[test]
fn after_a_panic_at_close_it_still_drops_the_executor_it_places_on_its_placement() {
    let (dropped_on, dropped) = mpsc::channel();
    let handle = DroppedOn(dropped_on);
    let (placement, jobs) = holding();
    let mut placed = recording(&Threads::default())
        .state_report(move |_: &Bit| -> Count {
            let DroppedOn(_) = &handle;
            panic!("the report has a bug")
        })
        .run_on(placement);
    let shot = ShotSet::one(0);
    let reply = placed
        .call(InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Flip],
        })
        .expect("an open executor queues a call");
    let job = jobs.try_recv().expect("the call hands a job");
    thread::spawn(job).join().expect("the job does not panic");
    ended(reply).1.expect("the logic does not fail");
    let closer = thread::spawn(move || {
        panic::catch_unwind(AssertUnwindSafe(|| placed.close()))
            .expect_err("the report's panic reaches the closer");
        placed
    });
    let job = jobs.recv_timeout(PATIENT).expect("the close hands a job");
    thread::spawn(job)
        .join()
        .expect("the job keeps the report's panic");
    let placed = ends_soon(move || closer.join().expect("the closer catches the panic"));
    drop(placed);
    let pool = thread::spawn(move || {
        for job in jobs.try_iter() {
            job();
        }
        thread::current().id()
    })
    .join()
    .expect("the jobs do not panic");
    assert_eq!(
        dropped.recv_timeout(PATIENT),
        Ok(pool),
        "dropped on the pool's thread, not the caller's"
    );
}

/// The executor's contract on close, through the conformance kit.
#[test]
fn closed_it_refuses_every_call() {
    let mut placed = recording(&Threads::default()).run_on(OwnThread);
    ends_soon(move || {
        let shot = ShotSet::one(0);
        conformance::rejects_calls_once_closed(
            &mut placed,
            InstructionBatchRef {
                shots: &shot,
                instructions: &[Op::Read(0)],
            },
        );
    });
}

/// The report of a run is counted once, through the conformance kit.
#[test]
fn a_second_close_reports_nothing() {
    let mut placed = recording(&Threads::default()).run_on(OwnThread);
    let (_, end) = send(&mut placed, &[0], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic does not fail");
    ends_soon(move || conformance::reports_once(&mut placed));
}

/// A second call on shot 0, on the executor's thread, reads the flip of
/// the first: the conformance kit's check of `resumable_shots`. The check that
/// a call changes no shot it does not name does not apply: the executor holds
/// one shot.
#[test]
fn a_second_call_continues_its_shot() {
    let threads = Threads::default();
    ends_soon(move || {
        conformance::continues_a_shot_from_one_call_to_the_next(
            || recording(&threads).run_on(OwnThread),
            &ShotSet::one(0),
            &[Op::Flip],
            &[Op::Read(0)],
        );
    });
}

/// A state per shot that draws its value from its seed, every shot on one
/// thread: each of shots 1, 3 and 5 reads the same 16 bits of its value
/// in two calls, whether it runs alone, among the others, or with them
/// split into calls in another order: the conformance kit's check of
/// `replayable`.
#[test]
fn placed_after_one_per_shot_each_shot_alone_gives_what_it_gives_among_others() {
    let drawing = || {
        executor_fn(
            Capabilities::default().with_replayable(true),
            draw_value,
            read_drawn_bits,
        )
        .seed(7)
        .shots(OnePerShot)
        .run_on(OwnThread)
    };
    ends_soon(move || {
        let ops: Vec<_> = (0..16).map(DrawingOp::Read).collect();
        conformance::gives_each_shot_alone_what_it_gives_among_others(
            drawing,
            &ShotSet::new(vec![1, 3, 5]).expect("three shots"),
            &[&ops[..8], &ops[8..]],
        );
    });
}

/// A second call on shots 0 and 1, every shot on one thread, reads the
/// flip of the first: the conformance kit's check of `resumable_shots`.
#[test]
fn placed_after_one_per_shot_a_second_call_continues_each_shot() {
    let threads = Threads::default();
    ends_soon(move || {
        conformance::continues_a_shot_from_one_call_to_the_next(
            || recording(&threads).shots(OnePerShot).run_on(OwnThread),
            &ShotSet::range(0..=1).expect("two shots"),
            &[Op::Flip],
            &[Op::Read(0)],
        );
    });
}

/// A flip on shots 1 and 3, every shot on one thread, leaves shots 0 and
/// 2, flipped before, as they were: the conformance kit's check that a
/// call changes no shot it does not name.
#[test]
fn placed_after_one_per_shot_a_call_changes_no_shot_it_does_not_name() {
    let threads = Threads::default();
    ends_soon(move || {
        conformance::changes_no_shot_a_call_does_not_name(
            || recording(&threads).shots(OnePerShot).run_on(OwnThread),
            &ShotSet::range(0..=3).expect("four shots"),
            &[Op::Flip],
            &[Op::Read(0)],
        );
    });
}

/// After an executor flipped shots 0 to 3, every shot on one thread, and
/// was closed, or dropped unclosed, the next one reads them as made: the
/// conformance kit's check that a new executor starts its shots as made.
#[test]
fn placed_after_one_per_shot_a_new_executor_starts_its_shots_as_made() {
    let threads = Threads::default();
    ends_soon(move || {
        conformance::starts_its_shots_as_made_whatever_an_earlier_executor_left(
            || recording(&threads).shots(OnePerShot).run_on(OwnThread),
            &ShotSet::range(0..=3).expect("four shots"),
            &[Op::Flip],
            &[Op::Read(0)],
        );
    });
}

/// A chain is composed against what its executor offers wherever it
/// runs: a placed executor promises what the executor it places does.
#[test]
fn a_placed_executor_promises_what_the_executor_it_places_does() {
    let threads = Threads::default();
    let promised = recording(&threads).capabilities();
    assert_eq!(
        recording(&threads).run_on(OwnThread).capabilities(),
        promised
    );
    let per_shot = recording(&threads).shots(OnePerShot);
    let promised = per_shot.capabilities();
    assert_eq!(per_shot.run_on(OwnThread).capabilities(), promised);
}
