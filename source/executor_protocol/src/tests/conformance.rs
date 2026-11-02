// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Each check of the conformance kit catches an implementation that breaks
//! its rule, and refuses inputs that would not let it check the rule. That
//! the checks pass on implementations that keep the rules is shown where
//! they run on the crate's own executors (the tests of `compose/adapters`,
//! `executor`, `executor_fn`, `executor_fn/one_per_shot`,
//! `executor_fn/one_per_shot/placed`, `instruction_set/renumbered`,
//! `lifecycle`, `placement` and `shot_states`) and on the guides'.

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::task::Waker;

use crate::conformance::{
    accepts_calls_after_a_refusal, changes_no_shot_a_call_does_not_name,
    continues_a_shot_from_one_call_to_the_next, default_is_empty,
    ends_calls_in_flight_at_an_earlier_failure, folds_in_any_order,
    gives_each_shot_alone_what_it_gives_among_others,
    keeps_each_shots_conditions_from_one_call_to_the_next,
    refuses_a_repeated_tag_at_the_index_of_its_instruction, refuses_a_tag_repeated_within_a_batch,
    refuses_calls_after_a_failure, refuses_calls_outside_its_set, rejects_calls_once_closed,
    reports_once, starts_its_shots_as_made_whatever_an_earlier_executor_left,
};
use crate::reply::CANCELLATION;
use crate::{
    Close, Executor, ExecutorLifecycle, Fold, ImmediateReply, IncrementalReply, InstructionBatch,
    InstructionBatchRef, InstructionCall, InstructionMismatch, InstructionSet, Operand,
    OperandKind, Rejected, Reply, ReplyStatus, ShotConditions, ShotId, ShotOutcome, ShotSet,
    check_unique_tags, reply_channel,
};

use super::{Count, SharedState, bits_set, flip_and_toss, on_shot_0, shots};

/// The rule of `close` a [`TestDoubleBreaksClose`] breaks.
#[derive(Clone, Copy, PartialEq)]
enum Broken {
    /// It still runs calls once closed.
    RunsAfterClose,
    /// It rejects calls once closed, but not with `Closed`.
    RejectsOtherwise,
    /// It keeps its totals at close, so that each close reports them.
    KeepsTotals,
    /// It reports its totals at the second close, the first reporting
    /// nothing.
    ReportsLate,
}

/// An executor that breaks one rule of `close`.
struct TestDoubleBreaksClose {
    broken: Broken,
    calls: u64,
    closed: bool,
}

impl TestDoubleBreaksClose {
    fn new(broken: Broken) -> Self {
        Self {
            broken,
            calls: 0,
            closed: false,
        }
    }
}

impl Executor<InstructionBatch<bool>> for TestDoubleBreaksClose {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, _: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        match self.broken {
            _ if !self.closed => {}
            Broken::RunsAfterClose => {}
            Broken::RejectsOtherwise => return Err(Rejected::Unavailable("busy".into())),
            Broken::KeepsTotals | Broken::ReportsLate => return Err(Rejected::Closed),
        }
        self.calls += 1;
        Ok(ImmediateReply::done(Vec::new()))
    }
}

impl Close for TestDoubleBreaksClose {
    type Report = Count;

    fn close(&mut self) -> Count {
        let first = !self.closed;
        self.closed = true;
        match self.broken {
            Broken::KeepsTotals => Count(self.calls),
            Broken::ReportsLate if first => Count::default(),
            _ => Count(std::mem::take(&mut self.calls)),
        }
    }
}

/// An executor that runs a call after it was closed breaks the first rule
/// of `close`.
#[test]
#[should_panic(expected = "a call after close must be rejected with `Closed`, but it was accepted")]
fn rejects_calls_once_closed_catches_a_call_run_after_close() {
    let mut executor = TestDoubleBreaksClose::new(Broken::RunsAfterClose);
    rejects_calls_once_closed::<InstructionBatch<bool>, _>(&mut executor, on_shot_0(&[true]));
}

/// An executor that rejects a call after close for another reason than
/// `Closed` breaks the rule too: the caller must learn that the executor is
/// closed, not that it may run the call later.
#[test]
#[should_panic(expected = "a call after close must be rejected with `Closed`, not `Unavailable")]
fn rejects_calls_once_closed_catches_another_rejection() {
    let mut executor = TestDoubleBreaksClose::new(Broken::RejectsOtherwise);
    rejects_calls_once_closed::<InstructionBatch<bool>, _>(&mut executor, on_shot_0(&[true]));
}

/// An executor that reports its totals at each close breaks the second
/// rule: folding its reports would count its work twice.
#[test]
#[should_panic(expected = "a second close must return an empty report")]
fn reports_once_catches_totals_reported_twice() {
    let mut executor = TestDoubleBreaksClose::new(Broken::KeepsTotals);
    executor
        .call(on_shot_0(&[true]))
        .expect("an open `TestDoubleBreaksClose` runs every call");
    reports_once(&mut executor);
}

/// An executor that keeps its totals at close, but was never called,
/// reports nothing at either close, so its fault would not show: the check
/// needs an executor that did some work.
#[test]
#[should_panic(expected = "`executor` must have done some work")]
fn reports_once_needs_an_executor_that_did_some_work() {
    reports_once(&mut TestDoubleBreaksClose::new(Broken::KeepsTotals));
}

/// An executor that reports its work at its second close only breaks the
/// rule, not the check's need: a run, which closes it once, would count
/// none of its work.
#[test]
#[should_panic(
    expected = "the first close must report the executor's work, not a later one, but the \
                first report was empty and the second was not"
)]
fn reports_once_catches_a_report_at_the_second_close_only() {
    let mut executor = TestDoubleBreaksClose::new(Broken::ReportsLate);
    executor
        .call(on_shot_0(&[true]))
        .expect("an open `TestDoubleBreaksClose` runs every call");
    reports_once(&mut executor);
}

/// How a [`TestDoubleForgetsFailure`] breaks the rule that a failure is
/// final.
#[derive(Clone, Copy)]
enum AfterFailure {
    /// It still runs calls.
    Runs,
    /// It refuses calls, but not with `EarlierFailure`.
    RefusesOtherwise,
    /// It refuses calls with `EarlierFailure`, naming no failure.
    RefusesUnnamed,
    /// It refuses calls with `EarlierFailure`, naming another failure.
    RefusesNamingAnother,
    /// It refuses the first call, then forgets the failure.
    RefusesOnce,
    /// It still refuses calls with `EarlierFailure` once closed.
    IgnoresClose,
    /// It refuses calls with `EarlierFailure`, but runs them once closed.
    RunsOnceClosed,
}

/// An executor whose state carries over, whose device stops at a `false`,
/// and that breaks the rule that a failure is final.
struct TestDoubleForgetsFailure {
    after: AfterFailure,
    failed: bool,
    closed: bool,
}

impl TestDoubleForgetsFailure {
    fn new(after: AfterFailure) -> Self {
        Self {
            after,
            failed: false,
            closed: false,
        }
    }
}

impl Executor<InstructionBatch<bool>> for TestDoubleForgetsFailure {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        let batch = batch.instructions;
        if self.closed {
            match self.after {
                AfterFailure::IgnoresClose => {}
                AfterFailure::RunsOnceClosed => return Ok(ImmediateReply::done(Vec::new())),
                _ => return Err(Rejected::Closed),
            }
        }
        if self.failed {
            match self.after {
                AfterFailure::Runs => {}
                AfterFailure::RefusesOtherwise => {
                    return Err(Rejected::Unavailable("busy".into()));
                }
                AfterFailure::RefusesUnnamed => {
                    return Err(Rejected::EarlierFailure(String::new()));
                }
                AfterFailure::RefusesNamingAnother => {
                    return Err(Rejected::EarlierFailure("another failure".into()));
                }
                AfterFailure::RefusesOnce => {
                    self.failed = false;
                    return Err(Rejected::EarlierFailure("the device stopped".into()));
                }
                AfterFailure::IgnoresClose | AfterFailure::RunsOnceClosed => {
                    return Err(Rejected::EarlierFailure("the device stopped".into()));
                }
            }
        }
        if batch.contains(&false) {
            self.failed = true;
            return Ok(ImmediateReply::failed(
                Vec::new(),
                "the device stopped".into(),
            ));
        }
        Ok(ImmediateReply::done(Vec::new()))
    }
}

impl Close for TestDoubleForgetsFailure {
    type Report = ();

    fn close(&mut self) {
        self.closed = true;
    }
}

/// An executor that runs a call after an earlier one failed breaks the
/// rule: the call would run on a state its caller never made. The panic
/// shows what the call's reply gave.
#[test]
#[should_panic(
    expected = "a call after a failure must be rejected with `EarlierFailure`, but it was \
                accepted (`later[0]`), and its reply, polled once, was `Done` with 0 events"
)]
fn refuses_calls_after_a_failure_catches_a_call_run_after_a_failure() {
    let mut executor = TestDoubleForgetsFailure::new(AfterFailure::Runs);
    refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[false]),
        &[on_shot_0(&[true])],
    );
}

/// An executor that refuses a call after a failure for another reason than
/// `EarlierFailure` breaks the rule too: the caller must learn that the run
/// is lost, not that it may send the call again.
#[test]
#[should_panic(
    expected = "a call after a failure must be rejected with `EarlierFailure`, not `Unavailable"
)]
fn refuses_calls_after_a_failure_catches_another_rejection() {
    let mut executor = TestDoubleForgetsFailure::new(AfterFailure::RefusesOtherwise);
    refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[false]),
        &[on_shot_0(&[true])],
    );
}

/// An executor that refuses a call after a failure without naming the
/// failure breaks the rule too: the caller cannot tell which failure ended
/// its run.
#[test]
#[should_panic(expected = "a refusal after a failure must name the failure, `the device stopped`")]
fn refuses_calls_after_a_failure_catches_a_refusal_that_does_not_name_the_failure() {
    let mut executor = TestDoubleForgetsFailure::new(AfterFailure::RefusesUnnamed);
    refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[false]),
        &[on_shot_0(&[true])],
    );
}

/// An executor that refuses a call after a failure naming another failure
/// than the one its reply gave breaks the rule too: the caller would look
/// for a cause that did not end its run.
#[test]
#[should_panic(
    expected = "a refusal after a failure must name the failure, `the device stopped`, but the \
                refusal of `later[0]` says `another failure`"
)]
fn refuses_calls_after_a_failure_catches_a_refusal_that_names_another_failure() {
    let mut executor = TestDoubleForgetsFailure::new(AfterFailure::RefusesNamingAnother);
    refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[false]),
        &[on_shot_0(&[true])],
    );
}

/// An executor that refuses the first call after a failure, then runs the
/// next, breaks the rule too: the failure is final for every later call.
#[test]
#[should_panic(
    expected = "a call after a failure must be rejected with `EarlierFailure`, but it was \
                accepted (`later[1]`)"
)]
fn refuses_calls_after_a_failure_catches_a_call_run_after_a_refusal() {
    let mut executor = TestDoubleForgetsFailure::new(AfterFailure::RefusesOnce);
    refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[false]),
        &[on_shot_0(&[true]), on_shot_0(&[true])],
    );
}

/// An executor that still says that an earlier call failed once it is
/// closed breaks the rule of close, which comes first: the caller called an
/// executor it closed, a fault of its own, whatever happened before.
#[test]
#[should_panic(
    expected = "a call after a failure and close must be rejected with `Closed`, not `EarlierFailure"
)]
fn refuses_calls_after_a_failure_catches_a_failure_said_after_close() {
    let mut executor = TestDoubleForgetsFailure::new(AfterFailure::IgnoresClose);
    refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[false]),
        &[on_shot_0(&[true])],
    );
}

/// An executor that refuses calls after a failure, but runs one once
/// closed, breaks the rule of close: the call would run on the state the
/// failure left, after its caller ended the run.
#[test]
#[should_panic(
    expected = "a call after a failure and close must be rejected with `Closed`, but it was \
                accepted"
)]
fn refuses_calls_after_a_failure_catches_a_call_run_after_close() {
    let mut executor = TestDoubleForgetsFailure::new(AfterFailure::RunsOnceClosed);
    refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[false]),
        &[on_shot_0(&[true])],
    );
}

/// An executor whose failing call's reply ends cancelled, and that then
/// refuses calls naming the cancellation.
struct TestDoubleCancelsItsCall {
    cancelled: bool,
}

impl Executor<InstructionBatch<bool>> for TestDoubleCancelsItsCall {
    type Reply = IncrementalReply<ShotOutcome>;

    fn call(&mut self, _: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        if self.cancelled {
            return Err(Rejected::EarlierFailure(CANCELLATION.to_owned()));
        }
        self.cancelled = true;
        let (sender, reply) = reply_channel();
        sender.cancel();
        Ok(reply)
    }
}

impl Close for TestDoubleCancelsItsCall {
    type Report = ();

    fn close(&mut self) {}
}

/// An executor whose failing message's reply ends cancelled, not failed,
/// does not show the rule, though it then refuses calls naming the
/// cancellation: a cancellation says that the work was stopped from
/// outside, not that it failed, so the check needs a reply that ends
/// `Failed`, and polls it to tell the two apart, where `wait` gives both
/// as text.
#[test]
#[should_panic(expected = "the failing message's reply must end failed, not cancelled")]
fn refuses_calls_after_a_failure_catches_a_cancelled_reply() {
    let mut executor = TestDoubleCancelsItsCall { cancelled: false };
    refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[false]),
        &[on_shot_0(&[true])],
    );
}

/// What a [`TestDoubleOfOneInstruction`] does when it refuses a batch.
#[derive(Clone, Copy, PartialEq)]
enum OnARefusal {
    /// Nothing more: it keeps the rule that a refusal leaves it usable.
    Nothing,
    /// It fails, as if the batch had started: it refuses every later call.
    Fails,
    /// It fails the reply of the next call.
    FailsTheNextReply,
    /// It refuses the batch with `EarlierFailure`.
    SaysEarlierFailure,
    /// It refuses the batch with `Closed`.
    SaysClosed,
    /// It refuses the batch with `NoReply`, as a link that lost the
    /// verdict does.
    SaysNoReply,
}

/// An executor whose device runs one instruction per batch, so that it
/// refuses a batch of more at its second, and stops at a `false`, saying
/// `reason`. It keeps the rules of `Executor` on an [`ExecutorLifecycle`],
/// but for what it does on a refusal.
struct TestDoubleOfOneInstruction {
    lifecycle: ExecutorLifecycle,
    reason: &'static str,
    on_a_refusal: OnARefusal,
    next_reply_fails: bool,
}

impl TestDoubleOfOneInstruction {
    fn new(on_a_refusal: OnARefusal) -> Self {
        Self {
            lifecycle: ExecutorLifecycle::default(),
            reason: "the device stopped",
            on_a_refusal,
            next_reply_fails: false,
        }
    }
}

impl Executor<InstructionBatch<bool>> for TestDoubleOfOneInstruction {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        let batch = batch.instructions;
        self.lifecycle.admit()?;
        if batch.len() > 1 {
            match self.on_a_refusal {
                OnARefusal::Nothing => {}
                OnARefusal::Fails => self.lifecycle.fail("refused"),
                OnARefusal::FailsTheNextReply => self.next_reply_fails = true,
                OnARefusal::SaysEarlierFailure => {
                    return Err(Rejected::EarlierFailure("refused".into()));
                }
                OnARefusal::SaysClosed => return Err(Rejected::Closed),
                OnARefusal::SaysNoReply => {
                    return Err(Rejected::NoReply("the link dropped".into()));
                }
            }
            return Err(Rejected::Instruction {
                at: 1,
                reason: "one instruction per batch".into(),
            });
        }
        if std::mem::take(&mut self.next_reply_fails) || batch == [false] {
            self.lifecycle.fail(self.reason);
            return Ok(ImmediateReply::failed(Vec::new(), self.reason.into()));
        }
        Ok(ImmediateReply::done(Vec::new()))
    }
}

impl Close for TestDoubleOfOneInstruction {
    type Report = ();

    fn close(&mut self) {
        self.lifecycle.close();
    }
}

/// An executor that fails when it refuses a batch breaks the rule that a
/// refusal means nothing ran: the caller corrects its batch, and every
/// later call is refused.
#[test]
#[should_panic(
    expected = "a call after a refusal must be accepted, but it was refused with `EarlierFailure"
)]
fn accepts_calls_after_a_refusal_catches_a_failure_at_a_refusal() {
    let mut executor = TestDoubleOfOneInstruction::new(OnARefusal::Fails);
    accepts_calls_after_a_refusal::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[true, true]),
        on_shot_0(&[true]),
    );
}

/// An executor that accepts the call after a refusal but fails its reply
/// breaks the rule too: the refused batch left a state behind.
#[test]
#[should_panic(expected = "a call after a refusal must run, but its reply failed")]
fn accepts_calls_after_a_refusal_catches_a_reply_failed_after_a_refusal() {
    let mut executor = TestDoubleOfOneInstruction::new(OnARefusal::FailsTheNextReply);
    accepts_calls_after_a_refusal::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[true, true]),
        on_shot_0(&[true]),
    );
}

/// An executor that refuses a batch for its own fault with `EarlierFailure`
/// breaks the rule: the caller would give up a run it could correct.
#[test]
#[should_panic(
    expected = "a call refused for its own fault must not be refused with `EarlierFailure"
)]
fn accepts_calls_after_a_refusal_catches_a_refusal_as_an_earlier_failure() {
    let mut executor = TestDoubleOfOneInstruction::new(OnARefusal::SaysEarlierFailure);
    accepts_calls_after_a_refusal::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[true, true]),
        on_shot_0(&[true]),
    );
}

/// An executor that refuses a batch for its own fault with `Closed` breaks
/// the rule too: the caller would think it closed the executor.
#[test]
#[should_panic(expected = "a call refused for its own fault must not be refused with `Closed`")]
fn accepts_calls_after_a_refusal_catches_a_refusal_as_closed() {
    let mut executor = TestDoubleOfOneInstruction::new(OnARefusal::SaysClosed);
    accepts_calls_after_a_refusal::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[true, true]),
        on_shot_0(&[true]),
    );
}

/// The check needs a message the executor refuses: one it accepts shows
/// nothing of a refusal.
#[test]
#[should_panic(expected = "the refused message must be refused")]
fn accepts_calls_after_a_refusal_needs_a_refused_message() {
    let mut executor = TestDoubleOfOneInstruction::new(OnARefusal::Nothing);
    accepts_calls_after_a_refusal::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[true]),
        on_shot_0(&[true]),
    );
}

/// The check needs a refusal after which nothing ran: after `NoReply`, the
/// refused message's work may have run, so a later call may rightly fail.
#[test]
#[should_panic(
    expected = "the refused message must be refused with a class that says that nothing ran, \
                not `NoReply"
)]
fn accepts_calls_after_a_refusal_needs_a_refusal_after_which_nothing_ran() {
    let mut executor = TestDoubleOfOneInstruction::new(OnARefusal::SaysNoReply);
    accepts_calls_after_a_refusal::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[true, true]),
        on_shot_0(&[true]),
    );
}

/// The check needs a later message: with none, it would check nothing of
/// the refusals after a failure.
#[test]
#[should_panic(expected = "`later` must hold a message to refuse")]
fn refuses_calls_after_a_failure_needs_a_later_message() {
    let mut executor = TestDoubleOfOneInstruction::new(OnARefusal::Nothing);
    refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[false]),
        &[],
    );
}

/// The check needs a failing message that the executor accepts: a refused
/// one starts no work that could fail.
#[test]
#[should_panic(expected = "the failing message must be accepted")]
fn refuses_calls_after_a_failure_needs_a_failing_message_accepted() {
    let mut executor = TestDoubleOfOneInstruction::new(OnARefusal::Nothing);
    refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[false, false]),
        &[on_shot_0(&[true])],
    );
}

/// The check needs a failing message whose reply fails: after a reply that
/// ends well, the executor rightly runs the later calls.
#[test]
#[should_panic(expected = "the failing message's reply must end failed")]
fn refuses_calls_after_a_failure_needs_a_reply_that_fails() {
    let mut executor = TestDoubleOfOneInstruction::new(OnARefusal::Nothing);
    refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[true]),
        &[on_shot_0(&[true])],
    );
}

/// The check needs a failure that says why: an empty reason is contained in
/// every refusal's, so the check could not tell whether they name it.
#[test]
#[should_panic(expected = "the failing message's reply must say why it failed")]
fn refuses_calls_after_a_failure_needs_a_failure_that_says_why() {
    let mut executor = TestDoubleOfOneInstruction {
        reason: "",
        ..TestDoubleOfOneInstruction::new(OnARefusal::Nothing)
    };
    refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        on_shot_0(&[false]),
        &[on_shot_0(&[true])],
    );
}

/// An executor that runs a call made as an earlier one fails breaks the
/// rule: the call ran on the state the failure left part-way.
#[test]
#[should_panic(
    expected = "a call in flight at an earlier failure must end failed, with that failure, \
                `the device stopped`, not `Done`"
)]
fn ends_calls_in_flight_at_an_earlier_failure_catches_a_call_run_after_a_failure() {
    ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<bool>, _>(
        &mut TestDoubleForgetsFailure::new(AfterFailure::Runs),
        on_shot_0(&[false]),
        on_shot_0(&[true]),
    );
}

/// An executor that refuses a call made as an earlier one fails for
/// another reason than `EarlierFailure` breaks the rule too: the caller
/// must learn that the run is lost, not that it may send the call again.
#[test]
#[should_panic(
    expected = "a call in flight at an earlier failure must be refused with `EarlierFailure`, \
                or end failed, not be refused with `Unavailable"
)]
fn ends_calls_in_flight_at_an_earlier_failure_catches_another_rejection() {
    ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<bool>, _>(
        &mut TestDoubleForgetsFailure::new(AfterFailure::RefusesOtherwise),
        on_shot_0(&[false]),
        on_shot_0(&[true]),
    );
}

/// An executor that refuses a call made as an earlier one fails, naming
/// another failure, breaks the rule too: the caller would look for a cause
/// that did not end its run.
#[test]
#[should_panic(
    expected = "a refusal after a failure must name the failure, `the device stopped`, but the \
                refusal of `later` says `another failure`"
)]
fn ends_calls_in_flight_at_an_earlier_failure_catches_a_refusal_that_names_another_failure() {
    ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<bool>, _>(
        &mut TestDoubleForgetsFailure::new(AfterFailure::RefusesNamingAnother),
        on_shot_0(&[false]),
        on_shot_0(&[true]),
    );
}

/// How a [`TestDoubleEndsLaterCalls`] ends the replies of calls made once
/// a call failed.
#[derive(Clone, Copy)]
enum LaterReply {
    /// It fails them, naming another failure.
    FailsNamingAnother,
    /// It fails them, naming the failure, but hands over a readout first.
    HandsOverAReadout,
    /// It cancels them.
    Cancelled,
}

/// An executor whose device stops at a `false`, and that accepts the calls
/// made once a call failed, as one that runs its calls elsewhere does, but
/// ends their replies breaking the rule that they end with the failure.
struct TestDoubleEndsLaterCalls {
    later: LaterReply,
    failed: bool,
}

impl Executor<InstructionBatch<bool>> for TestDoubleEndsLaterCalls {
    type Reply = IncrementalReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        let (sender, reply) = reply_channel();
        if self.failed {
            match self.later {
                LaterReply::FailsNamingAnother => sender.fail("another failure".into()),
                LaterReply::HandsOverAReadout => {
                    sender.push(ShotOutcome::new(0, 0, true.into()));
                    sender.fail("an earlier call failed: the device stopped".into());
                }
                LaterReply::Cancelled => sender.cancel(),
            }
        } else if batch.instructions.contains(&false) {
            self.failed = true;
            sender.fail("the device stopped".into());
        } else {
            sender.done();
        }
        Ok(reply)
    }
}

/// An executor that fails a call made as an earlier one fails, naming
/// another failure, breaks the rule: the caller would look for a cause that
/// did not end its run.
#[test]
#[should_panic(
    expected = "a call in flight at an earlier failure must end with that failure, \
                `the device stopped`, but its reply says `another failure`"
)]
fn ends_calls_in_flight_at_an_earlier_failure_catches_a_reply_that_names_another_failure() {
    ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<bool>, _>(
        &mut TestDoubleEndsLaterCalls {
            later: LaterReply::FailsNamingAnother,
            failed: false,
        },
        on_shot_0(&[false]),
        on_shot_0(&[true]),
    );
}

/// An executor that fails a call made as an earlier one fails, naming the
/// failure, but hands over a readout first breaks the rule too: the
/// readout was read from the state the failure left part-way.
#[test]
#[should_panic(
    expected = "a call in flight at an earlier failure must hand over no event, since it would \
                read the state the failure left, but it handed over 1"
)]
fn ends_calls_in_flight_at_an_earlier_failure_catches_a_reply_that_hands_over_a_readout() {
    ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<bool>, _>(
        &mut TestDoubleEndsLaterCalls {
            later: LaterReply::HandsOverAReadout,
            failed: false,
        },
        on_shot_0(&[false]),
        on_shot_0(&[true]),
    );
}

/// An executor that cancels a call made as an earlier one fails breaks the
/// rule too: a cancellation says that the work was stopped from outside,
/// so the caller would not learn that the failure lost its run.
#[test]
#[should_panic(
    expected = "a call in flight at an earlier failure must end failed, with that failure, \
                `the device stopped`, not `Cancelled`"
)]
fn ends_calls_in_flight_at_an_earlier_failure_catches_a_reply_that_ends_cancelled() {
    ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<bool>, _>(
        &mut TestDoubleEndsLaterCalls {
            later: LaterReply::Cancelled,
            failed: false,
        },
        on_shot_0(&[false]),
        on_shot_0(&[true]),
    );
}

/// An executor whose device stops at a `false`, that learns of the stop
/// only as the failing call's reply is polled, as one that runs its calls
/// elsewhere learns of their ends from their replies, and runs to its end
/// a call made before then.
struct TestDoubleLearnsOfItsFailureWhenPolled {
    failed: Rc<Cell<bool>>,
}

/// A reply of a [`TestDoubleLearnsOfItsFailureWhenPolled`], which records
/// its call's failure in the executor as it is polled.
struct RecordsItsFailureWhenPolled {
    stops: bool,
    failed: Rc<Cell<bool>>,
}

impl Reply for RecordsItsFailureWhenPolled {
    type Item = ShotOutcome;

    fn poll_events(&mut self, _: &mut Vec<ShotOutcome>, _: &Waker) -> ReplyStatus {
        if self.stops {
            self.failed.set(true);
            ReplyStatus::Failed("the device stopped".into())
        } else {
            ReplyStatus::Done
        }
    }
}

impl Executor<InstructionBatch<bool>> for TestDoubleLearnsOfItsFailureWhenPolled {
    type Reply = RecordsItsFailureWhenPolled;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        if self.failed.get() {
            return Err(Rejected::EarlierFailure("the device stopped".into()));
        }
        Ok(RecordsItsFailureWhenPolled {
            stops: batch.instructions.contains(&false),
            failed: Rc::clone(&self.failed),
        })
    }
}

/// An executor that learns of a failure from the failing call's reply, and
/// runs to its end a call made before that reply is polled, breaks the
/// rule: the call ran on the state the failure left part-way. The check
/// makes the call before it waits for the failing reply, so that the call
/// is in flight as the failure happens; made after, it would be refused,
/// and the fault would not show.
#[test]
#[should_panic(
    expected = "a call in flight at an earlier failure must end failed, with that failure, \
                `the device stopped`, not `Done`"
)]
fn ends_calls_in_flight_at_an_earlier_failure_catches_a_call_in_flight_run_to_its_end() {
    ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<bool>, _>(
        &mut TestDoubleLearnsOfItsFailureWhenPolled {
            failed: Rc::default(),
        },
        on_shot_0(&[false]),
        on_shot_0(&[true]),
    );
}

/// The check needs a failing message that the executor accepts: a refused
/// one starts no work that could fail.
#[test]
#[should_panic(expected = "the failing message must be accepted")]
fn ends_calls_in_flight_at_an_earlier_failure_needs_a_failing_message_accepted() {
    ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<bool>, _>(
        &mut TestDoubleOfOneInstruction::new(OnARefusal::Nothing),
        on_shot_0(&[false, false]),
        on_shot_0(&[true]),
    );
}

/// The least reading so far: its default, 0, is not empty, since folding it
/// in makes every total 0.
#[derive(Clone, Debug, Default, PartialEq)]
struct Least(u64);

impl Fold for Least {
    fn fold(&mut self, other: Self) {
        self.0 = self.0.min(other.0);
    }
}

/// A fold whose default is not empty changes a report it is folded with.
#[test]
#[should_panic(expected = "folding in the default report must change nothing")]
fn default_is_empty_catches_a_default_that_changes_a_total() {
    default_is_empty(&Least(3));
}

/// A report that keeps itself: its fold drops what is folded in.
#[derive(Clone, Debug, Default, PartialEq)]
struct KeepsItself(u64);

impl Fold for KeepsItself {
    fn fold(&mut self, _: Self) {}
}

/// A fold that drops what is folded in loses a part's report when the
/// total starts from the default, as a run's total does.
#[test]
#[should_panic(expected = "folding into the default report must give the report folded in")]
fn default_is_empty_catches_a_fold_that_drops_what_is_folded_in() {
    default_is_empty(&KeepsItself(3));
}

/// The last report folded: its total depends on which part ended last.
#[derive(Clone, Debug, Default, PartialEq)]
struct Last(u64);

impl Fold for Last {
    fn fold(&mut self, other: Self) {
        *self = other;
    }
}

/// A fold whose total depends on the order of the reports breaks the rule
/// that the parts of a run may end in any order.
#[test]
#[should_panic(expected = "the order of two reports must not change their total")]
fn folds_in_any_order_catches_a_total_that_depends_on_the_order() {
    folds_in_any_order(&Last(1), &Last(2), &Last(3));
}

/// The mean of two totals: the order does not matter, the grouping does.
#[derive(Clone, Debug, Default, PartialEq)]
struct Mean(f64);

impl Fold for Mean {
    fn fold(&mut self, other: Self) {
        self.0 = f64::midpoint(self.0, other.0);
    }
}

/// A fold whose total depends on how the reports are grouped breaks the rule
/// too, even when the order of two does not matter: the mean of the mean of
/// 1 and 2, and 4, is 2.75; the mean of 1 and the mean of 2 and 4 is 2.
#[test]
#[should_panic(
    expected = "the order and grouping of three reports must not change their total, but `(a b) \
                c` gives `Mean(2.75)`, and `a (b c)` gives `Mean(2.0)`"
)]
fn folds_in_any_order_catches_a_total_that_depends_on_the_grouping() {
    folds_in_any_order(&Mean(1.0), &Mean(2.0), &Mean(4.0));
}

/// A count of the calls a run made, or the first error of the run.
#[derive(Clone, Debug, Default, PartialEq)]
enum Tally {
    #[default]
    Empty,
    Count(u64),
    Error(String),
}

impl Fold for Tally {
    /// Adds two counts, but keeps its own error and drops an error folded
    /// in: right on every order of `a` and `b`, counts both, but not of a
    /// count and an error.
    fn fold(&mut self, other: Self) {
        *self = match (std::mem::take(self), other) {
            (Self::Empty, tally) | (tally, Self::Empty | Self::Error(_)) => tally,
            (Self::Error(error), _) => Self::Error(error),
            (Self::Count(a), Self::Count(b)) => Self::Count(a + b),
        };
    }
}

/// A fold that keeps its own error and drops one folded in gives a total
/// that depends on whether the error's part ended first: `a` and `b`,
/// two counts, fold alike in either order, `a` and `c`, a count and an
/// error, do not. The check tries every pair, both ways, and names each
/// whose orders differ.
#[test]
#[should_panic(
    expected = "the order of two reports must not change their total, but `b c` gives \
                `Count(2)`, `c b` `Error(\"lost\")`; `a c` gives `Count(1)`, `c a` \
                `Error(\"lost\")`"
)]
fn folds_in_any_order_catches_a_total_that_depends_on_the_order_of_some_pair_only() {
    folds_in_any_order(
        &Tally::Count(1),
        &Tally::Count(2),
        &Tally::Error("lost".into()),
    );
}

/// The first two errors of a run, by name: a report of bounded size.
#[derive(Clone, Debug, Default, PartialEq)]
struct FirstTwo(Vec<&'static str>);

impl Fold for FirstTwo {
    /// Keeps its own errors, then those folded in that it lacks, up to two,
    /// sorted: right on every order of two reports of one error each, and
    /// on both groupings of `a`, `b` and `c` in that order, but not of `a`,
    /// `c` and `b`.
    fn fold(&mut self, other: Self) {
        for error in other.0 {
            if !self.0.contains(&error) {
                self.0.push(error);
            }
        }
        self.0.truncate(2);
        self.0.sort_unstable();
    }
}

/// A report that keeps its first two errors, its own first, gives a total
/// that depends on which part ends last once three parts failed, though
/// every pair folds alike both ways, and `(a b) c` and `a (b c)` agree:
/// the check folds the three in every order and grouping, and names each
/// total that differs from the first.
#[test]
#[should_panic(
    expected = "the order and grouping of three reports must not change their total, but `(a b) \
                c` gives `FirstTwo([\"a\", \"b\"])`, and `(a c) b` gives \
                `FirstTwo([\"a\", \"c\"])`, `(b c) a` gives `FirstTwo([\"b\", \"c\"])`, `(c a) \
                b` gives `FirstTwo([\"a\", \"c\"])`, `c (a b)` gives `FirstTwo([\"a\", \"c\"])`, \
                `(c b) a` gives `FirstTwo([\"b\", \"c\"])`, `c (b a)` gives \
                `FirstTwo([\"a\", \"c\"])`"
)]
fn folds_in_any_order_catches_a_total_that_depends_on_an_order_of_three_only() {
    folds_in_any_order(
        &FirstTwo(vec!["a"]),
        &FirstTwo(vec!["b"]),
        &FirstTwo(vec!["c"]),
    );
}

/// A fold that keeps the last report folded gives the same total in any
/// order and grouping of reports that are all alike, so a caller that
/// gives one report three times would see its fold pass for nothing: the
/// check needs three reports that differ two by two, and names each pair
/// that does not.
#[test]
#[should_panic(
    expected = "`a`, `b` and `c` must differ two by two, so that a total that depends on the \
                order or the grouping would show, but `a` and `b`, `b` and `c`, `a` and `c` are \
                equal"
)]
fn folds_in_any_order_needs_reports_that_differ() {
    folds_in_any_order(&Last(1), &Last(1), &Last(1));
}

/// Two equal reports do not hide a fold that depends on the order when the
/// third differs: the check names the rule the fold breaks, not the need.
#[test]
#[should_panic(expected = "the order of two reports must not change their total")]
fn folds_in_any_order_catches_a_total_that_depends_on_the_order_though_two_reports_are_equal() {
    folds_in_any_order(&Last(1), &Last(2), &Last(1));
}

/// How a [`TestDoubleWithAFaultyCheck`] checks a batch against its set.
enum Fault {
    /// It checks nothing, and runs the batch.
    NoCheck,
    /// It rejects a batch that holds a call outside at its first call,
    /// whichever is outside.
    WrongIndex,
    /// It rejects a batch that holds a call outside at its last call,
    /// whichever is outside.
    AtLast,
    /// It rejects a batch at its last call outside, searching from the end.
    AtTheLastCallOutside,
    /// It checks the batch's opcodes in one pass, then the operands in
    /// another, so that it refuses at a call whose opcode the set lacks
    /// before an earlier call whose operands are wrong.
    TwoPasses,
    /// It checks the batch's first and last calls only.
    FirstAndLastOnly,
    /// It checks every call but the first, which it takes for a header.
    SkipsTheFirstCall,
    /// It rejects a batch that holds a call outside as unavailable.
    OtherRejection,
    /// It checks each call's opcode, not its operands.
    OpcodesOnly,
    /// It checks each call's operands, not its opcode, against the kinds
    /// its opcode takes, or for an opcode the set lacks, those of the
    /// set's first instruction, as a dispatch table that falls back to its
    /// first entry.
    OperandsOnly,
    /// It checks each call's opcode and how many operands it has, not their
    /// kinds.
    CountsOperandsOnly,
    /// It checks the kinds of a call's operands one by one against those
    /// its opcode takes, as far as both go (a `zip`), so that it takes a
    /// call with operands left out.
    AcceptsTooFew,
    /// It checks the count of a call's operands, and the kind of its first
    /// only.
    FirstKindOnly,
    /// It checks the count of a call's operands, and the kinds of all but
    /// its last (a loop that stops one short).
    AllButTheLastKind,
    /// It checks the count of a call's operands, and of each whether it is
    /// a block or a number, not which number.
    BlockOrNumber,
    /// It checks the count and the kinds of a call's operands, but takes a
    /// tag for an integer, and an integer for a tag (both whole numbers).
    TagsAsIntegers,
    /// It refuses every batch of more than one call at its second call,
    /// whatever the batch holds.
    RefusesAllButOne,
    /// It refuses every batch of more than two calls at its third call, as
    /// a device of two instruction slots, before it checks the calls.
    AtMostTwoCalls,
}

/// An executor of a set whose check of calls against it is faulty.
struct TestDoubleWithAFaultyCheck {
    fault: Fault,
    set: InstructionSet,
}

impl TestDoubleWithAFaultyCheck {
    /// Whether it refuses `call` when its check compares two kinds with
    /// `alike` (`call`'s operand at its index, and the kind the set
    /// declares there), and the count of `call`'s operands with `count`
    /// (`call`'s, and the set's).
    fn refuses(
        &self,
        call: &InstructionCall,
        count: impl Fn(usize, usize) -> bool,
        alike: impl Fn(usize, OperandKind, OperandKind) -> bool,
    ) -> bool {
        let declared =
            (self.set.mnemonic(call.opcode)).and_then(|mnemonic| self.set.operands(mnemonic));
        let Some(declared) = declared else {
            return true;
        };
        !count(call.operands.len(), declared.len())
            || (call.operands.iter().zip(declared).enumerate())
                .any(|(at, (operand, &kind))| !alike(at, operand.kind(), kind))
    }
}

impl Executor<InstructionBatch<InstructionCall>> for TestDoubleWithAFaultyCheck {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        let batch = batch.instructions;
        let outside = |call: &InstructionCall| self.set.check(call).is_err();
        let same_count = |given: usize, declared: usize| given == declared;
        let last = batch.len() - 1;
        let at = match self.fault {
            Fault::NoCheck => None,
            Fault::WrongIndex => batch.iter().position(outside).map(|_| 0),
            Fault::AtLast => batch.iter().position(outside).map(|_| last),
            Fault::AtTheLastCallOutside => batch.iter().rposition(outside),
            Fault::TwoPasses => (batch.iter())
                .position(|call| self.set.mnemonic(call.opcode).is_none())
                .or_else(|| batch.iter().position(outside)),
            Fault::FirstAndLastOnly => [0, last].into_iter().find(|&at| outside(&batch[at])),
            Fault::SkipsTheFirstCall => (batch.iter().skip(1).position(outside)).map(|at| at + 1),
            Fault::OtherRejection => {
                if batch.iter().any(outside) {
                    return Err(Rejected::Unavailable("the device is busy".into()));
                }
                None
            }
            Fault::OpcodesOnly => {
                (batch.iter()).position(|call| self.set.mnemonic(call.opcode).is_none())
            }
            Fault::OperandsOnly => batch.iter().position(|call| {
                let first = self
                    .set
                    .mnemonics()
                    .next()
                    .expect("a set of one instruction");
                let declared = (self.set.mnemonic(call.opcode))
                    .and_then(|mnemonic| self.set.operands(mnemonic))
                    .or_else(|| self.set.operands(first))
                    .expect("an instruction's operands");
                (call.operands.iter().map(Operand::kind)).ne(declared.iter().copied())
            }),
            Fault::CountsOperandsOnly => {
                (batch.iter()).position(|call| self.refuses(call, same_count, |_, _, _| true))
            }
            Fault::AcceptsTooFew => batch.iter().position(|call| {
                self.refuses(
                    call,
                    |given, declared| given <= declared,
                    |_, given, kind| given == kind,
                )
            }),
            Fault::FirstKindOnly => batch.iter().position(|call| {
                self.refuses(call, same_count, |at, given, kind| at > 0 || given == kind)
            }),
            Fault::AllButTheLastKind => batch.iter().position(|call| {
                let last = call.operands.len().saturating_sub(1);
                self.refuses(call, same_count, |at, given, kind| {
                    at == last || given == kind
                })
            }),
            Fault::BlockOrNumber => batch.iter().position(|call| {
                self.refuses(call, same_count, |_, given, kind| {
                    (given == OperandKind::Block) == (kind == OperandKind::Block)
                })
            }),
            Fault::TagsAsIntegers => batch.iter().position(|call| {
                let whole = |kind| match kind {
                    OperandKind::Tag => OperandKind::Integer,
                    kind => kind,
                };
                self.refuses(call, same_count, |_, given, kind| {
                    whole(given) == whole(kind)
                })
            }),
            Fault::RefusesAllButOne => (batch.len() > 1).then_some(1),
            Fault::AtMostTwoCalls => (batch.len() > 2)
                .then_some(2)
                .or_else(|| batch.iter().position(outside)),
        };
        match at {
            None => Ok(ImmediateReply::done(Vec::new())),
            Some(at) => Err(Rejected::Instruction {
                at,
                reason: "not in the set".into(),
            }),
        }
    }
}

/// A set of one instruction, `x`, which takes a block, and its call.
fn x_only() -> (InstructionSet, InstructionCall) {
    let set = InstructionSet::new("x only").with("x", [OperandKind::Block]);
    let x = set
        .call("x", [Operand::Block(0)])
        .expect("`x` takes a block");
    (set, x)
}

/// A set of one instruction, `g`, which takes an operand of each kind, a
/// block, a real, an integer and a tag, in that order, and its call.
fn one_of_each_kind() -> (InstructionSet, InstructionCall) {
    let set = InstructionSet::new("one of each kind").with(
        "g",
        [
            OperandKind::Block,
            OperandKind::Real,
            OperandKind::Integer,
            OperandKind::Tag,
        ],
    );
    let g = set
        .call(
            "g",
            [
                Operand::Block(1),
                Operand::Real(0.5),
                Operand::Integer(1),
                Operand::Tag(1),
            ],
        )
        .expect("`g` takes a block, a real, an integer and a tag");
    (set, g)
}

/// Runs the check on a [`TestDoubleWithAFaultyCheck`] of `set`, with
/// `accepted` its call, on shot 0.
fn check_set(fault: Fault, (set, accepted): (InstructionSet, InstructionCall)) {
    refuses_calls_outside_its_set(
        &mut TestDoubleWithAFaultyCheck {
            fault,
            set: set.clone(),
        },
        &ShotSet::one(0),
        &set,
        &accepted,
    );
}

/// An executor that runs a call outside the set it describes breaks the
/// rule of a described set.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected with `Instruction`, but it was accepted"
)]
fn refuses_calls_outside_its_set_catches_a_call_run() {
    check_set(Fault::NoCheck, x_only());
}

/// An executor that refuses the batch at another index breaks the rule too:
/// the caller would change a call that was fine.
#[test]
#[should_panic(expected = "a call outside the set must be rejected at its index, 1, not 0")]
fn refuses_calls_outside_its_set_catches_a_refusal_at_another_index() {
    check_set(Fault::WrongIndex, x_only());
}

/// An executor that refuses the batch for another reason breaks the rule:
/// the caller would wait and send it again, and it would never run.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected with `Instruction`, not `Unavailable"
)]
fn refuses_calls_outside_its_set_catches_another_rejection() {
    check_set(Fault::OtherRejection, x_only());
}

/// An executor that checks a call's opcode and not its operands breaks the
/// rule: it runs `x` with an operand `x` does not take.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected with `Instruction`, but it was accepted"
)]
fn refuses_calls_outside_its_set_catches_a_check_of_opcodes_only() {
    check_set(Fault::OpcodesOnly, x_only());
}

/// An executor that checks a call's operands and not its opcode breaks the
/// rule: it runs a call at an opcode its set lacks as one it has.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected with `Instruction`, but it was accepted"
)]
fn refuses_calls_outside_its_set_catches_a_check_of_operands_only() {
    check_set(Fault::OperandsOnly, x_only());
}

/// The check needs a call of the set: a batch it begins with a call
/// outside would be refused at 0, whatever the executor checks after.
#[test]
#[should_panic(expected = "`accepted` must be a call of the set")]
fn refuses_calls_outside_its_set_needs_a_call_of_the_set() {
    let (set, x) = x_only();
    let outside = InstructionCall::new(x.opcode, [Operand::Block(0), Operand::Block(1)]);
    refuses_calls_outside_its_set(
        &mut TestDoubleWithAFaultyCheck {
            fault: Fault::NoCheck,
            set: set.clone(),
        },
        &ShotSet::one(0),
        &set,
        &outside,
    );
}

/// An executor that refuses a batch at its last call, whichever is
/// outside, is right whenever the call outside comes last: a call outside
/// in the middle of a batch shows it.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected at its index, 1, not 2 (`[accepted, \
                outside, accepted]`, the call outside at an opcode the set lacks)"
)]
fn refuses_calls_outside_its_set_catches_a_refusal_at_the_last_call() {
    check_set(Fault::AtLast, x_only());
}

/// An executor that searches a batch for a call outside from its end is
/// right whenever the batch holds one: two show it, since the caller would
/// learn of the second first, and change it, to be refused again.
#[test]
#[should_panic(
    expected = "the first of two calls outside the set must be rejected at its index, 0, not 2 \
                (`[outside, accepted, outside]`, the first call outside at an opcode the set \
                lacks, the second at an opcode the set lacks)"
)]
fn refuses_calls_outside_its_set_catches_a_refusal_at_the_last_call_outside() {
    check_set(Fault::AtTheLastCallOutside, x_only());
}

/// An executor that checks a batch's opcodes in one pass, then its
/// operands in another, refuses each call outside alone at its index, and
/// two calls outside for one reason at the first: a call with an operand
/// too many before one at an opcode the set lacks shows it, since it
/// refuses the later call.
#[test]
#[should_panic(
    expected = "the first of two calls outside the set must be rejected at its index, 0, not 2 \
                (`[outside, accepted, outside]`, the first call outside with an operand too \
                many, the second at an opcode the set lacks)"
)]
fn refuses_calls_outside_its_set_catches_a_check_in_two_passes() {
    check_set(Fault::TwoPasses, one_of_each_kind());
}

/// An executor that checks a batch's first and last calls only runs a call
/// outside in the middle of the batch.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected with `Instruction`, but it was accepted \
                (`[accepted, outside, accepted]`, the call outside at an opcode the set lacks)"
)]
fn refuses_calls_outside_its_set_catches_a_check_of_the_first_and_last_calls_only() {
    check_set(Fault::FirstAndLastOnly, x_only());
}

/// An executor that takes a batch's first call for a header, and checks the
/// others only, runs a call outside that comes first.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected with `Instruction`, but it was accepted \
                (`[outside, accepted]`, the call outside at an opcode the set lacks)"
)]
fn refuses_calls_outside_its_set_catches_a_check_that_skips_the_first_call() {
    check_set(Fault::SkipsTheFirstCall, x_only());
}

/// An executor that counts a call's operands and not their kinds runs `x`
/// on a real where it takes a block.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected with `Instruction`, but it was accepted \
                (`[accepted, outside]`, the call outside with its operand 0 of kind `Real`, not \
                `Block`)"
)]
fn refuses_calls_outside_its_set_catches_a_check_of_operand_counts_only() {
    check_set(Fault::CountsOperandsOnly, x_only());
}

/// An executor that compares a call's operands with those its opcode takes
/// as far as both go runs `x` with its block left out.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected with `Instruction`, but it was accepted \
                (`[accepted, outside]`, the call outside with an operand too few)"
)]
fn refuses_calls_outside_its_set_catches_a_call_run_with_an_operand_too_few() {
    check_set(Fault::AcceptsTooFew, x_only());
}

/// An executor that checks the kind of a call's first operand only runs
/// `g` on a block where it takes a real, at its second operand.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected with `Instruction`, but it was accepted \
                (`[accepted, outside]`, the call outside with its operand 1 of kind `Block`, not \
                `Real`)"
)]
fn refuses_calls_outside_its_set_catches_a_check_of_the_first_operands_kind_only() {
    check_set(Fault::FirstKindOnly, one_of_each_kind());
}

/// An executor whose loop over a call's operands stops one short runs `g`
/// on a block where it takes a tag, at its last operand.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected with `Instruction`, but it was accepted \
                (`[accepted, outside]`, the call outside with its operand 3 of kind `Block`, not \
                `Tag`)"
)]
fn refuses_calls_outside_its_set_catches_a_check_of_all_but_the_last_operands_kind() {
    check_set(Fault::AllButTheLastKind, one_of_each_kind());
}

/// An executor that tells a block from a number, and not one number from
/// another, runs `g` on an integer where it takes a real.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected with `Instruction`, but it was accepted \
                (`[accepted, outside]`, the call outside with its operand 1 of kind `Integer`, not \
                `Real`)"
)]
fn refuses_calls_outside_its_set_catches_a_check_of_blocks_against_numbers_only() {
    check_set(Fault::BlockOrNumber, one_of_each_kind());
}

/// An executor that takes a tag for an integer runs `g` on a tag where it
/// takes an integer, which it would read as a readout's tag.
#[test]
#[should_panic(
    expected = "a call outside the set must be rejected with `Instruction`, but it was accepted \
                (`[accepted, outside]`, the call outside with its operand 2 of kind `Tag`, not \
                `Integer`)"
)]
fn refuses_calls_outside_its_set_catches_a_tag_taken_for_an_integer() {
    check_set(Fault::TagsAsIntegers, one_of_each_kind());
}

/// An executor that refuses every batch of two calls or more at its second
/// call refuses each batch with a call outside where the rule asks, though
/// it runs nothing of its own set either: the check needs `accepted` run,
/// three times in one batch, before it can blame a refusal on the call
/// outside.
#[test]
#[should_panic(
    expected = "`accepted` must be a call that the executor runs, three times in one batch, so \
                that a refusal of a batch that holds it can only be the call outside's, but \
                `[accepted, accepted, accepted]` was rejected with `Instruction { at: 1"
)]
fn refuses_calls_outside_its_set_needs_accepted_run_three_times_in_one_batch() {
    check_set(Fault::RefusesAllButOne, x_only());
}

/// A device of two instruction slots refuses every batch of three calls at
/// the third, a limit of its own, which the check would blame on the call
/// outside in the middle of a batch of three: it needs `accepted` run three
/// times in one batch, as many calls as its largest batch holds.
#[test]
#[should_panic(
    expected = "`accepted` must be a call that the executor runs, three times in one batch, so \
                that a refusal of a batch that holds it can only be the call outside's, but \
                `[accepted, accepted, accepted]` was rejected with `Instruction { at: 2"
)]
fn refuses_calls_outside_its_set_needs_accepted_run_in_a_batch_as_large_as_its_largest() {
    check_set(Fault::AtMostTwoCalls, x_only());
}

/// A sound executor of a set, which records each batch that it refuses,
/// with the index it refuses it at.
struct TestDoubleThatRecordsItsRefusals {
    set: InstructionSet,
    refused: Vec<(Vec<InstructionCall>, usize)>,
}

impl Executor<InstructionBatch<InstructionCall>> for TestDoubleThatRecordsItsRefusals {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        let batch = batch.instructions;
        if let Err(rejected) = self.set.check_batch(batch) {
            if let Rejected::Instruction { at, .. } = rejected {
                self.refused.push((batch.to_vec(), at));
            }
            return Err(rejected);
        }
        Ok(ImmediateReply::done(Vec::new()))
    }
}

/// Why [`InstructionSet::check`] refuses a call.
#[derive(Clone, Copy, Debug)]
enum Outside {
    /// The set lacks its opcode.
    Opcode,
    /// It has more operands than its instruction takes.
    TooMany,
    /// It has fewer operands than its instruction takes.
    TooFew,
    /// Its operand at `at` is of `kind`, which its instruction does not
    /// take there; the others are of the kinds it takes.
    Kind { at: usize, kind: OperandKind },
}

/// Why `set` refuses `call`, which it refuses, by its own answer: a
/// variant that the error of [`InstructionSet::check`] gains fails to
/// compile here, until it is told apart. A new reason under a variant
/// already told apart does not.
fn why_outside(set: &InstructionSet, call: &InstructionCall) -> Outside {
    match set.check(call) {
        Ok(()) => panic!("a call refused for being outside the set must be outside it"),
        Err(InstructionMismatch::UnknownOpcode { .. }) => Outside::Opcode,
        Err(InstructionMismatch::DifferentOperands { mnemonic }) => {
            let declared = set.operands(&mnemonic).expect("the call's instruction");
            let given: Vec<OperandKind> = call.operands.iter().map(Operand::kind).collect();
            match given.len().cmp(&declared.len()) {
                Ordering::Greater => Outside::TooMany,
                Ordering::Less => Outside::TooFew,
                Ordering::Equal => {
                    let differ: Vec<usize> = (0..given.len())
                        .filter(|&at| given[at] != declared[at])
                        .collect();
                    let [at] = differ[..] else {
                        panic!("each call outside differs at one operand, not at {differ:?}")
                    };
                    Outside::Kind {
                        at,
                        kind: given[at],
                    }
                }
            }
        }
        Err(
            InstructionMismatch::Missing { .. }
            | InstructionMismatch::DifferentAction { .. }
            | InstructionMismatch::UndeclaredAction { .. },
        ) => unreachable!("`check` refuses a call at an opcode, not by mnemonic or action"),
    }
}

/// Every kind of operand: a kind added to [`OperandKind`] fails to compile
/// in the match here, until it is matched. Listing it in `every` too is up
/// to whoever adds it: no test checks it.
fn every_operand_kind() -> Vec<OperandKind> {
    let every = [
        OperandKind::Block,
        OperandKind::Real,
        OperandKind::Integer,
        OperandKind::Tag,
    ];
    for kind in every {
        match kind {
            OperandKind::Block | OperandKind::Real | OperandKind::Integer | OperandKind::Tag => {}
        }
    }
    every.to_vec()
}

/// The check tries each reason the set's own check has to refuse a call,
/// at each place in a batch: last, in the middle and first; and every
/// ordered pair of reasons in one batch, one reason twice included,
/// refused at the first. The reasons are those of
/// [`InstructionSet::check`]: an opcode the set lacks, an operand too many,
/// one too few, and at each operand each other kind. A reason that `check`
/// gains under a new variant of its error fails to compile in
/// [`why_outside`]; once told apart there, and listed in `reasons`, this
/// test fails until the check tries it. A new reason under a variant
/// already told apart (a check of an operand's value, say) needs a new
/// call outside in the check, which no test asks for.
#[test]
fn refuses_calls_outside_its_set_tries_each_reason_a_set_has_to_refuse_a_call_at_each_place() {
    let (set, g) = one_of_each_kind();
    let mut executor = TestDoubleThatRecordsItsRefusals {
        set: set.clone(),
        refused: Vec::new(),
    };
    refuses_calls_outside_its_set(&mut executor, &ShotSet::one(0), &set, &g);
    let place = |outside: &[(usize, Outside)], at: usize, len: usize| {
        let reasons: Vec<String> = (outside.iter())
            .map(|&(index, reason)| match reason {
                Outside::Kind { at, kind } => format!("operand {at} of kind {kind:?} at {index}"),
                other => format!("{other:?} at {index}"),
            })
            .collect();
        format!("{} of {len}, refused at {at}", reasons.join(" and "))
    };
    let tried: BTreeSet<String> = (executor.refused.iter())
        .map(|(batch, at)| {
            let outside: Vec<(usize, Outside)> = (batch.iter().enumerate())
                .filter(|(_, call)| set.check(call).is_err())
                .map(|(index, call)| (index, why_outside(&set, call)))
                .collect();
            place(&outside, *at, batch.len())
        })
        .collect();
    let declared = set.operands("g").expect("`g` is in its set");
    let mut reasons = vec![Outside::Opcode, Outside::TooMany, Outside::TooFew];
    for (at, &taken) in declared.iter().enumerate() {
        for kind in every_operand_kind() {
            if kind != taken {
                reasons.push(Outside::Kind { at, kind });
            }
        }
    }
    let alone = (reasons.iter()).flat_map(|&reason| {
        [(1, 2), (1, 3), (0, 2)].map(|(at, len)| place(&[(at, reason)], at, len))
    });
    let pairs = (reasons.iter()).flat_map(|&first| {
        (reasons.iter()).map(move |&second| place(&[(0, first), (2, second)], 0, 3))
    });
    let expected: BTreeSet<String> = alone.chain(pairs).collect();
    assert_eq!(
        tried, expected,
        "the check must try each reason to refuse a call, at each place, and each pair"
    );
}

/// How a [`TestDoubleWithAFaultyTagCheck`] checks the tags of a batch.
enum TagFault {
    /// It checks nothing, and runs the batch.
    NoCheck,
    /// It compares each readout's tag with the one before it only.
    NeighboursOnly,
    /// It refuses the batch at the first use of the repeated tag.
    FirstUse,
    /// It refuses the batch as unavailable.
    OtherRejection,
    /// It keeps the tags of every batch, so that a later batch cannot use
    /// one again.
    AcrossBatches(BTreeSet<u32>),
    /// It refuses every batch of more than one readout.
    OneReadoutOnly,
    /// It compares each readout's tag with the batch's first tag only.
    FirstOnly,
    /// It refuses a batch that repeats a tag at its last readout, wherever
    /// the repeat is.
    AtLast,
    /// It refuses every batch of more than two readouts at its third, as an
    /// executor of two readout registers that takes a third readout for a
    /// repeat.
    AtMostTwo,
    /// It looks for repeats tag by tag, in increasing order, and refuses
    /// the batch at the repeat of the first tag it finds repeated.
    InTagOrder,
    /// It refuses the batch at the last readout that repeats a tag before
    /// it.
    AtTheLastRepeat,
    /// It refuses a batch that reads a qubit twice, at the second readout,
    /// not one that repeats a tag: right whenever each readout of a batch
    /// reads under its qubit's own tag.
    QubitsNotTags,
    /// It refuses a batch that reads a qubit twice, or whose readout
    /// repeats the tag of the readout just before, at the second readout:
    /// right whenever another instruction that repeats a tag comes just
    /// after the first.
    QubitsAndNeighbouringTags,
}

/// An instruction of a [`TestDoubleWithAFaultyTagCheck`]: a readout of a
/// qubit, under a tag.
#[derive(Clone, Copy, PartialEq)]
struct Readout {
    qubit: u32,
    tag: u32,
}

/// An executor whose instructions are each a readout of a qubit under the
/// tag they carry, of zero on each shot, and whose check of their tags is
/// faulty.
struct TestDoubleWithAFaultyTagCheck(TagFault);

impl Executor<InstructionBatch<Readout>> for TestDoubleWithAFaultyTagCheck {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, Readout>) -> Result<Self::Reply, Rejected> {
        let (shots, readouts) = (batch.shots, batch.instructions);
        let batch: Vec<u32> = readouts.iter().map(|read| read.tag).collect();
        let refuse = |at: usize| {
            Err(Rejected::Instruction {
                at,
                reason: "a repeated tag".into(),
            })
        };
        match &mut self.0 {
            TagFault::NoCheck => {}
            TagFault::NeighboursOnly => {
                if let Some(at) = batch.windows(2).position(|pair| pair[0] == pair[1]) {
                    return refuse(at + 1);
                }
            }
            TagFault::FirstUse => {
                if let Some(at) = (0..batch.len()).find(|&at| batch[at + 1..].contains(&batch[at]))
                {
                    return refuse(at);
                }
            }
            TagFault::OtherRejection => {
                if check_unique_tags(batch.iter().copied().enumerate()).is_err() {
                    return Err(Rejected::Unavailable("the device is busy".into()));
                }
            }
            TagFault::AcrossBatches(seen) => {
                if let Some(at) = batch.iter().position(|&tag| !seen.insert(tag)) {
                    return refuse(at);
                }
            }
            TagFault::OneReadoutOnly => {
                if batch.len() > 1 {
                    return refuse(1);
                }
            }
            TagFault::FirstOnly => {
                if let Some(at) = (batch.iter().skip(1)).position(|tag| Some(tag) == batch.first())
                {
                    return refuse(at + 1);
                }
            }
            TagFault::AtLast => {
                if check_unique_tags(batch.iter().copied().enumerate()).is_err() {
                    return refuse(batch.len() - 1);
                }
            }
            TagFault::AtMostTwo => {
                if batch.len() > 2 {
                    return refuse(2);
                }
            }
            TagFault::InTagOrder => {
                let ordered: BTreeSet<u32> = batch.iter().copied().collect();
                for tag in ordered {
                    let mut uses = (0..batch.len()).filter(|&at| batch[at] == tag);
                    if let Some(at) = uses.nth(1) {
                        return refuse(at);
                    }
                }
            }
            TagFault::AtTheLastRepeat => {
                if let Some(at) = (0..batch.len())
                    .rev()
                    .find(|&at| batch[..at].contains(&batch[at]))
                {
                    return refuse(at);
                }
            }
            TagFault::QubitsNotTags => {
                check_unique_tags(readouts.iter().map(|read| read.qubit).enumerate())?;
            }
            TagFault::QubitsAndNeighbouringTags => {
                let repeats = |at: usize| {
                    let read = readouts[at];
                    (readouts[..at].iter()).any(|before| before.qubit == read.qubit)
                        || (at > 0 && readouts[at - 1].tag == read.tag)
                };
                if let Some(at) = (0..readouts.len()).find(|&at| repeats(at)) {
                    return refuse(at);
                }
            }
        }
        let outcomes = (shots.iter())
            .flat_map(|&shot| {
                batch
                    .iter()
                    .map(move |&tag| ShotOutcome::new(shot, tag, false.into()))
            })
            .collect();
        Ok(ImmediateReply::done(outcomes))
    }
}

/// Runs the check on a [`TestDoubleWithAFaultyTagCheck`], with `a` and `b`
/// the readouts of qubits 0 and 1 under tags 0 and 1, and `a_again` that of
/// qubit 2 under tag 0.
fn check_tags(fault: TagFault) {
    let read = |qubit, tag| Readout { qubit, tag };
    refuses_a_tag_repeated_within_a_batch(
        &mut TestDoubleWithAFaultyTagCheck(fault),
        &ShotSet::one(0),
        &read(0, 0),
        &read(1, 1),
        &read(2, 0),
    );
}

/// An executor that runs a batch repeating a tag breaks the rule: two
/// outcomes would come under one tag, and the caller could not tell them
/// apart.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch must be rejected with `Instruction`, but it was accepted"
)]
fn refuses_a_tag_repeated_within_a_batch_catches_a_repeat_run() {
    check_tags(TagFault::NoCheck);
}

/// An executor that compares neighbours only misses a repeat after another
/// readout.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch must be rejected with `Instruction`, but it was \
                accepted (`[a, b, a]`)"
)]
fn refuses_a_tag_repeated_within_a_batch_catches_a_check_of_neighbours_only() {
    check_tags(TagFault::NeighboursOnly);
}

/// An executor that compares each tag with the batch's first only misses a
/// repeat of a later tag: a batch that reads `b`, then `a` twice, would
/// hand over two outcomes of a shot under `a`'s tag.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch must be rejected with `Instruction`, but it was \
                accepted (`[b, a, a]`)"
)]
fn refuses_a_tag_repeated_within_a_batch_catches_a_check_against_the_first_tag_only() {
    check_tags(TagFault::FirstOnly);
}

/// An executor that refuses the batch at the tag's first use breaks the
/// rule: the caller would change a readout that was fine.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch must be rejected at its index, 2, not 0 (`[a, b, a]`)"
)]
fn refuses_a_tag_repeated_within_a_batch_catches_a_refusal_at_the_first_use() {
    check_tags(TagFault::FirstUse);
}

/// An executor that refuses a batch at its last readout, wherever the
/// repeat is, is right whenever the repeat comes last: a repeat in the
/// middle of the batch shows it.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch must be rejected at its index, 1, not 2 (`[a, a, b]`)"
)]
fn refuses_a_tag_repeated_within_a_batch_catches_a_refusal_at_the_last_readout() {
    check_tags(TagFault::AtLast);
}

/// An executor that refuses every batch of three readouts at the third is
/// right whenever the repeat is the third readout: a repeat in the middle
/// of the batch shows it.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch must be rejected at its index, 1, not 2 (`[a, a, b]`)"
)]
fn refuses_a_tag_repeated_within_a_batch_catches_a_refusal_of_every_third_readout() {
    check_tags(TagFault::AtMostTwo);
}

/// An executor that looks for repeats tag by tag finds the repeat of tag 0
/// first, though tag 1's comes before it in the batch: the caller is told
/// of a readout that a later batch would still refuse.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch must be rejected at its index, 2, not 3 \
                (`[a, b, b, a]`)"
)]
fn refuses_a_tag_repeated_within_a_batch_catches_a_search_for_repeats_in_the_order_of_tags() {
    check_tags(TagFault::InTagOrder);
}

/// An executor that refuses the batch at its last repeat is right whenever
/// the batch repeats one tag: a batch that repeats two shows it.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch must be rejected at its index, 2, not 3 \
                (`[a, b, b, a]`)"
)]
fn refuses_a_tag_repeated_within_a_batch_catches_a_refusal_at_the_last_repeat() {
    check_tags(TagFault::AtTheLastRepeat);
}

/// With `a` under tag 1 and `b` under tag 0, an executor that looks for
/// repeats tag by tag finds `b`'s first: the batch that repeats `a` first
/// shows it.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch must be rejected at its index, 2, not 3 \
                (`[b, a, a, b]`)"
)]
fn refuses_a_tag_repeated_within_a_batch_catches_a_search_in_the_order_of_tags_whichever_is_less() {
    let read = |qubit, tag| Readout { qubit, tag };
    refuses_a_tag_repeated_within_a_batch(
        &mut TestDoubleWithAFaultyTagCheck(TagFault::InTagOrder),
        &ShotSet::one(0),
        &read(0, 1),
        &read(1, 0),
        &read(2, 1),
    );
}

/// An executor that refuses a qubit read twice in a batch, rather than a
/// tag, is right whenever a batch repeats an instruction: another
/// instruction under `a`'s tag, a readout of another qubit, shows it,
/// since the caller would get two outcomes it cannot tell apart.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch must be rejected with `Instruction`, but it was \
                accepted (`[a, a_again]`)"
)]
fn refuses_a_tag_repeated_within_a_batch_catches_a_check_of_qubits_not_tags() {
    check_tags(TagFault::QubitsNotTags);
}

/// An executor that refuses a qubit read twice, and a tag repeated by the
/// readout just after, is right whenever a repeat by another instruction
/// comes next to the first: one after another readout shows it.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch must be rejected with `Instruction`, but it was \
                accepted (`[a, b, a_again]`)"
)]
fn refuses_a_tag_repeated_within_a_batch_catches_a_check_of_qubits_and_of_neighbouring_tags() {
    check_tags(TagFault::QubitsAndNeighbouringTags);
}

/// `a_again` that is `a` makes `[a, a_again]` a repeated instruction, so
/// an executor that refuses a repeated qubit, not a repeated tag, would
/// pass: the check needs another instruction than `a`.
#[test]
#[should_panic(
    expected = "`a_again` must be another instruction than `a`, so that a batch that holds both \
                repeats a tag, but no instruction"
)]
fn refuses_a_tag_repeated_within_a_batch_needs_a_again_to_be_another_instruction() {
    let a = Readout { qubit: 0, tag: 0 };
    refuses_a_tag_repeated_within_a_batch(
        &mut TestDoubleWithAFaultyTagCheck(TagFault::QubitsNotTags),
        &ShotSet::one(0),
        &a,
        &Readout { qubit: 1, tag: 1 },
        &a,
    );
}

/// An `a_again` that reads under `b`'s tag makes `[a, a_again]` repeat no
/// tag, so a sound executor that runs it would be blamed: the check needs
/// `a_again` to read under `a`'s tag, and names every shot where it does
/// not.
#[test]
#[should_panic(
    expected = "`a_again` must make one readout on each shot, under `a`'s tag, so that \
                `[a, a_again]` repeats a tag, but `[a_again]` does not, on shots 0, 1"
)]
fn refuses_a_tag_repeated_within_a_batch_needs_a_again_to_read_under_the_tag_of_a() {
    refuses_a_tag_repeated_within_a_batch(
        &mut TestDoubleOfBits::new(BitsBehaviour::Sound),
        &shots_0_and_1(),
        &BitStep::Read(0),
        &BitStep::Read(1),
        &BitStep::ReadInverted(1),
    );
}

/// An `a_again` that reads only on odd shots makes `[a, a_again]` repeat
/// no tag on shots 2 and 4: the check names them.
#[test]
#[should_panic(
    expected = "`a_again` must make one readout on each shot, under `a`'s tag, so that \
                `[a, a_again]` repeats a tag, but `[a_again]` does not, on shots 2, 4"
)]
fn refuses_a_tag_repeated_within_a_batch_needs_a_readout_of_a_again_on_each_shot() {
    refuses_a_tag_repeated_within_a_batch(
        &mut TestDoubleOfBits::new(BitsBehaviour::Sound),
        &ShotSet::range(1..=4).expect("four shots"),
        &BitStep::Read(0),
        &BitStep::Read(1),
        &BitStep::ReadIfOdd(0),
    );
}

/// An executor that refuses the batch for another reason breaks the rule:
/// the caller would wait and send it again, and it would never run.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch must be rejected with `Instruction`, not `Unavailable"
)]
fn refuses_a_tag_repeated_within_a_batch_catches_another_rejection() {
    check_tags(TagFault::OtherRejection);
}

/// An executor that keeps tags from one batch to the next breaks the other
/// side of the rule: a program measuring in a loop names its readout the
/// same in each batch.
#[test]
#[should_panic(expected = "a tag used again in a later batch must be accepted")]
fn refuses_a_tag_repeated_within_a_batch_catches_a_tag_refused_in_a_later_batch() {
    check_tags(TagFault::AcrossBatches(BTreeSet::new()));
}

/// An executor that refuses any batch of two readouts refuses more than
/// the rule asks.
#[test]
#[should_panic(expected = "two readouts under different tags must be accepted")]
fn refuses_a_tag_repeated_within_a_batch_catches_distinct_tags_refused() {
    check_tags(TagFault::OneReadoutOnly);
}

/// An instruction that reads only on odd shots makes no readout on shots 2
/// and 4, where a batch that holds it twice repeats no tag, so an executor
/// that runs the batch would be blamed for it: the check needs `a` and `b`
/// to make one readout on each shot, and names every shot where one does
/// not.
#[test]
#[should_panic(
    expected = "`a` and `b` must each make one readout on each shot, so that a batch that holds \
                one twice repeats a tag, but `[a]` does not, on shots 2, 4"
)]
fn refuses_a_tag_repeated_within_a_batch_needs_a_readout_of_a_on_each_shot() {
    refuses_a_tag_repeated_within_a_batch(
        &mut TestDoubleOfBits::new(BitsBehaviour::Sound),
        &ShotSet::range(1..=4).expect("four shots"),
        &BitStep::ReadIfOdd(0),
        &BitStep::Read(1),
        &BitStep::ReadInverted(0),
    );
}

/// As above, for `b`.
#[test]
#[should_panic(
    expected = "`a` and `b` must each make one readout on each shot, so that a batch that holds \
                one twice repeats a tag, but `[b]` does not, on shots 2, 4"
)]
fn refuses_a_tag_repeated_within_a_batch_needs_a_readout_of_b_on_each_shot() {
    refuses_a_tag_repeated_within_a_batch(
        &mut TestDoubleOfBits::new(BitsBehaviour::Sound),
        &ShotSet::range(1..=4).expect("four shots"),
        &BitStep::Read(0),
        &BitStep::ReadIfOdd(1),
        &BitStep::ReadInverted(0),
    );
}

/// Two readouts under one tag make `[a, b]` a repeat, which a sound
/// executor refuses, and would be blamed for: the check needs `a` and `b`
/// under different tags, and says so before it calls `[a, b]`.
#[test]
#[should_panic(
    expected = "`a` and `b` must read under different tags, so that `[a, b]` repeats none, but \
                they read under one tag on shots 0, 1"
)]
fn refuses_a_tag_repeated_within_a_batch_needs_a_and_b_under_different_tags() {
    refuses_a_tag_repeated_within_a_batch(
        &mut TestDoubleOfBits::new(BitsBehaviour::Sound),
        &shots_0_and_1(),
        &BitStep::Read(0),
        &BitStep::Read(0),
        &BitStep::ReadInverted(0),
    );
}

/// An executor that refuses a repeated tag at the index of the readout
/// among the batch's readouts is right whenever every instruction reads:
/// a flip before the first readout shows it, since the caller would change
/// the first readout, which was fine.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch, after an instruction that makes no readout, must \
                be rejected at its index, 3, not 1 (`[silent, a, silent, a]`)"
)]
fn refuses_a_repeated_tag_at_the_index_of_its_instruction_catches_a_refusal_at_the_readouts_index()
{
    refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut TestDoubleOfBits::new(BitsBehaviour::RefusesARepeatAtTheIndexOfItsReadout),
        &shots_0_and_1(),
        &BitStep::Read(0),
        &BitStep::Flip,
        &BitStep::ReadInverted(0),
    );
}

/// An instruction that reads only on odd shots makes no readout on shots 2
/// and 4, where a batch that holds it twice repeats no tag, so an executor
/// that runs the batch would be blamed for it: the check needs `a` to make
/// one readout on each shot, and names every shot where it does not.
#[test]
#[should_panic(
    expected = "`a` must make one readout on each shot, so that a batch that holds it twice \
                repeats a tag, but `[a]` does not, on shots 2, 4"
)]
fn refuses_a_repeated_tag_at_the_index_of_its_instruction_needs_a_readout_of_a_on_each_shot() {
    refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut TestDoubleOfBits::new(BitsBehaviour::Sound),
        &ShotSet::range(1..=4).expect("four shots"),
        &BitStep::ReadIfOdd(0),
        &BitStep::Flip,
        &BitStep::ReadInverted(0),
    );
}

/// A `silent` that reads on odd shots makes the index of a readout there
/// that of its instruction, so a refusal at the readout's index would pass
/// for nothing on them: the check needs `silent` to make no readout, and
/// names every shot where it makes one.
#[test]
#[should_panic(
    expected = "`silent` must make no readout, so that the index of a readout differs from its \
                instruction's, but `[silent]` makes some on shots 1, 3"
)]
fn refuses_a_repeated_tag_at_the_index_of_its_instruction_needs_silent_to_make_no_readout() {
    refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut TestDoubleOfBits::new(BitsBehaviour::Sound),
        &ShotSet::range(1..=4).expect("four shots"),
        &BitStep::Read(0),
        &BitStep::ReadIfOdd(1),
        &BitStep::ReadInverted(0),
    );
}

/// A device of two instruction slots refuses every batch of three steps
/// at the third, a limit of its own, which the check would blame on the
/// repeat of `a` in `[silent, a, silent, a]`: it needs
/// `[silent, a, silent]` accepted first.
#[test]
#[should_panic(
    expected = "`[silent, a, silent]` must be accepted, so that a refusal of a batch that adds a \
                second readout under `a`'s tag can only be the repeat's, but it was rejected with \
                `Instruction { at: 2"
)]
fn refuses_a_repeated_tag_at_the_index_of_its_instruction_needs_silent_a_silent_accepted() {
    refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut TestDoubleOfBits::new(BitsBehaviour::AtMostTwoSteps),
        &shots_0_and_1(),
        &BitStep::Read(0),
        &BitStep::Flip,
        &BitStep::ReadInverted(0),
    );
}

/// An executor that runs a batch that reads twice under one tag gives the
/// caller two readouts it cannot tell apart, whatever comes before them.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch, after an instruction that makes no readout, must \
                be rejected with `Instruction`, but it was accepted (`[silent, a, silent, a]`)"
)]
fn refuses_a_repeated_tag_at_the_index_of_its_instruction_catches_a_repeat_run() {
    refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut TestDoubleOfBits::new(BitsBehaviour::AcceptsARepeatedTag),
        &shots_0_and_1(),
        &BitStep::Read(0),
        &BitStep::Flip,
        &BitStep::ReadInverted(0),
    );
}

/// An executor that counts the instructions of a batch from its first
/// readout, as if a batch started there, is right whenever the batch starts
/// with a readout: a flip before it shows the fault.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch, after an instruction that makes no readout, must \
                be rejected at its index, 3, not 2 (`[silent, a, silent, a]`)"
)]
fn refuses_a_repeated_tag_at_the_index_of_its_instruction_catches_a_count_from_the_first_readout() {
    refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut TestDoubleOfBits::new(BitsBehaviour::RefusesARepeatCountingFromTheFirstReadout),
        &shots_0_and_1(),
        &BitStep::Read(0),
        &BitStep::Flip,
        &BitStep::ReadInverted(0),
    );
}

/// An executor that refuses a readout repeated step for step, rather than
/// a repeated tag, is right whenever the batch repeats an instruction:
/// another instruction under `a`'s tag shows it.
#[test]
#[should_panic(
    expected = "a tag repeated within a batch, after an instruction that makes no readout, must \
                be rejected with `Instruction`, but it was accepted \
                (`[silent, a, silent, a_again]`)"
)]
fn refuses_a_repeated_tag_at_the_index_of_its_instruction_catches_a_check_of_repeated_readouts() {
    refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut TestDoubleOfBits::new(BitsBehaviour::RefusesARepeatedReadout),
        &shots_0_and_1(),
        &BitStep::Read(0),
        &BitStep::Flip,
        &BitStep::ReadInverted(0),
    );
}

/// As for the other tag check, `a_again` that is `a` would let an
/// executor that refuses a repeated readout, not a repeated tag, pass.
#[test]
#[should_panic(
    expected = "`a_again` must be another instruction than `a`, so that a batch that holds both \
                repeats a tag, but no instruction"
)]
fn refuses_a_repeated_tag_at_the_index_of_its_instruction_needs_a_again_to_be_another_instruction()
{
    refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut TestDoubleOfBits::new(BitsBehaviour::RefusesARepeatedReadout),
        &shots_0_and_1(),
        &BitStep::Read(0),
        &BitStep::Flip,
        &BitStep::Read(0),
    );
}

/// An instruction of a [`TestDoubleOfBits`].
#[derive(Clone, Copy, PartialEq)]
enum BitStep {
    /// Flips the shot's bit.
    Flip,
    /// Sets the shot's bit to one: run again, it changes nothing.
    Set,
    /// Tosses the shot's bit: sets it to the parity of a draw, a count of
    /// the draws made, which starts at the shot's number.
    Toss,
    /// Reads the shot's bit, under a tag.
    Read(u32),
    /// Reads the bit of an odd-numbered shot, under a tag; reads nothing
    /// on an even one.
    ReadIfOdd(u32),
    /// Reads the complement of the shot's bit, under a tag: another
    /// readout than `Read`'s, of the same bit, the shot's only one.
    ReadInverted(u32),
    /// Rejects an odd-numbered shot: raises [`ShotConditions::REJECTED`]
    /// on it, which its readouts carry from then on; does nothing on an
    /// even one.
    RejectIfOdd,
}

impl BitStep {
    /// The readouts `batch` asks for, each with the index of its step, as
    /// [`check_unique_tags`] takes them.
    fn readouts(batch: &[Self]) -> impl Iterator<Item = (usize, u32)> + '_ {
        (batch.iter().enumerate()).filter_map(|(at, step)| match *step {
            Self::Read(tag) | Self::ReadIfOdd(tag) | Self::ReadInverted(tag) => Some((at, tag)),
            _ => None,
        })
    }
}

/// How a [`TestDoubleOfBits`] keeps the promises of `replayable` and
/// `resumable_shots`, or breaks one.
#[derive(Clone, Copy, PartialEq)]
enum BitsBehaviour {
    /// It keeps both.
    Sound,
    /// It keeps both, but gives a shot's readouts in reverse order when the
    /// batch has several shots, which their tags allow.
    ReadsBackwardsAmongOthers,
    /// Its shots toss from one count of draws, made in the order the shots
    /// run, so a shot's tosses depend on the shots run before it.
    SharesItsDraws,
    /// It makes each shot's bit again at every call.
    RestartsItsShots,
    /// It keeps shot 0's bit from one call to the next, but makes the
    /// others' again at every call.
    KeepsOnlyShot0,
    /// It names the readouts of a call's i-th shot by the i-th of the
    /// call's shots in increasing order: right only when they come in
    /// increasing order.
    NamesShotsAsIfSorted,
    /// It names a shot's readouts by the call's least shot plus the shot's
    /// rank among the call's shots: right only when they are contiguous.
    NamesShotsAsIfContiguous,
    /// It also runs a call's instructions on every shot it holds that the
    /// call does not name, and drops their readouts.
    RunsHeldShotsNotNamed,
    /// It runs a call's instructions on every shot from the call's least
    /// shot to its greatest, and drops the readouts of the shots the call
    /// does not name: right only when they are contiguous.
    RunsTheRangeOfItsShots,
    /// It also runs a call's instructions on every shot from the call's
    /// least shot to its greatest that it does not hold yet, making it,
    /// and drops their readouts.
    RunsNewShotsInItsRange,
    /// It makes a new shot's bit as a copy of a template bit, which every
    /// call's flips and sets also change (a grouped executor that forgets
    /// to copy its template before it runs a call on it).
    CopiesNewShotsFromATemplateItChanges,
    /// It makes every shot's bit again when a call names a shot it does
    /// not hold yet.
    RestartsItsShotsWhenOneJoins,
    /// It makes the bits of the shots a call names again when the call
    /// names a shot it does not hold yet, and keeps the others'.
    RemakesNamedShotsWhenOneJoins,
    /// It makes every shot's bit again when a call names other shots than
    /// the call before it.
    RestartsItsShotsWhenTheShotsChange,
    /// It keeps both, but runs a batch that reads twice under one tag,
    /// against the rule of tags, where the others refuse it.
    AcceptsARepeatedTag,
    /// It keeps both, but refuses a batch that reads twice under one tag at
    /// the index of the readout among the batch's readouts, not of its
    /// instruction among the batch's instructions.
    RefusesARepeatAtTheIndexOfItsReadout,
    /// It keeps both, but refuses a batch that reads twice under one tag at
    /// the index of its instruction counted from the batch's first readout.
    RefusesARepeatCountingFromTheFirstReadout,
    /// It keeps both, but refuses every batch of more than two steps at
    /// its third, as a device of two instruction slots.
    AtMostTwoSteps,
    /// It keeps both, but refuses a batch at a readout that repeats an
    /// earlier readout, step for step, not at one that repeats its tag.
    RefusesARepeatedReadout,
    /// It keeps both, but forgets its shots' conditions at every call.
    ForgetsConditions,
    /// It keeps both, but raises a condition raised on one shot of a call
    /// on every shot of the call, and on their readouts (a layer that
    /// marks the shots of a call as one).
    RaisesOnEveryShotOfTheCall,
    /// It keeps both, but raises a condition raised in a call on every
    /// shot it holds, after the call (a layer that marks every state it
    /// holds).
    RaisesOnEveryShotItHolds,
    /// It keeps both, but gives a shot's conditions on the shot's last
    /// readout of a call alone (a layer that marks what its sends read,
    /// but not what its logic wrote before them in its reply).
    MarksTheLastReadoutOfAShotAlone,
}

/// A shot of a [`TestDoubleOfBits`]: its bit, the draws its tosses made,
/// and its conditions.
type ShotOfBits = (bool, u32, ShotConditions);

/// The shots of a device that several [`TestDoubleOfBits`] run on, one
/// after another.
type Device = Rc<RefCell<BTreeMap<ShotId, ShotOfBits>>>;

/// An executor of a bit per shot, which offers `replayable` and
/// `resumable_shots` and may break one of them, or start its shots from a
/// device that an earlier executor left as it was. It refuses a batch that
/// reads twice under one tag, unless it is
/// [`BitsBehaviour::AcceptsARepeatedTag`], at the instruction that repeats the
/// tag, unless it is [`BitsBehaviour::RefusesARepeatAtTheIndexOfItsReadout`].
struct TestDoubleOfBits {
    behaviour: BitsBehaviour,
    /// Each shot's bit, the draws its tosses made, and its conditions.
    shots: BTreeMap<ShotId, ShotOfBits>,
    /// The draws of all the shots, for [`BitsBehaviour::SharesItsDraws`].
    shared: u32,
    /// The bit a new shot starts with, for
    /// [`BitsBehaviour::CopiesNewShotsFromATemplateItChanges`].
    template: bool,
    /// The device whose shots it starts from, and leaves as they are when
    /// it closes, not reset, for the next executor on it, unless it resets
    /// the device at close.
    device: Option<Device>,
    /// Whether it runs on the device's shots themselves, leaving them
    /// changed after each call, and resets the device when it closes, not
    /// when it is dropped unclosed.
    resets_the_device_at_close: bool,
    /// The shots the last call named, for
    /// [`BitsBehaviour::RestartsItsShotsWhenTheShotsChange`].
    last_named: Vec<ShotId>,
}

impl TestDoubleOfBits {
    fn new(behaviour: BitsBehaviour) -> Self {
        Self {
            behaviour,
            shots: BTreeMap::new(),
            shared: 0,
            template: false,
            device: None,
            resets_the_device_at_close: false,
            last_named: Vec::new(),
        }
    }

    /// A sound executor that runs on the shots `device` holds, changing
    /// them as it runs, and resets `device` when it closes: one dropped
    /// unclosed leaves its shots changed for the next executor on it.
    fn reset_at_close_only(device: &Device) -> Self {
        Self {
            resets_the_device_at_close: true,
            ..Self::on(device)
        }
    }

    /// A sound executor that starts from the shots `device` holds, and
    /// leaves its own there when it closes.
    fn on(device: &Device) -> Self {
        Self {
            shots: device.borrow().clone(),
            device: Some(Rc::clone(device)),
            ..Self::new(BitsBehaviour::Sound)
        }
    }

    /// Its check of a batch: refuses one that reads twice under one tag,
    /// unless its behaviour breaks that rule, or one it cannot run.
    fn check(&self, batch: &[BitStep]) -> Result<(), Rejected> {
        match self.behaviour {
            BitsBehaviour::AcceptsARepeatedTag => Ok(()),
            BitsBehaviour::AtMostTwoSteps if batch.len() > 2 => Err(Rejected::Instruction {
                at: 2,
                reason: "two instruction slots".into(),
            }),
            BitsBehaviour::RefusesARepeatAtTheIndexOfItsReadout => {
                check_unique_tags(BitStep::readouts(batch).map(|(_, tag)| tag).enumerate())
            }
            BitsBehaviour::RefusesARepeatCountingFromTheFirstReadout => {
                let from = BitStep::readouts(batch).next().map_or(0, |(at, _)| at);
                check_unique_tags(BitStep::readouts(batch).map(|(at, tag)| (at - from, tag)))
            }
            BitsBehaviour::RefusesARepeatedReadout => {
                let readouts: Vec<usize> = BitStep::readouts(batch).map(|(at, _)| at).collect();
                let repeat = (readouts.iter().enumerate()).find(|&(nth, &at)| {
                    readouts[..nth]
                        .iter()
                        .any(|&before| batch[before] == batch[at])
                });
                match repeat {
                    Some((_, &at)) => Err(Rejected::Instruction {
                        at,
                        reason: "a repeated readout".into(),
                    }),
                    None => Ok(()),
                }
            }
            _ => check_unique_tags(BitStep::readouts(batch)),
        }
    }

    /// The shots a call that names `named` runs on, in order: those it
    /// names, unless its behaviour runs others too.
    fn runs_on(&self, named: &[ShotId]) -> Vec<ShotId> {
        match self.behaviour {
            BitsBehaviour::RunsHeldShotsNotNamed => (named.iter().copied())
                .chain(
                    self.shots
                        .keys()
                        .copied()
                        .filter(|shot| !named.contains(shot)),
                )
                .collect(),
            BitsBehaviour::RunsTheRangeOfItsShots => {
                let (least, greatest) = (named.iter().min(), named.iter().max());
                (*least.expect("a shot set is not empty")..=*greatest.expect("as above")).collect()
            }
            BitsBehaviour::RunsNewShotsInItsRange => {
                let (least, greatest) = (named.iter().min(), named.iter().max());
                (*least.expect("a shot set is not empty")..=*greatest.expect("as above"))
                    .filter(|shot| named.contains(shot) || !self.shots.contains_key(shot))
                    .collect()
            }
            _ => named.to_vec(),
        }
    }

    /// Forgets, as a call starts, what its behaviour forgets: its shots,
    /// all but shot 0's, or their conditions.
    fn forget_at_call(&mut self) {
        match self.behaviour {
            BitsBehaviour::RestartsItsShots => self.shots.clear(),
            BitsBehaviour::KeepsOnlyShot0 => self.shots.retain(|&shot, _| shot == 0),
            BitsBehaviour::ForgetsConditions => {
                for (_, _, conditions) in self.shots.values_mut() {
                    *conditions = ShotConditions::NONE;
                }
            }
            _ => {}
        }
    }

    /// Raises `raised`, the conditions raised in a call on the shots
    /// `named`, on the shots its behaviour spreads them to, and on the
    /// call's `outcomes` when it marks them all.
    fn spread(&mut self, named: &[ShotId], raised: ShotConditions, outcomes: &mut [ShotOutcome]) {
        let raise_on = |shot: &ShotId| match self.behaviour {
            BitsBehaviour::RaisesOnEveryShotOfTheCall => named.contains(shot),
            BitsBehaviour::RaisesOnEveryShotItHolds => true,
            _ => false,
        };
        for (_, (_, _, conditions)) in self.shots.iter_mut().filter(|(shot, _)| raise_on(shot)) {
            *conditions |= raised;
        }
        if self.behaviour == BitsBehaviour::RaisesOnEveryShotOfTheCall {
            for outcome in outcomes {
                outcome.conditions |= raised;
            }
        }
    }

    /// Clears the conditions of each shot's readouts of the call but its
    /// last, when its behaviour marks the last alone.
    fn clear_earlier_readouts(&self, outcomes: &mut [ShotOutcome]) {
        if self.behaviour == BitsBehaviour::MarksTheLastReadoutOfAShotAlone {
            let mut later = BTreeSet::new();
            for outcome in outcomes.iter_mut().rev() {
                if !later.insert(outcome.shot) {
                    outcome.conditions = ShotConditions::NONE;
                }
            }
        }
    }
}

impl Close for TestDoubleOfBits {
    type Report = ();

    fn close(&mut self) {
        if let Some(device) = &self.device {
            if self.resets_the_device_at_close {
                device.borrow_mut().clear();
            } else {
                device.borrow_mut().clone_from(&self.shots);
            }
        }
    }
}

impl Executor<InstructionBatch<BitStep>> for TestDoubleOfBits {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, BitStep>) -> Result<Self::Reply, Rejected> {
        self.check(batch.instructions)?;
        self.forget_at_call();
        let mut raised = ShotConditions::NONE;
        let mut outcomes = Vec::new();
        let named = batch.shots.as_slice();
        let joins = named.iter().any(|shot| !self.shots.contains_key(shot));
        if self.behaviour == BitsBehaviour::RestartsItsShotsWhenOneJoins && joins {
            self.shots.clear();
        }
        if self.behaviour == BitsBehaviour::RemakesNamedShotsWhenOneJoins && joins {
            self.shots.retain(|shot, _| !named.contains(shot));
        }
        if self.behaviour == BitsBehaviour::RestartsItsShotsWhenTheShotsChange
            && named != self.last_named
        {
            self.shots.clear();
            self.last_named = named.to_vec();
        }
        if self.behaviour == BitsBehaviour::CopiesNewShotsFromATemplateItChanges {
            for &shot in named {
                (self.shots.entry(shot)).or_insert((self.template, shot, ShotConditions::NONE));
            }
            for step in batch.instructions {
                match *step {
                    BitStep::Flip => self.template = !self.template,
                    BitStep::Set => self.template = true,
                    BitStep::Toss
                    | BitStep::Read(_)
                    | BitStep::ReadIfOdd(_)
                    | BitStep::ReadInverted(_)
                    | BitStep::RejectIfOdd => {}
                }
            }
        }
        for shot in self.runs_on(named) {
            let first = outcomes.len();
            let (bit, draws, conditions) =
                (self.shots.entry(shot)).or_insert((false, shot, ShotConditions::NONE));
            for step in batch.instructions {
                match *step {
                    BitStep::Flip => *bit = !*bit,
                    BitStep::Set => *bit = true,
                    BitStep::Toss => {
                        let draw = if self.behaviour == BitsBehaviour::SharesItsDraws {
                            &mut self.shared
                        } else {
                            &mut *draws
                        };
                        *bit = *draw % 2 == 1;
                        *draw += 1;
                    }
                    BitStep::Read(tag) => outcomes.push(
                        ShotOutcome::new(shot, tag, (*bit).into()).with_conditions(*conditions),
                    ),
                    BitStep::ReadIfOdd(tag) if shot % 2 == 1 => outcomes.push(
                        ShotOutcome::new(shot, tag, (*bit).into()).with_conditions(*conditions),
                    ),
                    BitStep::ReadInverted(tag) => outcomes.push(
                        ShotOutcome::new(shot, tag, (!*bit).into()).with_conditions(*conditions),
                    ),
                    BitStep::RejectIfOdd if shot % 2 == 1 => {
                        *conditions |= ShotConditions::REJECTED;
                        raised |= ShotConditions::REJECTED;
                    }
                    BitStep::ReadIfOdd(_) | BitStep::RejectIfOdd => {}
                }
            }
            if self.behaviour == BitsBehaviour::ReadsBackwardsAmongOthers && batch.shots.len() > 1 {
                outcomes[first..].reverse();
            }
        }
        outcomes.retain(|outcome| named.contains(&outcome.shot));
        self.spread(named, raised, &mut outcomes);
        self.clear_earlier_readouts(&mut outcomes);
        let mut sorted = batch.shots.as_slice().to_vec();
        sorted.sort_unstable();
        let name: BTreeMap<ShotId, ShotId> = match self.behaviour {
            BitsBehaviour::NamesShotsAsIfSorted => {
                batch.shots.iter().copied().zip(sorted).collect()
            }
            BitsBehaviour::NamesShotsAsIfContiguous => {
                (sorted.iter().copied()).zip(sorted[0]..).collect()
            }
            _ => BTreeMap::new(),
        };
        for outcome in &mut outcomes {
            outcome.shot = name.get(&outcome.shot).copied().unwrap_or(outcome.shot);
        }
        if self.resets_the_device_at_close
            && let Some(device) = &self.device
        {
            device.borrow_mut().clone_from(&self.shots);
        }
        Ok(ImmediateReply::done(outcomes))
    }
}

/// Shots 0 and 1.
fn shots_0_and_1() -> ShotSet {
    ShotSet::new(vec![0, 1]).expect("two shots")
}

/// An executor whose shots toss from one count of draws breaks the promise
/// of `replayable`: among shots 0 to 3, each shot tosses the parity of the
/// draws made before it, so shots 1 and 3 toss one, where alone each tosses
/// zero. The check names both.
#[test]
#[should_panic(
    expected = "each shot alone must give what it gives among the shots [0, 1, 2, 3], but it \
                gives otherwise on shots 1, 3"
)]
fn gives_each_shot_alone_what_it_gives_among_others_catches_draws_shared_by_the_shots() {
    gives_each_shot_alone_what_it_gives_among_others(
        || TestDoubleOfBits::new(BitsBehaviour::SharesItsDraws),
        &shots(&[0, 1, 2, 3]),
        &[&[BitStep::Toss, BitStep::Read(0)]],
    );
}

/// An ensemble executor, whose shots share one state on one seed, gives
/// each shot alone what it gives among the others, since the state starts
/// alike; but split into two calls, the shots run each batch twice on that
/// state, so their readouts change: it does not offer `replayable`, and the
/// check tells it.
#[test]
#[should_panic(
    expected = "each shot must give what it gives among the shots [0, 1, 2] when they are split \
                into calls at odd indices, then at even ones, but it gives otherwise on shots 0, 2"
)]
fn gives_each_shot_alone_what_it_gives_among_others_catches_shots_that_share_one_state() {
    let batches = flip_and_toss(&bits_set());
    gives_each_shot_alone_what_it_gives_among_others(
        || SharedState::new(3),
        &shots(&[0, 1, 2]),
        &[&batches[0], &batches[1]],
    );
}

/// An executor that names the readouts of a call's shots as if they came
/// in increasing order is right on every call of shots in that order: all
/// the shots, one alone, or split into odd and even ones. The check splits
/// the shots in reverse order too, [3, 2] then [1, 0], where it gives shot
/// 0 shot 1's toss, though the caller gives one batch: both splits run
/// whatever the number of batches, so a caller of one batch is checked as
/// fully as one of many.
#[test]
#[should_panic(
    expected = "each shot must give what it gives among the shots [0, 1, 2, 3] when they are \
                split into calls in reverse order, in two halves, but it gives otherwise on \
                shots 0, 1, 2, 3"
)]
fn gives_each_shot_alone_what_it_gives_among_others_catches_shots_named_as_if_sorted() {
    gives_each_shot_alone_what_it_gives_among_others(
        || TestDoubleOfBits::new(BitsBehaviour::NamesShotsAsIfSorted),
        &shots(&[0, 1, 2, 3]),
        &[&[BitStep::Toss, BitStep::Read(0)]],
    );
}

/// An executor that names the readouts of a call's shots as if they were
/// contiguous is right on every call of contiguous shots: all the shots,
/// one alone, or split into halves. The check splits the shots into odd
/// and even ones too, [1, 3] then [0, 2], where it names shot 2's readout
/// as shot 1's, though the caller gives one batch: both splits run
/// whatever the number of batches.
#[test]
#[should_panic(
    expected = "each shot must give what it gives among the shots [0, 1, 2, 3] when they are \
                split into calls at odd indices, then at even ones, but it gives otherwise on \
                shots 1, 2, 3"
)]
fn gives_each_shot_alone_what_it_gives_among_others_catches_shots_named_as_if_contiguous() {
    gives_each_shot_alone_what_it_gives_among_others(
        || TestDoubleOfBits::new(BitsBehaviour::NamesShotsAsIfContiguous),
        &shots(&[0, 1, 2, 3]),
        &[&[BitStep::Toss, BitStep::Read(0)]],
    );
}

/// An executor that raises a condition raised on one shot of a call on
/// every shot of the call, as a layer that marks a call's shots as one,
/// gives its shots' own readouts, but marks the even shots rejected when
/// they share a call with an odd one: alone, they are not. The check
/// compares the conditions with the readouts, and names both.
#[test]
#[should_panic(
    expected = "each shot alone must give what it gives among the shots [0, 1, 2, 3], but it \
                gives otherwise on shots 0, 2"
)]
fn gives_each_shot_alone_what_it_gives_among_others_catches_a_condition_raised_on_other_shots() {
    gives_each_shot_alone_what_it_gives_among_others(
        || TestDoubleOfBits::new(BitsBehaviour::RaisesOnEveryShotOfTheCall),
        &shots(&[0, 1, 2, 3]),
        &[&[BitStep::Toss, BitStep::RejectIfOdd, BitStep::Read(0)]],
    );
}

/// Shots that read alike, on batches that draw nothing (a flip, then a
/// read), would read alike from a generator they share, so the check needs
/// readouts that differ between some two shots.
#[test]
#[should_panic(expected = "the shots' readouts must differ between some two shots")]
fn gives_each_shot_alone_what_it_gives_among_others_needs_shots_that_read_differently() {
    gives_each_shot_alone_what_it_gives_among_others(
        || TestDoubleOfBits::new(BitsBehaviour::Sound),
        &shots(&[0, 1, 2]),
        &[&[BitStep::Flip, BitStep::Read(0)]],
    );
}

/// A device that runs one instruction per batch refuses a batch of two, so
/// there is nothing to compare: the check needs its batches accepted.
#[test]
#[should_panic(
    expected = "the batches must be accepted, but one was refused with `Instruction { at: 1"
)]
fn gives_each_shot_alone_what_it_gives_among_others_needs_its_batches_accepted() {
    gives_each_shot_alone_what_it_gives_among_others(
        || TestDoubleOfOneInstruction::new(OnARefusal::Nothing),
        &shots(&[0, 1, 2]),
        &[&[true, true]],
    );
}

/// Of two shots, both ways the check splits them give the same calls (the
/// shot at index 1, then the one at index 0), so a shot named by its rank
/// among sorted shots, or contiguous ones, would not show: the check needs
/// three shots or more.
#[test]
#[should_panic(expected = "`shots` must hold three shots or more")]
fn gives_each_shot_alone_what_it_gives_among_others_needs_three_shots() {
    gives_each_shot_alone_what_it_gives_among_others(
        || TestDoubleOfBits::new(BitsBehaviour::Sound),
        &shots_0_and_1(),
        &[&[BitStep::Toss, BitStep::Read(0)]],
    );
}

/// Batches that read nothing give nothing to compare, so the check needs a
/// readout.
#[test]
#[should_panic(expected = "`batches` must make a readout")]
fn gives_each_shot_alone_what_it_gives_among_others_needs_a_readout() {
    gives_each_shot_alone_what_it_gives_among_others(
        || TestDoubleOfBits::new(BitsBehaviour::Sound),
        &shots(&[0, 1, 2]),
        &[&[BitStep::Toss]],
    );
}

/// An executor that gives a shot's readouts in another order among other
/// shots than alone keeps the promise: the tags name the readouts.
#[test]
fn gives_each_shot_alone_what_it_gives_among_others_accepts_readouts_in_another_order() {
    gives_each_shot_alone_what_it_gives_among_others(
        || TestDoubleOfBits::new(BitsBehaviour::ReadsBackwardsAmongOthers),
        &shots(&[0, 1, 2]),
        &[&[
            BitStep::Toss,
            BitStep::Read(0),
            BitStep::Flip,
            BitStep::Read(1),
        ]],
    );
}

/// A reply that hands over its events by putting them in place of what the
/// caller's buffer held, rather than after it.
struct ReplacesWhatTheCallerHeld(ImmediateReply<ShotOutcome>);

impl Reply for ReplacesWhatTheCallerHeld {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        let mut events = Vec::new();
        let status = self.0.poll_events(&mut events, waker);
        *out = events;
        status
    }
}

/// A [`TestDoubleOfBits`] that keeps its promises, but whose replies
/// replace what the caller held.
struct TestDoubleReplacesWhatTheCallerHeld(TestDoubleOfBits);

impl Executor<InstructionBatch<BitStep>> for TestDoubleReplacesWhatTheCallerHeld {
    type Reply = ReplacesWhatTheCallerHeld;

    fn call(&mut self, batch: InstructionBatchRef<'_, BitStep>) -> Result<Self::Reply, Rejected> {
        self.0.call(batch).map(ReplacesWhatTheCallerHeld)
    }
}

/// A driver that gathers the readouts of its batches in one buffer, waiting
/// for each reply into it, loses the earlier batches' readouts to a reply
/// that replaces what the buffer held: it breaks the rule that a reply
/// appends its events, which the checks that read readouts check, though
/// each shot's readouts of each call are right.
#[test]
#[should_panic(
    expected = "a reply must append its events to the caller's buffer, keeping what it held"
)]
fn gives_each_shot_alone_what_it_gives_among_others_catches_a_reply_that_drops_what_was_held() {
    gives_each_shot_alone_what_it_gives_among_others(
        || TestDoubleReplacesWhatTheCallerHeld(TestDoubleOfBits::new(BitsBehaviour::Sound)),
        &shots(&[0, 1, 2]),
        &[&[BitStep::Toss, BitStep::Read(0)]],
    );
}

/// A reply that marks the outcomes of the shots it flags with their
/// conditions from the start of the caller's buffer, rather than from
/// where its own events begin.
struct MarksWhatTheCallerHeld(ImmediateReply<ShotOutcome>);

impl Reply for MarksWhatTheCallerHeld {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        let from = out.len();
        let status = self.0.poll_events(out, waker);
        let flagged: BTreeMap<ShotId, ShotConditions> = out[from..]
            .iter()
            .filter(|o| !o.conditions.is_empty())
            .map(|o| (o.shot, o.conditions))
            .collect();
        for outcome in out.iter_mut() {
            if let Some(&conditions) = flagged.get(&outcome.shot) {
                outcome.conditions |= conditions;
            }
        }
        status
    }
}

/// A [`TestDoubleOfBits`] that keeps its promises, but whose replies mark
/// the outcomes the caller held of the shots they flag.
struct TestDoubleMarksWhatTheCallerHeld(TestDoubleOfBits);

impl Executor<InstructionBatch<BitStep>> for TestDoubleMarksWhatTheCallerHeld {
    type Reply = MarksWhatTheCallerHeld;

    fn call(&mut self, batch: InstructionBatchRef<'_, BitStep>) -> Result<Self::Reply, Rejected> {
        (self.0.call(batch)).map(MarksWhatTheCallerHeld)
    }
}

/// A driver that gathers a shot's readouts of several calls in one buffer
/// finds its earlier readouts of shot 1, the one the executor rejects and
/// not the first of the call, marked by a later reply: the checks that
/// read readouts hold an outcome of each shot the call names, so that they
/// see it.
#[test]
#[should_panic(
    expected = "a reply must append its events to the caller's buffer, keeping what it held"
)]
fn keeps_each_shots_conditions_from_one_call_to_the_next_catches_a_reply_that_marks_what_was_held()
{
    keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut TestDoubleMarksWhatTheCallerHeld(TestDoubleOfBits::new(BitsBehaviour::Sound)),
        &shots(&[0, 1, 2]),
        &[
            &[BitStep::RejectIfOdd, BitStep::Read(0)],
            &[BitStep::Read(1)],
        ],
    );
}

/// An executor that makes its shots again at every call breaks the promise
/// of `resumable_shots`: a flip, then a read in a second call, reads zero,
/// where one batch of both reads one.
#[test]
#[should_panic(
    expected = "a second call must continue each shot where the first left it: its readouts \
                over two calls must be those of one batch of both, but they differ on shots 0, 1"
)]
fn continues_a_shot_from_one_call_to_the_next_catches_shots_made_again_at_each_call() {
    continues_a_shot_from_one_call_to_the_next(
        || TestDoubleOfBits::new(BitsBehaviour::RestartsItsShots),
        &shots_0_and_1(),
        &[BitStep::Flip],
        &[BitStep::Read(0)],
    );
}

/// An executor that keeps shot 0's bit from one call to the next, but
/// makes the others' again, breaks the promise of `resumable_shots` on shots 1
/// and 2 only: a flip, then a read in a second call, reads zero on them.
/// The check names both.
#[test]
#[should_panic(
    expected = "a second call must continue each shot where the first left it: its readouts \
                over two calls must be those of one batch of both, but they differ on shots 1, 2"
)]
fn continues_a_shot_from_one_call_to_the_next_catches_a_shot_made_again_among_kept_ones() {
    continues_a_shot_from_one_call_to_the_next(
        || TestDoubleOfBits::new(BitsBehaviour::KeepsOnlyShot0),
        &shots(&[0, 1, 2]),
        &[BitStep::Flip],
        &[BitStep::Read(0)],
    );
}

/// An executor that makes its shots again when a call names other shots
/// than the call before it breaks the promise of `resumable_shots`, though two
/// calls on the same shots continue them: after a toss on shots 0 to 3,
/// which changes shots 1 and 3, a read on shots 1 and 3, then one on 0 and
/// 2, reads zero on 1 and 3, where one batch of both reads one. The check
/// names both.
#[test]
#[should_panic(
    expected = "a call must continue each shot it names where the calls before it left it, or \
                as made if none of them named it, whatever shots they named, but a shot reads \
                otherwise when `second` names part of the shots that `first` named, on shots \
                1, 3"
)]
fn continues_a_shot_from_one_call_to_the_next_catches_shots_made_again_when_the_shots_change() {
    continues_a_shot_from_one_call_to_the_next(
        || TestDoubleOfBits::new(BitsBehaviour::RestartsItsShotsWhenTheShotsChange),
        &shots(&[0, 1, 2, 3]),
        &[BitStep::Toss],
        &[BitStep::Read(0)],
    );
}

/// An executor that makes again the shots a call names when one of them is
/// new to it breaks the promise of `resumable_shots` where a feedback program
/// calls a subset of its shots, then all of them: shots 0, 2, 1, 3, 4 and
/// 5, given in that order, are at odd indices 2, 3 and 5, which a toss
/// changes on 3 and 5; the read on all the shots that follows makes them
/// again, so they read zero, where over two calls on all the shots they
/// read one. The check names both.
#[test]
#[should_panic(
    expected = "a call must continue each shot it names where the calls before it left it, or \
                as made if none of them named it, whatever shots they named, but a shot reads \
                otherwise when `second` names it with shots new to the executor, on shots 3, 5"
)]
fn continues_a_shot_from_one_call_to_the_next_catches_named_shots_made_again_when_one_joins() {
    continues_a_shot_from_one_call_to_the_next(
        || TestDoubleOfBits::new(BitsBehaviour::RemakesNamedShotsWhenOneJoins),
        &shots(&[0, 2, 1, 3, 4, 5]),
        &[BitStep::Toss],
        &[BitStep::Read(0)],
    );
}

/// An executor that also runs a call on the shots in its range it does not
/// hold yet breaks the promise that a shot starts as made at its first
/// call: `first` on shots 1, 3 and 5 flips shots 2 and 4 too, so the read
/// on all the shots that follows reads one on them, where alone it reads
/// zero. Shots 0 and 6 lie outside the range, and read as made. The check
/// names both.
#[test]
#[should_panic(
    expected = "a call must continue each shot it names where the calls before it left it, or \
                as made if none of them named it, whatever shots they named, but a shot new to \
                the executor reads otherwise than alone when `second` names it with shots that \
                `first` named, on shots 2, 4"
)]
fn continues_a_shot_from_one_call_to_the_next_catches_a_shot_changed_before_its_first_call() {
    continues_a_shot_from_one_call_to_the_next(
        || TestDoubleOfBits::new(BitsBehaviour::RunsNewShotsInItsRange),
        &ShotSet::range(0..=6).expect("seven shots"),
        &[BitStep::Flip],
        &[BitStep::Read(0)],
    );
}

/// A first and a second call that read under one tag cannot be joined into
/// one batch, whose readouts the check compares with the two calls', so the
/// check needs other tags in `second` than in `first`: a `first` that reads
/// under tag 0 on odd shots only clashes with `second` on shots 3 and 5 of
/// shots 2 to 5, and the check names both.
#[test]
#[should_panic(
    expected = "`first` and `second` must use different tags, so that one batch of both is \
                accepted, but they read under a tag in both on shots 3, 5"
)]
fn continues_a_shot_from_one_call_to_the_next_needs_other_tags_in_the_second_call() {
    continues_a_shot_from_one_call_to_the_next(
        || TestDoubleOfBits::new(BitsBehaviour::Sound),
        &ShotSet::range(2..=5).expect("four shots"),
        &[BitStep::Flip, BitStep::ReadIfOdd(0)],
        &[BitStep::Read(0)],
    );
}

/// A first call that only reads leaves the shots as they were made, so an
/// executor that makes them again at the second call would read the same:
/// the check needs a second call that reads what the first changed.
#[test]
#[should_panic(expected = "`second` must read what `first` changed")]
fn continues_a_shot_from_one_call_to_the_next_needs_the_second_call_to_read_a_change() {
    continues_a_shot_from_one_call_to_the_next(
        || TestDoubleOfBits::new(BitsBehaviour::Sound),
        &shots_0_and_1(),
        &[BitStep::Read(1)],
        &[BitStep::Read(0)],
    );
}

/// A toss sets a shot's bit to the parity of its first draw, which is the
/// shot's number: of shots 1 to 4, it leaves 2 and 4 as they were made, so
/// an executor that made them again at the second call would not show on
/// them, though 1 and 3 read the toss: the check needs a change on each
/// shot, and names every shot without one.
#[test]
#[should_panic(
    expected = "alone, it must read otherwise than after `first` on each shot, but it reads the \
                same on shots 2, 4"
)]
fn continues_a_shot_from_one_call_to_the_next_needs_a_change_on_each_shot() {
    continues_a_shot_from_one_call_to_the_next(
        || TestDoubleOfBits::new(BitsBehaviour::Sound),
        &ShotSet::range(1..=4).expect("four shots"),
        &[BitStep::Toss],
        &[BitStep::Read(0)],
    );
}

/// A device that stops at a `false` fails the reply of a first call that
/// holds one, so there is nothing to continue: the check needs its
/// batches run.
#[test]
#[should_panic(
    expected = "the batches must run, but the reply to one failed: `the device stopped`"
)]
fn continues_a_shot_from_one_call_to_the_next_needs_its_batches_run() {
    continues_a_shot_from_one_call_to_the_next(
        || TestDoubleOfOneInstruction::new(OnARefusal::Nothing),
        &shots_0_and_1(),
        &[false],
        &[true],
    );
}

/// A second call that reads nothing does not show the state the first
/// left, so the check needs a readout in it.
#[test]
#[should_panic(expected = "`second` must make a readout")]
fn continues_a_shot_from_one_call_to_the_next_needs_a_readout_in_the_second_call() {
    continues_a_shot_from_one_call_to_the_next(
        || TestDoubleOfBits::new(BitsBehaviour::Sound),
        &shots_0_and_1(),
        &[BitStep::Read(0)],
        &[BitStep::Flip],
    );
}

/// A first call that reads under tag 1 and a second under tag 0 give the
/// readouts in another order than one batch of both: the tags name the
/// readouts, so the promise holds.
#[test]
fn continues_a_shot_from_one_call_to_the_next_accepts_tags_in_another_order() {
    continues_a_shot_from_one_call_to_the_next(
        || TestDoubleOfBits::new(BitsBehaviour::Sound),
        &shots_0_and_1(),
        &[BitStep::Flip, BitStep::Read(1)],
        &[BitStep::Read(0)],
    );
}

/// Shots 0 to 3.
fn shots_0_to_3() -> ShotSet {
    ShotSet::range(0..=3).expect("four shots")
}

/// An executor that also runs a call on the shots it holds but the call
/// does not name breaks the rule: shots 0 and 2 flip, then shots 1 and 3,
/// which flips 0 and 2 back, so they read zero, where they read one when
/// the call on 1 and 3 does not run. The check names both.
#[test]
#[should_panic(
    expected = "a call must change no shot it does not name, but a shot reads otherwise after \
                a call on the shots [1, 3], on shots 0, 2"
)]
fn changes_no_shot_a_call_does_not_name_catches_a_call_run_on_held_shots() {
    changes_no_shot_a_call_does_not_name(
        || TestDoubleOfBits::new(BitsBehaviour::RunsHeldShotsNotNamed),
        &shots_0_to_3(),
        &[BitStep::Flip],
        &[BitStep::Read(0)],
    );
}

/// An executor that raises a condition raised in a call on every shot it
/// holds, as a layer that marks every state it holds, leaves shots 0 and
/// 2 unchanged when the call on shots 1 and 3 rejects them, but marks
/// them rejected too: their readouts carry it, where they do not when
/// that call does not run. The check names both.
#[test]
#[should_panic(
    expected = "a call must change no shot it does not name, but a shot reads otherwise after \
                a call on the shots [1, 3], on shots 0, 2"
)]
fn changes_no_shot_a_call_does_not_name_catches_a_condition_raised_on_held_shots() {
    changes_no_shot_a_call_does_not_name(
        || TestDoubleOfBits::new(BitsBehaviour::RaisesOnEveryShotItHolds),
        &shots_0_to_3(),
        &[BitStep::Flip, BitStep::RejectIfOdd],
        &[BitStep::Read(0)],
    );
}

/// An executor that runs a call on the range of its shots breaks the rule
/// where the call's shots are not contiguous: of shots 0 to 5, the call on
/// shots 1, 3 and 5 flips shots 2 and 4 too, though not shot 0. The check
/// names both.
#[test]
#[should_panic(
    expected = "a call must change no shot it does not name, but a shot reads otherwise after \
                a call on the shots [1, 3, 5], on shots 2, 4"
)]
fn changes_no_shot_a_call_does_not_name_catches_a_call_run_on_the_range_of_its_shots() {
    changes_no_shot_a_call_does_not_name(
        || TestDoubleOfBits::new(BitsBehaviour::RunsTheRangeOfItsShots),
        &ShotSet::range(0..=5).expect("six shots"),
        &[BitStep::Flip],
        &[BitStep::Read(0)],
    );
}

/// Shots given out of order, 0, 2, 1 and 3, whose odd and even indices in
/// that order would split them into the ranges 2 to 3 and 0 to 1: the check
/// splits them in increasing order, so the call on shots 1 and 3 still
/// flips shot 2 on an executor that runs a call on the range of its shots.
#[test]
#[should_panic(
    expected = "a call must change no shot it does not name, but a shot reads otherwise after \
                a call on the shots [1, 3], on shot 2"
)]
fn changes_no_shot_a_call_does_not_name_catches_a_call_run_on_the_range_of_shots_given_in_any_order()
 {
    changes_no_shot_a_call_does_not_name(
        || TestDoubleOfBits::new(BitsBehaviour::RunsTheRangeOfItsShots),
        &ShotSet::new(vec![0, 2, 1, 3]).expect("four shots, each named once"),
        &[BitStep::Flip],
        &[BitStep::Read(0)],
    );
}

/// An executor that also runs a call on the shots in its range it does not
/// hold yet breaks the rule: of shots 0 to 5, the call on shots 0, 2 and 4
/// flips shots 1 and 3 too, which then read one at their first call, where
/// they read zero alone; shot 5 lies outside the range. The check names
/// both.
#[test]
#[should_panic(
    expected = "a call must change no shot it does not name, held or not, so that a new shot \
                starts fresh, but a shot reads otherwise after a call on the shots [0, 2, 4] \
                than alone, on shots 1, 3"
)]
fn changes_no_shot_a_call_does_not_name_catches_a_call_run_on_new_shots_in_its_range() {
    changes_no_shot_a_call_does_not_name(
        || TestDoubleOfBits::new(BitsBehaviour::RunsNewShotsInItsRange),
        &ShotSet::range(0..=5).expect("six shots"),
        &[BitStep::Flip],
        &[BitStep::Read(0)],
    );
}

/// A grouped executor that makes each new shot from a template, but runs
/// each call on the template too, breaks the rule: after the call on shots
/// 0 and 2 flipped the template, shots 1 and 3 start flipped, so they read
/// one at their first call, where they read zero alone. The check names
/// both.
#[test]
#[should_panic(
    expected = "a call must change no shot it does not name, held or not, so that a new shot \
                starts fresh, but a shot reads otherwise after a call on the shots [0, 2] than \
                alone, on shots 1, 3"
)]
fn changes_no_shot_a_call_does_not_name_catches_new_shots_copied_from_a_template_calls_change() {
    changes_no_shot_a_call_does_not_name(
        || TestDoubleOfBits::new(BitsBehaviour::CopiesNewShotsFromATemplateItChanges),
        &shots_0_to_3(),
        &[BitStep::Flip],
        &[BitStep::Read(0)],
    );
}

/// An executor that makes every shot again when a call names a shot new to
/// it breaks the rule, and is told so, not that its inputs fall short: the
/// call on shots 1 and 3 starts shots 0 and 2 again, which then read zero,
/// where they read one when that call does not run. The check names both.
#[test]
#[should_panic(
    expected = "a call must change no shot it does not name, but a shot reads otherwise after \
                a call on the shots [1, 3], on shots 0, 2"
)]
fn changes_no_shot_a_call_does_not_name_catches_shots_made_again_when_one_joins() {
    changes_no_shot_a_call_does_not_name(
        || TestDoubleOfBits::new(BitsBehaviour::RestartsItsShotsWhenOneJoins),
        &shots_0_to_3(),
        &[BitStep::Flip],
        &[BitStep::Read(0)],
    );
}

/// A toss sets a shot's bit to the parity of its first draw, which is the
/// shot's number: of shots 1 to 4, it changes 1 and 3, at even indices,
/// but leaves 2 and 4 as they were made, so a call that changed them
/// before their first call would not show: the check needs `first` to
/// change a new shot, on each shot at an odd index. `first` reads the bit
/// too, which differs from what `second` reads alone, so the need is
/// judged on `second`'s readouts only.
#[test]
#[should_panic(
    expected = "`first` must change a new shot, and `second` read that change, so that a \
                call that changes a shot not held yet would show: after `first`, `second` \
                must read otherwise than alone on each shot at an odd index, but it reads the \
                same on shots 2, 4"
)]
fn changes_no_shot_a_call_does_not_name_needs_first_to_change_a_new_shot() {
    changes_no_shot_a_call_does_not_name(
        || TestDoubleOfBits::new(BitsBehaviour::Sound),
        &ShotSet::range(1..=4).expect("four shots"),
        &[BitStep::Toss, BitStep::Read(1)],
        &[BitStep::Read(0)],
    );
}

/// An executor that also runs a call on the shots it holds, checked with a
/// `first` that sets the bit and reads it: `first` run twice reads under
/// its own tag where run once it does not, but `second` reads the same
/// after either, so the check needs `first` to change a shot each time it
/// runs, judged on `second`'s readouts only.
#[test]
#[should_panic(
    expected = "`first` must change a shot each time it runs (a flip, not a set), and \
                `second` read that change"
)]
fn changes_no_shot_a_call_does_not_name_needs_second_to_read_the_change_though_first_reads() {
    changes_no_shot_a_call_does_not_name(
        || TestDoubleOfBits::new(BitsBehaviour::RunsHeldShotsNotNamed),
        &shots_0_to_3(),
        &[BitStep::Set, BitStep::Read(1)],
        &[BitStep::Read(0)],
    );
}

/// Of three shots, the one at an odd index is a range of its own, so an
/// executor that runs a call on the range of its shots would not show: the
/// check needs four shots.
#[test]
#[should_panic(
    expected = "`shots` must hold four shots or more, so that the shots at odd \
                           indices are not contiguous"
)]
fn changes_no_shot_a_call_does_not_name_needs_four_shots() {
    changes_no_shot_a_call_does_not_name(
        || TestDoubleOfBits::new(BitsBehaviour::RunsTheRangeOfItsShots),
        &ShotSet::range(0..=2).expect("three shots"),
        &[BitStep::Flip],
        &[BitStep::Read(0)],
    );
}

/// A set leaves a shot as it was when run again, so an executor that also
/// runs a call on the shots it holds but the call does not name, which
/// sets shots 0 and 2 again as it sets 1 and 3, would not show: the check
/// needs a `first` that changes a shot each time it runs.
#[test]
#[should_panic(
    expected = "`first` must change a shot each time it runs (a flip, not a set), and \
                `second` read that change"
)]
fn changes_no_shot_a_call_does_not_name_needs_first_to_change_a_shot_each_time_it_runs() {
    changes_no_shot_a_call_does_not_name(
        || TestDoubleOfBits::new(BitsBehaviour::RunsHeldShotsNotNamed),
        &shots_0_to_3(),
        &[BitStep::Set],
        &[BitStep::Read(0)],
    );
}

/// An executor that keeps shot 0's bit from one call to the next, but
/// makes the others' again at every call, reads shots 2 and 4 the same
/// however often `first` flips them, so a call run again on them would not
/// show there: the check needs the change on each shot at an even index,
/// not only on shot 0, and names every shot without it.
#[test]
#[should_panic(expected = "on each shot at an even index, but it reads the same on shots 2, 4")]
fn changes_no_shot_a_call_does_not_name_needs_the_change_on_each_shot_it_checks() {
    changes_no_shot_a_call_does_not_name(
        || TestDoubleOfBits::new(BitsBehaviour::KeepsOnlyShot0),
        &ShotSet::range(0..=5).expect("six shots"),
        &[BitStep::Flip],
        &[BitStep::Read(0)],
    );
}

/// Executors that run one after another on a device that keeps each
/// shot's bit as the last executor left it, not reset: the one built after
/// an executor tossed shots 2 to 5, which changes shots 3 and 5, reads
/// them changed, where the one built before reads them as made. The check
/// names both.
#[test]
#[should_panic(
    expected = "a new executor's shots must start as made, whatever an earlier executor left, \
                but a shot reads otherwise after an earlier executor ran `first` on it, on \
                shots 3, 5"
)]
fn starts_its_shots_as_made_whatever_an_earlier_executor_left_catches_a_device_not_reset() {
    let device = Device::default();
    starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || TestDoubleOfBits::on(&device),
        &ShotSet::range(2..=5).expect("four shots"),
        &[BitStep::Toss],
        &[BitStep::Read(0)],
    );
}

/// Executors that start from the shots a device holds, and leave their own
/// there, leave a shot's conditions too: `first` rejects shots 1 and 3
/// and changes no bit, but the next executor's readouts of them carry the
/// rejection. The check compares the conditions with the readouts, and
/// names both.
#[test]
#[should_panic(
    expected = "a new executor's shots must start as made, whatever an earlier executor left, \
                but a shot reads otherwise after an earlier executor ran `first` on it, on \
                shots 1, 3"
)]
fn starts_its_shots_as_made_whatever_an_earlier_executor_left_catches_conditions_left_behind() {
    let device = Device::default();
    starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || TestDoubleOfBits::on(&device),
        &shots_0_to_3(),
        &[BitStep::RejectIfOdd],
        &[BitStep::Read(0)],
    );
}

/// Executors that run on a device's shots themselves, and reset it when
/// they close: each closed executor leaves the shots as made, but one
/// dropped unclosed (by a caller that panics, or returns early) leaves
/// them flipped, and the next reads them so. The check names every shot.
#[test]
#[should_panic(
    expected = "a new executor's shots must start as made, whatever an earlier executor left, \
                but a shot reads otherwise after an earlier executor ran `first` on it and was \
                dropped unclosed, on shots 0, 1, 2, 3"
)]
fn starts_its_shots_as_made_whatever_an_earlier_executor_left_catches_a_device_reset_at_close_only()
{
    let device = Device::default();
    starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || TestDoubleOfBits::reset_at_close_only(&device),
        &shots_0_to_3(),
        &[BitStep::Flip],
        &[BitStep::Read(0)],
    );
}

/// A toss sets a shot's bit to the parity of its first draw, which is the
/// shot's number: of shots 2 to 5, it leaves 2 and 4 as they were made, so
/// a device not reset between executors would not show on them: the check
/// needs `second` to read, after `first` in one batch of both, otherwise
/// than alone on each shot, and names every shot where it does not.
#[test]
#[should_panic(
    expected = "`second` must read what `first` changed, so that a shot left changed by an \
                earlier executor would show: after `first`, in one batch of both, it must \
                read otherwise than alone on each shot, but it reads the same on shots 2, 4"
)]
fn starts_its_shots_as_made_whatever_an_earlier_executor_left_needs_first_to_change_a_shot() {
    starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || TestDoubleOfBits::new(BitsBehaviour::Sound),
        &ShotSet::range(2..=5).expect("four shots"),
        &[BitStep::Toss],
        &[BitStep::Read(0)],
    );
}

/// A `first` and a `second` that read under one tag cannot be joined into
/// one batch, which a sound executor refuses: the check needs `second` to
/// read under other tags, and says so before it runs the batch. A `first`
/// that reads under tag 0 on odd shots only clashes with `second` on shots
/// 3 and 5 of shots 2 to 5, and the check names both.
#[test]
#[should_panic(
    expected = "`first` and `second` must use different tags, so that one batch of both is \
                accepted, but they read under a tag in both on shots 3, 5"
)]
fn starts_its_shots_as_made_whatever_an_earlier_executor_left_needs_second_to_read_under_other_tags()
 {
    starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || TestDoubleOfBits::new(BitsBehaviour::Sound),
        &ShotSet::range(2..=5).expect("four shots"),
        &[BitStep::Flip, BitStep::ReadIfOdd(0)],
        &[BitStep::Read(0)],
    );
}

/// An executor that runs a batch that reads twice under one tag: a `first`
/// that only reads, under `second`'s tag, changes nothing, yet the batch of
/// both reads under that tag after `first` too, which would pass the need
/// that `second` read a change. The check needs other tags whether the
/// executor refuses the batch or not.
#[test]
#[should_panic(
    expected = "`first` and `second` must use different tags, so that one batch of both is \
                accepted, but they read under a tag in both on shots 0, 1, 2, 3"
)]
fn starts_its_shots_as_made_whatever_an_earlier_executor_left_needs_other_tags_where_a_repeat_runs()
{
    starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || TestDoubleOfBits::new(BitsBehaviour::AcceptsARepeatedTag),
        &shots_0_to_3(),
        &[BitStep::Read(0)],
        &[BitStep::Read(0)],
    );
}

/// A code's runtime rejects shot 1 in a call, and reads it in the next:
/// an executor that forgets its shots' conditions at each call gives the
/// later readout clear. The check names the shot.
#[test]
#[should_panic(
    expected = "each outcome of a shot must carry the conditions its outcomes in earlier calls \
                carried, since a condition stays raised for its shot, but one drops a condition \
                on shot 1"
)]
fn keeps_each_shots_conditions_from_one_call_to_the_next_catches_conditions_forgotten_at_a_call() {
    keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut TestDoubleOfBits::new(BitsBehaviour::ForgetsConditions),
        &shots(&[0, 1, 2]),
        &[
            &[BitStep::RejectIfOdd, BitStep::Read(0)],
            &[BitStep::Read(1)],
        ],
    );
}

/// A code's runtime rejects shot 1 in a call, and reads it twice in the
/// next: an executor that gives the condition on the shot's last readout
/// of a call alone gives the earlier readout clear. The check looks at
/// every outcome of the shot, not only its last, and names the shot.
#[test]
#[should_panic(
    expected = "each outcome of a shot must carry the conditions its outcomes in earlier calls \
                carried, since a condition stays raised for its shot, but one drops a condition \
                on shot 1"
)]
fn keeps_each_shots_conditions_from_one_call_to_the_next_catches_a_condition_dropped_on_an_earlier_outcome()
 {
    keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut TestDoubleOfBits::new(BitsBehaviour::MarksTheLastReadoutOfAShotAlone),
        &shots(&[0, 1, 2]),
        &[
            &[BitStep::RejectIfOdd, BitStep::Read(0)],
            &[BitStep::Read(1), BitStep::Read(2)],
        ],
    );
}

/// A code's runtime rejects shot 1 in a call, and reads it twice in the
/// next, under tag 2 then tag 1: an executor that gives the condition on
/// the shot's last readout of a call alone gives the readout under tag 2,
/// first in its reply, clear, though the one under tag 1 carries it. The
/// check looks at each outcome of the shot, whatever their order, not at
/// whether another of its call carries the condition, and names the shot.
#[test]
#[should_panic(
    expected = "each outcome of a shot must carry the conditions its outcomes in earlier calls \
                carried, since a condition stays raised for its shot, but one drops a condition \
                on shot 1"
)]
fn keeps_each_shots_conditions_from_one_call_to_the_next_catches_a_condition_one_outcome_of_a_call_drops_while_another_carries_it()
 {
    keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut TestDoubleOfBits::new(BitsBehaviour::MarksTheLastReadoutOfAShotAlone),
        &shots(&[0, 1, 2]),
        &[
            &[BitStep::RejectIfOdd, BitStep::Read(0)],
            &[BitStep::Read(2), BitStep::Read(1)],
        ],
    );
}

/// A shot rejected between two readouts of a call carries the condition on
/// the second only, tagged lower than the first, and on its readouts of
/// later calls: the rule holds from one call to the next, not within a
/// call, whose readouts a caller takes together, whatever their tags.
#[test]
fn keeps_each_shots_conditions_from_one_call_to_the_next_lets_a_call_raise_one_between_readouts() {
    keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut TestDoubleOfBits::new(BitsBehaviour::Sound),
        &shots(&[0, 1, 2]),
        &[
            &[BitStep::Read(1), BitStep::RejectIfOdd, BitStep::Read(0)],
            &[BitStep::Read(2)],
        ],
    );
}

/// A code's runtime reads shot 1 clear under tag 2 in a first call,
/// rejects it and reads it under tag 0 in a second, and under tag 1 in a
/// third: the check takes the calls in the order they ran, whatever their
/// tags, so the clear readout of the first call, tagged after the later
/// ones, drops nothing.
#[test]
fn keeps_each_shots_conditions_from_one_call_to_the_next_takes_the_calls_in_their_order_whatever_their_tags()
 {
    keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut TestDoubleOfBits::new(BitsBehaviour::Sound),
        &shots(&[0, 1, 2]),
        &[
            &[BitStep::Read(2)],
            &[BitStep::RejectIfOdd, BitStep::Read(0)],
            &[BitStep::Read(1)],
        ],
    );
}

/// A shot rejected in the last call only has no later outcome to carry
/// the condition, so there is nothing to keep: the check needs batches
/// that give some shot an outcome that carries a condition before a later
/// call reads the shot.
#[test]
#[should_panic(
    expected = "`batches` must give some shot an outcome that carries a condition before a later \
                call reads that shot, so that there is a condition to keep"
)]
fn keeps_each_shots_conditions_from_one_call_to_the_next_needs_a_condition_to_keep() {
    keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut TestDoubleOfBits::new(BitsBehaviour::ForgetsConditions),
        &shots(&[0, 1, 2]),
        &[
            &[BitStep::Read(0)],
            &[BitStep::RejectIfOdd, BitStep::Read(1)],
        ],
    );
}
