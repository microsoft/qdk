// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An `adapter_fn` keeps the rules of every executor of the crate, and
//! reaches the executor below through `Below`: each send one call below
//! whose reply passes up, each run one whose readouts its logic reads.
//! The rules of `Outcomes`' `Extend<ShotOutcome>` (a readout answers its
//! own shot alone; one of a shot the state does not serve panics) are
//! tested here too, though `Outcomes` lives with `executor_fn`: they serve
//! a logic that writes, by shot, what it read below, and only an
//! `adapter_fn`'s logic reads below.

use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::ops::Shr;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::task::Waker;
use std::thread;
use std::time::Duration;

use crate::tests::executor_fn::{
    Bit, Freed, Op, PowerDownFailed, PowersDownBadly, ReleasesBadly, draw_value, freed_so_far,
    panic_text, powering_down_badly, promise, read_drawn_bits, readouts, run_bit,
};
use crate::tests::placement::{self as placed, Threads, polled_now, recording};
use crate::tests::test_double::{TestDoubleExecutor, TestDoubleModel, run, step};
use crate::tests::{
    Bits, PATIENT, PanicsOnClose, bits_set, call, ends_soon, flip_and_toss, reversed_set, shots,
    values,
};
use crate::{
    AdapterError, AdapterFn, Below, BuildError, Capabilities, Close, ComponentId, Compose,
    Describe, Executor, ExecutorFn, ExecutorLogic, Fold, ImmediateReply, IncrementalReply,
    InstructionBatch, InstructionBatchRef, InstructionCall, InstructionMismatch, InstructionSet,
    JobFaults, MakeState, Need, Needs, OnePerShot, OnePerShotAdapter, OnePerShotExecutor, Operand,
    OperandKind, Outcome, Outcomes, OwnThread, ProgramInput, Rejected, Reply, ReplySender,
    ReplyStatus, ShotContext, ShotId, ShotOutcome, ShotSet, StaticProgram, TypedInstruction, Value,
    WithExecutor, adapter_fn, check_unique_tags, compose, conformance, driver, executor_fn,
    reply_channel, wait,
};

use OperandKind::{Block, Tag};

/// The tests' one-bit device ([`Op`]), a state per shot, promising
/// `promise`: it flips and reads each shot's bit, and fails a call at a
/// stop (`the device stopped`).
fn device_promising(
    promise: Capabilities,
) -> OnePerShotExecutor<Op, Bit, impl MakeState<Bit>, impl ExecutorLogic<Op, Bit>> {
    executor_fn(promise, Bit::new, run_bit).shots(OnePerShot)
}

/// The tests' one-bit device ([`device_promising`]), promising the tests'
/// device's promise.
fn device() -> OnePerShotExecutor<Op, Bit, impl MakeState<Bit>, impl ExecutorLogic<Op, Bit>> {
    device_promising(promise())
}

mod one_per_shot;
mod owning;

/// A pulse of a device whose driver fails to release its channel when a
/// copy of it is dropped (a bug): a copy's drop counts itself in
/// `released`, then panics; the original's drops quietly. One made
/// [`Pulse::failing_to_copy`] panics as it is copied. Pulses on one
/// channel are equal.
#[derive(Debug)]
pub(super) struct Pulse {
    channel: u8,
    copy: bool,
    copy_fails: bool,
    released: Rc<Cell<usize>>,
}

impl Pulse {
    pub(super) fn on(channel: u8, released: &Rc<Cell<usize>>) -> Self {
        Self {
            channel,
            copy: false,
            copy_fails: false,
            released: Rc::clone(released),
        }
    }

    pub(super) fn failing_to_copy(channel: u8, released: &Rc<Cell<usize>>) -> Self {
        let mut pulse = Self::on(channel, released);
        pulse.copy_fails = true;
        pulse
    }
}

impl PartialEq for Pulse {
    fn eq(&self, other: &Self) -> bool {
        self.channel == other.channel
    }
}

impl Clone for Pulse {
    fn clone(&self) -> Self {
        assert!(!self.copy_fails, "a pulse's copy has a bug");
        Self {
            channel: self.channel,
            copy: true,
            copy_fails: false,
            released: Rc::clone(&self.released),
        }
    }
}

impl Drop for Pulse {
    fn drop(&mut self) {
        if self.copy {
            self.released.set(self.released.get() + 1);
            panic!("a pulse's copy fails to release channel {}", self.channel);
        }
    }
}

impl TypedInstruction for Pulse {}

/// A device of pulses, of no state, that runs any.
pub(super) fn pulse_device()
-> ExecutorFn<Pulse, (), impl MakeState<()>, impl ExecutorLogic<Pulse, ()>> {
    executor_fn(
        Capabilities::default(),
        |_| Ok(()),
        |(): &mut (), _: &ProgramInput<'_, Pulse>, _: &mut Outcomes<'_>| Ok(()),
    )
}

/// The state of an adapter that keeps none.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature a state closure has"
)]
fn no_state(_: ShotContext) -> Result<(), Rejected> {
    Ok(())
}

/// The logic of an observer that sends each call down as it is.
fn pass(
    (): &mut (),
    input: &ProgramInput<'_, bool>,
    below: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    below.send(input.instructions())
}

/// The tests' executor of `model`, seeded with 7.
fn below(model: TestDoubleModel) -> TestDoubleExecutor<TestDoubleModel> {
    TestDoubleExecutor::with_model(model, 7)
}

/// An observer that sends each call down as it is ([`pass`]), upstream of
/// the tests' executor of `model`.
fn passing_over(
    model: TestDoubleModel,
) -> impl Executor<InstructionBatch<bool>> + Close<Report = impl std::fmt::Debug + PartialEq> {
    compose(adapter_fn(no_state, pass), below(model)).expect("bools fit bools")
}

/// The calls an executor is called with, call after call: each call's
/// shots and instructions, shared with the test.
type Calls = Rc<RefCell<Vec<(Vec<ShotId>, Vec<bool>)>>>;

/// An executor, around `E`, that records each call it is called with: its
/// shots and its instructions.
struct Recorded<E> {
    executor: E,
    calls: Calls,
}

/// `executor`, recorded, and the calls it is called with.
fn recorded<E>(executor: E) -> (Recorded<E>, Calls) {
    let calls = Rc::default();
    let recording = Recorded {
        executor,
        calls: Rc::clone(&calls),
    };
    (recording, calls)
}

impl<E: Executor<InstructionBatch<bool>>> Executor<InstructionBatch<bool>> for Recorded<E> {
    type Reply = E::Reply;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<E::Reply, Rejected> {
        let call = (batch.shots.to_vec(), batch.instructions.to_vec());
        self.calls.borrow_mut().push(call);
        self.executor.call(batch)
    }
}

impl<E: Describe> Describe for Recorded<E> {
    fn capabilities(&self) -> Capabilities {
        self.executor.capabilities()
    }
}

impl<E: Close> Close for Recorded<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.executor.close()
    }
}

/// `>>` with a recorded executor on the right, as an executor's crate
/// implements it.
macro_rules! recorded_by_shr {
    ($adapter:ident) => {
        impl<I, O, S, M, L, C, R, E> Shr<Recorded<E>> for $adapter<I, O, S, M, L, C, R>
        where
            Self: Compose<Recorded<E>, WithExecutor>,
        {
            type Output = <Self as Compose<Recorded<E>, WithExecutor>>::Output;

            fn shr(self, executor: Recorded<E>) -> Self::Output {
                self.compose(executor)
            }
        }
    };
}

recorded_by_shr!(AdapterFn);
recorded_by_shr!(OnePerShotAdapter);

/// A profiler that passes three shots' calls down gives the outcomes they
/// give with no adapter, call after call: what it sends reads the same
/// bits, of the same shots, tags kept.
#[test]
fn a_logic_that_sends_each_call_gives_the_outcomes_of_no_adapter() {
    let three = shots(&[0, 1, 2]);
    let batches: [&[bool]; 2] = [&[true, true], &[false, true]];
    let mut observed = passing_over(TestDoubleModel::default());
    let mut bare = below(TestDoubleModel::default());
    for batch in batches {
        let (through, end) = run(&mut observed, &three, batch).expect("the adapter accepts it");
        end.expect("the call ends well through the adapter");
        let (alone, end) = run(&mut bare, &three, batch).expect("the executor accepts it");
        end.expect("the call ends well alone");
        assert!(!through.is_empty(), "the batch reads bits");
        assert_eq!(values(&through), values(&alone));
    }
}

/// The bits' instructions, numbered by a set that names the flip `flip`
/// and puts it first.
fn flip_first_set() -> InstructionSet {
    InstructionSet::new("flip first")
        .with("flip", [Block])
        .with("coin", [Block])
        .with("measure", [Block, Tag])
}

/// [`flip_and_toss`], written in [`flip_first_set`].
fn flip_and_toss_flip_first() -> Vec<Vec<InstructionCall>> {
    let set = flip_first_set();
    vec![
        vec![
            call(&set, "flip", &[Operand::Block(0)]),
            call(&set, "measure", &[Operand::Block(0), Operand::Tag(0)]),
        ],
        vec![
            call(&set, "coin", &[Operand::Block(1)]),
            call(&set, "measure", &[Operand::Block(1), Operand::Tag(1)]),
        ],
    ]
}

/// The logic of a lowering from [`flip_first_set`] to [`reversed_set`],
/// the bits' instructions numbered otherwise than the bits number them:
/// each call is sent as the call of the same name, `flip` as `x`, operands
/// and tags kept.
fn lower(
    (): &mut (),
    input: &ProgramInput<'_, InstructionCall>,
    below: &mut Below<'_, InstructionCall>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    let (from, to) = (flip_first_set(), reversed_set());
    let lowered: Vec<InstructionCall> = (input.instructions().iter())
        .map(|call| {
            let mnemonic = from.mnemonic(call.opcode).expect("a call of the set");
            let mnemonic = if mnemonic == "flip" { "x" } else { mnemonic };
            to.call(mnemonic, call.operands.iter().copied())
                .expect("the bits have each instruction")
        })
        .collect();
    below.send(&lowered)
}

/// A lowering that renames the flip and numbers the calls otherwise, run
/// on two shots, gives what the program written in the bits' own set gives
/// alone: each call reaches the bits as the instruction it names, though
/// the set it sends numbers them otherwise than the bits, and each read
/// keeps its tag.
#[test]
fn a_logic_that_rewrites_each_call_keeps_its_tags() {
    let lowering = adapter_fn(no_state, lower)
        .accepts(flip_first_set())
        .sends(reversed_set());
    let mut lowered = compose(lowering, TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits have every instruction it sends");
    let mut bare = TestDoubleExecutor::with_model(Bits::default(), 7);
    let two = shots(&[0, 1]);
    for (program, written) in flip_and_toss_flip_first()
        .iter()
        .zip(flip_and_toss(&bits_set()))
    {
        let through = values(&outcomes(&mut lowered, &two, program));
        assert!(!through.is_empty(), "each batch reads a bit");
        assert_eq!(through, values(&outcomes(&mut bare, &two, &written)));
    }
}

/// The outcomes of `batch`, run on `shots` of `executor`, which accepts the
/// batch and does not fail it.
fn outcomes<E: Executor<InstructionBatch<InstructionCall>>>(
    executor: &mut E,
    shots: &ShotSet,
    batch: &[InstructionCall],
) -> Vec<crate::ShotOutcome> {
    let reply = executor
        .call(InstructionBatchRef {
            shots,
            instructions: batch,
        })
        .expect("the executor accepts the batch");
    let mut out = Vec::new();
    wait(reply, &mut out).expect("the batch does not fail");
    out
}

/// A profiler that counts the calls it sees in its state: called twice on
/// three shots, it makes its state once and runs once per call, not once
/// per shot.
#[test]
fn one_state_serves_every_call_of_every_shot() {
    let made = Rc::new(Cell::new(0));
    let counts = Rc::new(RefCell::new(Vec::new()));
    let profiler = {
        let (made, counts) = (Rc::clone(&made), Rc::clone(&counts));
        adapter_fn(
            move |_| {
                made.set(made.get() + 1);
                Ok(0)
            },
            move |calls: &mut u32,
                  input: &ProgramInput<'_, bool>,
                  below: &mut Below<'_, bool>,
                  _: &mut Outcomes<'_>| {
                *calls += 1;
                counts.borrow_mut().push(*calls);
                below.send(input.instructions())
            },
        )
    };
    let mut executor = compose(profiler, below(TestDoubleModel::default())).expect("bools fit");
    let three = shots(&[0, 1, 2]);
    step(&mut executor, &three, &[true], "the first call runs");
    step(&mut executor, &three, &[true], "the second call runs");
    assert_eq!(made.get(), 1, "one state");
    assert_eq!(*counts.borrow(), [1, 2], "the logic ran once per call");
}

/// Shot 0's executor below is busy: a call on it is refused with the
/// refusal below, as it is with no adapter, and the adapter stays open for
/// shot 1.
#[test]
fn a_call_whose_send_is_refused_is_refused_with_the_refusal_below() {
    let mut executor = passing_over(TestDoubleModel {
        refusing: Some(0),
        ..TestDoubleModel::default()
    });
    let refused = run(&mut executor, &ShotSet::one(0), &[true]).expect_err("shot 0 is busy");
    assert_eq!(refused, Rejected::Unavailable("busy".into()));
    step(&mut executor, &ShotSet::one(1), &[true], "shot 1 runs");
}

/// A profiler that counts the states it makes, upstream of an executor
/// whose shot 0 is busy, and the count.
fn counting_states_over(
    model: TestDoubleModel,
) -> (impl Executor<InstructionBatch<bool>>, Rc<Cell<usize>>) {
    let made = Rc::new(Cell::new(0));
    let counting = {
        let made = Rc::clone(&made);
        adapter_fn(
            move |_| {
                made.set(made.get() + 1);
                Ok(())
            },
            pass,
        )
    };
    let executor = compose(counting, below(model)).expect("bools fit");
    (executor, made)
}

/// The first call is refused below: the state made for it is dropped, so
/// the next call makes one afresh, as if the refused call never came.
#[test]
fn a_refused_call_keeps_no_state_made_for_it() {
    let (mut executor, made) = counting_states_over(TestDoubleModel {
        refusing: Some(0),
        ..TestDoubleModel::default()
    });
    run(&mut executor, &ShotSet::one(0), &[true]).expect_err("shot 0 is busy");
    step(&mut executor, &ShotSet::one(1), &[true], "shot 1 runs");
    assert_eq!(made.get(), 2, "the refused call's state was dropped");
}

/// A call runs, then one is refused below: the state the first call made
/// stays, and serves the call after.
#[test]
fn a_refused_call_keeps_the_state_made_before_it() {
    let (mut executor, made) = counting_states_over(TestDoubleModel {
        refusing: Some(0),
        ..TestDoubleModel::default()
    });
    step(&mut executor, &ShotSet::one(1), &[true], "shot 1 runs");
    run(&mut executor, &ShotSet::one(0), &[true]).expect_err("shot 0 is busy");
    step(
        &mut executor,
        &ShotSet::one(1),
        &[true],
        "shot 1 runs again",
    );
    assert_eq!(made.get(), 1, "the state made first was kept");
}

/// Two loggers whose send is refused below, one that returns `Ok` anyway
/// and one that passes the refusal on as its `Err`: both calls are
/// refused, with the refusal below, since nothing went down.
#[test]
fn a_refused_first_send_refuses_the_call_whatever_the_logic_returns() {
    let busy = || TestDoubleModel {
        refusing: Some(0),
        ..TestDoubleModel::default()
    };
    let ignoring = |(): &mut (),
                    input: &ProgramInput<'_, bool>,
                    below: &mut Below<'_, bool>,
                    _: &mut Outcomes<'_>| {
        let _ = below.send(input.instructions());
        Ok(())
    };
    let mut ignores = compose(adapter_fn(no_state, ignoring), below(busy())).expect("bools fit");
    let mut passes_on = passing_over(busy());
    let when_ignored = run(&mut ignores, &ShotSet::one(0), &[true]).expect_err("shot 0 is busy");
    let when_passed_on =
        run(&mut passes_on, &ShotSet::one(0), &[true]).expect_err("shot 0 is busy");
    assert_eq!(when_ignored, Rejected::Unavailable("busy".into()));
    assert_eq!(when_passed_on, Rejected::Unavailable("busy".into()));
}

/// A lowering that reads a bit before each call, called with a skip alone,
/// over an executor that refuses a skip: the refusal names the skip at
/// index 1, where it is in the call below, though the call holds one
/// instruction. The logic cannot map it to its call's index yet
/// (planned).
#[test]
fn a_refusal_below_names_the_instruction_as_sent() {
    let reading_first = |(): &mut (),
                         input: &ProgramInput<'_, bool>,
                         below: &mut Below<'_, bool>,
                         _: &mut Outcomes<'_>| {
        let lowered: Vec<bool> = [true].iter().chain(input.instructions()).copied().collect();
        below.send(&lowered)
    };
    let model = TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    };
    let mut lowered =
        compose(adapter_fn(no_state, reading_first), below(model)).expect("bools fit");
    let refused = run(&mut lowered, &ShotSet::one(0), &[false]).expect_err("a skip below");
    assert_eq!(
        refused,
        Rejected::Instruction {
            at: 1,
            reason: "not a read".into()
        }
    );
}

/// Shot 0's state below cannot be made the first time, then can: a logic
/// that sends again after its send was refused is refused at once, with
/// the same reason, and nothing goes down, so no state is made below.
#[test]
fn a_send_after_a_refused_one_does_not_go_down() {
    let model = TestDoubleModel {
        unavailable_once: Cell::new(Some(0)),
        ..TestDoubleModel::default()
    };
    let made_below = Rc::clone(&model.made);
    let results = Rc::new(RefCell::new(Vec::new()));
    let retrying = {
        let results = Rc::clone(&results);
        adapter_fn(
            no_state,
            move |(): &mut (),
                  input: &ProgramInput<'_, bool>,
                  below: &mut Below<'_, bool>,
                  _: &mut Outcomes<'_>| {
                let first = below.send(input.instructions());
                let again = below.send(input.instructions());
                results.borrow_mut().extend([first, again]);
                Ok(())
            },
        )
    };
    let mut executor = compose(retrying, below(model)).expect("bools fit");
    run(&mut executor, &ShotSet::one(0), &[true]).expect_err("shot 0 has no memory");
    let results = results.borrow();
    assert!(results[0].is_err(), "the first send is refused");
    assert_eq!(results[1], results[0], "the second, with the same reason");
    assert_eq!(made_below.get(), 0, "nothing went down");
}

/// A logic that sends a read, then a batch the executor below refuses: the
/// read stands, so the call is not refused but fails, after the read, and
/// so does the adapter.
#[test]
fn a_send_refused_after_another_fails_the_call_and_the_adapter() {
    let model = TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    };
    let twice = |(): &mut (),
                 _: &ProgramInput<'_, bool>,
                 below: &mut Below<'_, bool>,
                 _: &mut Outcomes<'_>| {
        below.send(&[true])?;
        below.send(&[false])
    };
    let mut executor = compose(adapter_fn(no_state, twice), below(model)).expect("bools fit");
    let (read, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("the read went down");
    assert_eq!(read.len(), 1, "the read stands: {read:?}");
    end.expect_err("the second send was refused");
    let later = run(&mut executor, &ShotSet::one(0), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// A decoder whose logic fails on a call: the call ends failed with its
/// reason, and every later call is refused for it.
#[test]
fn a_logic_that_fails_fails_the_adapter() {
    let failing = |(): &mut (),
                   _: &ProgramInput<'_, bool>,
                   _: &mut Below<'_, bool>,
                   _: &mut Outcomes<'_>| { Err("no syndrome".into()) };
    let mut executor = compose(
        adapter_fn(no_state, failing),
        below(TestDoubleModel::default()),
    )
    .expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("the call is accepted");
    assert_eq!(end, Err("no syndrome".to_owned()));
    let later = run(&mut executor, &ShotSet::one(1), &[true]).expect_err("the adapter failed");
    assert_eq!(later, Rejected::EarlierFailure("no syndrome".into()));
}

/// A decoder whose logic fails on a call whose reply is dropped unread: the
/// adapter fails all the same, as the call fails, so a later call is
/// refused.
#[test]
fn a_logic_that_fails_fails_the_adapter_though_its_reply_is_dropped() {
    let failing = |(): &mut (),
                   _: &ProgramInput<'_, bool>,
                   _: &mut Below<'_, bool>,
                   _: &mut Outcomes<'_>| { Err("no syndrome".into()) };
    let mut executor = compose(
        adapter_fn(no_state, failing),
        below(TestDoubleModel::default()),
    )
    .expect("bools fit");
    let reply = executor.call(InstructionBatchRef {
        shots: &ShotSet::one(0),
        instructions: &[true],
    });
    drop(reply.expect("the call is accepted"));
    let later = run(&mut executor, &ShotSet::one(1), &[true]).expect_err("the adapter failed");
    assert_eq!(later, Rejected::EarlierFailure("no syndrome".into()));
}

/// A logic that sends a read, then a batch the executor below refuses,
/// called with its reply dropped unread: the adapter fails all the same,
/// as the call fails below, so a later call is refused.
#[test]
fn a_call_failed_below_fails_the_adapter_though_its_reply_is_dropped() {
    let model = TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    };
    let twice = |(): &mut (),
                 _: &ProgramInput<'_, bool>,
                 below: &mut Below<'_, bool>,
                 _: &mut Outcomes<'_>| {
        below.send(&[true])?;
        below.send(&[false])
    };
    let mut executor = compose(adapter_fn(no_state, twice), below(model)).expect("bools fit");
    let reply = executor.call(InstructionBatchRef {
        shots: &ShotSet::one(0),
        instructions: &[true],
    });
    drop(reply.expect("the read went down"));
    let later = run(&mut executor, &ShotSet::one(0), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// Shot 0's batches fail below: the call's reply fails with the failure
/// below, and the adapter fails too, so a call on shot 1 is refused.
#[test]
fn a_failure_below_fails_the_adapter() {
    let mut executor = passing_over(TestDoubleModel {
        failing: Some(0),
        ..TestDoubleModel::default()
    });
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("shot 0 accepts");
    assert_eq!(end, Err("lost".to_owned()));
    let later = run(&mut executor, &ShotSet::one(1), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// The logic of an observer that sends each call down, unless it has no
/// instructions: an empty call is answered without going down.
fn pass_unless_empty(
    (): &mut (),
    input: &ProgramInput<'_, bool>,
    below: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    if input.instructions().is_empty() {
        return Ok(());
    }
    below.send(input.instructions())
}

/// Shot 0's state below failed in a call whose reply did not show it: the
/// call is refused for that failure, and the adapter fails with it, unlike
/// a refusal for a shortage below
/// (`a_call_whose_send_is_refused_is_refused_with_the_refusal_below`): a
/// later call that would not go down is refused too.
#[test]
fn a_refusal_below_for_an_earlier_failure_fails_the_adapter() {
    let model = TestDoubleModel {
        failed_unseen: Some(0),
        ..TestDoubleModel::default()
    };
    let mut executor =
        compose(adapter_fn(no_state, pass_unless_empty), below(model)).expect("bools fit");
    let refused = run(&mut executor, &ShotSet::one(0), &[true]).expect_err("shot 0 failed");
    assert_eq!(refused, Rejected::EarlierFailure("lost".into()));
    let later = run(&mut executor, &ShotSet::one(1), &[]).expect_err("the adapter failed");
    assert_eq!(later, Rejected::EarlierFailure("lost".into()));
}

/// Shot 0's batches are cancelled below: the call's reply ends cancelled,
/// after the read.
#[test]
fn a_reply_below_cancelled_ends_the_call_cancelled() {
    let mut executor = passing_over(TestDoubleModel {
        cancelling: Some(0),
        ..TestDoubleModel::default()
    });
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[true],
        })
        .expect("shot 0 accepts");
    let mut out = Vec::new();
    let end = reply.poll_events(&mut out, Waker::noop());
    assert_eq!(end, ReplyStatus::Cancelled);
    assert_eq!(out.len(), 1, "the read stands: {out:?}");
}

/// A decoder sends the call down, below which it is cancelled, then fails
/// itself: the call ends with the decoder's failure, which says more than
/// the cancel.
#[test]
fn a_logic_that_fails_outranks_a_send_cancelled() {
    let failing = |(): &mut (),
                   input: &ProgramInput<'_, bool>,
                   below: &mut Below<'_, bool>,
                   _: &mut Outcomes<'_>| {
        below.send(input.instructions())?;
        Err("no syndrome".into())
    };
    let model = TestDoubleModel {
        cancelling: Some(0),
        ..TestDoubleModel::default()
    };
    let mut executor = compose(adapter_fn(no_state, failing), below(model)).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("shot 0 accepts");
    assert_eq!(end, Err("no syndrome".to_owned()));
}

/// A decoder sends the call down, where its reply waits, then fails
/// itself; the reply below fails once the call returned. The call ends with
/// the decoder's failure, the one its later calls are refused with, so the
/// caller learns one reason for the run's end.
#[test]
fn a_call_that_failed_ends_with_its_own_failure_though_a_send_fails_after_it_returned() {
    let failing = |(): &mut (),
                   input: &ProgramInput<'_, bool>,
                   below: &mut Below<'_, bool>,
                   _: &mut Outcomes<'_>| {
        below.send(input.instructions())?;
        Err("no syndrome".into())
    };
    let model = TestDoubleModel {
        deferred: vec![0],
        ..TestDoubleModel::default()
    };
    let seen = Rc::clone(&model.seen);
    let mut executor = compose(adapter_fn(no_state, failing), below(model)).expect("bools fit");
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[true],
        })
        .expect("shot 0 accepts");
    let sender = seen
        .borrow_mut()
        .replies
        .get_mut(&0)
        .expect("the send's reply waits below")
        .remove(0);
    sender.fail("lost".into());
    let end = reply.poll_events(&mut Vec::new(), Waker::noop());
    let later = run(&mut executor, &ShotSet::one(1), &[true]).expect_err("the adapter failed");
    assert_eq!(
        (end, later),
        (
            ReplyStatus::Failed("no syndrome".into()),
            Rejected::EarlierFailure("no syndrome".into())
        )
    );
}

/// Shot 0's batches are cancelled below, and an observer's reply is read
/// to its end: the adapter fails, so a later call is refused, though it
/// would not go down.
#[test]
fn a_send_cancelled_below_fails_the_adapter() {
    let model = TestDoubleModel {
        cancelling: Some(0),
        ..TestDoubleModel::default()
    };
    let mut executor =
        compose(adapter_fn(no_state, pass_unless_empty), below(model)).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("shot 0 accepts");
    end.expect_err("shot 0's send was cancelled");
    let later = run(&mut executor, &ShotSet::one(1), &[]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// Shot 0's batches are cancelled below, and an observer's reply is
/// dropped unread: the adapter fails all the same, as the call returns,
/// so a later call is refused, though it would not go down.
#[test]
fn a_send_cancelled_below_fails_the_adapter_though_its_reply_is_dropped() {
    let model = TestDoubleModel {
        cancelling: Some(0),
        ..TestDoubleModel::default()
    };
    let mut executor =
        compose(adapter_fn(no_state, pass_unless_empty), below(model)).expect("bools fit");
    let reply = executor.call(InstructionBatchRef {
        shots: &ShotSet::one(0),
        instructions: &[true],
    });
    drop(reply.expect("shot 0 accepts"));
    let later = run(&mut executor, &ShotSet::one(1), &[]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// Shot 0's batches fail below as they are called: a decoder that ignores
/// its send's error, sends again, then runs, is told at each why the call
/// cannot go on, the send's failure, and nothing more goes down.
#[test]
fn nothing_goes_down_after_a_send_whose_reply_failed() {
    let model = TestDoubleModel {
        failing: Some(0),
        ..TestDoubleModel::default()
    };
    let seen = Rc::clone(&model.seen);
    let results = Rc::new(RefCell::new(Vec::new()));
    let retrying = {
        let results = Rc::clone(&results);
        adapter_fn(
            no_state,
            move |(): &mut (),
                  input: &ProgramInput<'_, bool>,
                  below: &mut Below<'_, bool>,
                  _: &mut Outcomes<'_>| {
                let sent = below.send(input.instructions());
                let again = below.send(input.instructions());
                let ran = below.run(input.instructions()).map(drop);
                results.borrow_mut().extend([sent, again, ran]);
                Ok(())
            },
        )
    };
    let mut executor = (retrying >> below(model)).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("shot 0 accepts");
    end.expect_err("the send failed");
    let lost = Err(AdapterError::from("lost".to_owned()));
    assert_eq!(*results.borrow(), [lost.clone(), lost.clone(), lost]);
    assert_eq!(
        seen.borrow().calls.get(&0),
        Some(&1),
        "the first send alone ran"
    );
}

/// The replies an executor holds, for the test to end.
type Held = Rc<RefCell<Vec<ReplySender<ShotOutcome>>>>;

/// An executor below that answers a call of `[false]` later, its reply
/// held for the test to end, as a device does, and any other call at
/// once: tag 0 of each shot, a one. It counts its calls, and keeps no
/// rule of its own, so that what it is called with shows the adapter's.
struct Holding {
    held: Held,
    calls: Rc<Cell<usize>>,
}

/// An executor that holds the replies of `[false]`, the replies it holds,
/// and its calls.
fn holding() -> (Holding, Held, Rc<Cell<usize>>) {
    let (held, calls) = (Held::default(), Rc::new(Cell::new(0)));
    let executor = Holding {
        held: Rc::clone(&held),
        calls: Rc::clone(&calls),
    };
    (executor, held, calls)
}

impl Executor<InstructionBatch<bool>> for Holding {
    type Reply = IncrementalReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        self.calls.set(self.calls.get() + 1);
        let (sender, reply) = reply_channel();
        if batch.instructions == [false] {
            self.held.borrow_mut().push(sender);
        } else {
            for &shot in batch.shots.as_slice() {
                sender.push(one(shot, 0));
            }
            sender.done();
        }
        Ok(reply)
    }
}

impl Describe for Holding {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default().with_qubits(1)
    }
}

impl Close for Holding {
    type Report = ();

    fn close(&mut self) {}
}

/// A decoder that sends a syndrome round, whose reply comes later and
/// fails while the decoder works, then runs a readout: the run is told
/// the send's failure, and does not go down.
#[test]
fn a_send_whose_reply_fails_while_pending_stops_a_later_run() {
    let (executor, held, calls) = holding();
    let ran = Rc::new(RefCell::new(None));
    let decoding = {
        let (held, ran) = (Rc::clone(&held), Rc::clone(&ran));
        move |(): &mut (),
              _: &ProgramInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| {
            below.send(&[false])?;
            let pending = held.borrow_mut().pop().expect("the send's reply is held");
            pending.fail("lost".into());
            *ran.borrow_mut() = Some(below.run(&[true]).map(drop));
            Ok(())
        }
    };
    let mut executor = compose(adapter_fn(no_state, decoding), executor).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("shot 0 accepts");
    end.expect_err("the send failed");
    assert_eq!(*ran.borrow(), Some(Err("lost".into())));
    assert_eq!(calls.get(), 1, "the send alone went down");
}

/// A decoder that sends a syndrome round, whose reply comes later and
/// fails before the logic returns, and whose call's reply is dropped
/// unread: the adapter fails as the call returns, so a later call is
/// refused, though the executor below keeps no rule that would refuse it.
#[test]
fn a_send_failed_before_the_logic_returns_fails_the_adapter_though_its_reply_is_dropped() {
    let (executor, held, calls) = holding();
    let decoding = {
        let held = Rc::clone(&held);
        move |(): &mut (),
              _: &ProgramInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| {
            below.send(&[false])?;
            let pending = held.borrow_mut().pop().expect("the send's reply is held");
            pending.fail("lost".into());
            Ok(())
        }
    };
    let mut executor = compose(adapter_fn(no_state, decoding), executor).expect("bools fit");
    let reply = executor.call(InstructionBatchRef {
        shots: &ShotSet::one(0),
        instructions: &[true],
    });
    drop(reply.expect("shot 0 accepts"));
    let later = run(&mut executor, &ShotSet::one(1), &[true]).expect_err("the adapter failed");
    assert_eq!(later, Rejected::EarlierFailure("lost".into()));
    assert_eq!(calls.get(), 1, "the later call did not go down");
}

/// A decoder that sends a syndrome round, whose reply comes later and
/// fails while the decoder works, then sends a readout: the readout goes
/// down, since nothing sees the failure before the next run or the
/// logic's return (a rule of `adapter_fn`'s).
#[test]
fn a_send_made_after_a_pending_sends_reply_failed_still_goes_down() {
    let (executor, held, calls) = holding();
    let decoding = {
        let held = Rc::clone(&held);
        move |(): &mut (),
              _: &ProgramInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| {
            below.send(&[false])?;
            let pending = held.borrow_mut().pop().expect("the send's reply is held");
            pending.fail("lost".into());
            below.send(&[true])
        }
    };
    let mut executor = compose(adapter_fn(no_state, decoding), executor).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("shot 0 accepts");
    end.expect_err("the first send failed");
    assert_eq!(calls.get(), 2, "both sends went down");
}

/// An executor below that holds the replies of `[false]`, as [`Holding`]
/// does, until it is called with anything else: as a device that goes
/// down, it fails the replies it holds, lost, and refuses the call, busy.
pub(super) struct HoldingUntilBusy(Holding, Held);

/// An executor that holds the replies of `[false]` until it is busy.
pub(super) fn holding_until_busy() -> HoldingUntilBusy {
    let (holding, held, _) = holding();
    HoldingUntilBusy(holding, held)
}

impl Executor<InstructionBatch<bool>> for HoldingUntilBusy {
    type Reply = IncrementalReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        if batch.instructions == [false] {
            return self.0.call(batch);
        }
        for pending in self.1.borrow_mut().drain(..) {
            pending.fail("lost".into());
        }
        Err(Rejected::Unavailable("busy".into()))
    }
}

impl Describe for HoldingUntilBusy {
    fn capabilities(&self) -> Capabilities {
        self.0.capabilities()
    }
}

impl Close for HoldingUntilBusy {
    type Report = ();

    fn close(&mut self) {}
}

/// A decoder sends a syndrome round, whose reply waits below, then a
/// readout the device refuses as it goes down, which ends the call; the
/// syndrome round's reply fails, lost, and the decoder passes the refusal
/// on. The call ends with the refusal that ended it, the reason its later
/// calls are refused with, not with the later loss.
#[test]
fn a_call_ended_by_a_refusal_below_ends_with_it_though_an_earlier_sends_reply_fails_later() {
    let decoding = |(): &mut (),
                    _: &ProgramInput<'_, bool>,
                    below: &mut Below<'_, bool>,
                    _: &mut Outcomes<'_>| {
        below.send(&[false])?;
        below.send(&[true])
    };
    let mut executor =
        compose(adapter_fn(no_state, decoding), holding_until_busy()).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("shot 0 accepts");
    let later = run(&mut executor, &ShotSet::one(1), &[true]).expect_err("the adapter failed");
    assert_eq!(
        (end, later),
        (
            Err("unavailable: busy".to_owned()),
            Rejected::EarlierFailure("unavailable: busy".into())
        )
    );
}

/// A logger whose state holds a file, here a counted handle, and that
/// sends a syndrome round whose reply fails only after the call returned:
/// the call's reply ends failed, and the state made for the call is kept
/// (a rule of `adapter_fn`'s), until close.
#[test]
fn a_send_whose_reply_fails_after_the_call_returned_leaves_the_call_its_state() {
    let file = Rc::new(());
    let (executor, held, _) = holding();
    let logging = {
        let file = Rc::clone(&file);
        adapter_fn(
            move |_: ShotContext| Ok(Rc::clone(&file)),
            |_: &mut Rc<()>,
             _: &ProgramInput<'_, bool>,
             below: &mut Below<'_, bool>,
             _: &mut Outcomes<'_>| below.send(&[false]),
        )
    };
    let mut executor = compose(logging, executor).expect("bools fit");
    let reply = executor.call(InstructionBatchRef {
        shots: &ShotSet::one(0),
        instructions: &[true],
    });
    let mut reply = reply.expect("shot 0 accepts");
    let pending = held.borrow_mut().pop().expect("the send's reply is held");
    pending.fail("lost".into());
    let end = reply.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(end, ReplyStatus::Failed("lost".into()), "the call failed");
    assert_eq!(Rc::strong_count(&file), 3, "the state is held");
}

/// A logger whose state holds a file, here a counted handle, and whose
/// logic fails at its first call: the state made for the call is
/// dropped, as an `executor_fn`'s is, though the adapter is not closed.
#[test]
fn a_failed_call_keeps_no_state_made_for_it() {
    let file = Rc::new(());
    let logging = {
        let file = Rc::clone(&file);
        adapter_fn(
            move |_: ShotContext| Ok(Rc::clone(&file)),
            |_: &mut Rc<()>,
             input: &ProgramInput<'_, bool>,
             below: &mut Below<'_, bool>,
             _: &mut Outcomes<'_>| {
                below.send(input.instructions())?;
                Err("disk full".to_owned().into())
            },
        )
    };
    let mut executor = (logging >> below(TestDoubleModel::default())).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("accepted");
    end.expect_err("the logic failed");
    assert_eq!(Rc::strong_count(&file), 2, "no state is held");
}

/// A logger whose state holds a file, here a counted handle, and whose
/// send is cancelled below at its first call: the state made for the call
/// is dropped, as for a failure, though the adapter is not closed.
#[test]
fn a_cancelled_call_keeps_no_state_made_for_it() {
    let file = Rc::new(());
    let logging = {
        let file = Rc::clone(&file);
        adapter_fn(
            move |_: ShotContext| Ok(Rc::clone(&file)),
            |_: &mut Rc<()>,
             input: &ProgramInput<'_, bool>,
             below: &mut Below<'_, bool>,
             _: &mut Outcomes<'_>| below.send(input.instructions()),
        )
    };
    let model = TestDoubleModel {
        cancelling: Some(0),
        ..TestDoubleModel::default()
    };
    let mut executor = (logging >> below(model)).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("accepted");
    end.expect_err("the send was cancelled");
    assert_eq!(Rc::strong_count(&file), 2, "no state is held");
}

/// A logger whose state holds a file, here a counted handle, whose logic
/// ignores its send's result, and whose batches fail below as they are
/// called: the send's reply fails before the call returns, so the call
/// fails though its logic did not, and the state made for it is dropped,
/// though the adapter is not closed.
#[test]
fn a_call_failed_by_a_sends_reply_keeps_no_state_made_for_it() {
    let file = Rc::new(());
    let logging = {
        let file = Rc::clone(&file);
        adapter_fn(
            move |_: ShotContext| Ok(Rc::clone(&file)),
            |_: &mut Rc<()>,
             input: &ProgramInput<'_, bool>,
             below: &mut Below<'_, bool>,
             _: &mut Outcomes<'_>| {
                let _ = below.send(input.instructions());
                Ok(())
            },
        )
    };
    let model = TestDoubleModel {
        failing: Some(0),
        ..TestDoubleModel::default()
    };
    let mut executor = (logging >> below(model)).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("accepted");
    assert_eq!(end, Err("lost".to_owned()), "the send's reply failed");
    assert_eq!(Rc::strong_count(&file), 2, "no state is held");
}

/// An observer, and a profiler given a report, each closed: each refuses
/// calls, and its logic does not run, whether the adapter reports or not.
#[test]
fn a_closed_adapter_refuses_calls() {
    let ran = Rc::new(Cell::new(0));
    let counting = {
        let ran = Rc::clone(&ran);
        move |(): &mut (),
              input: &ProgramInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| {
            ran.set(ran.get() + 1);
            below.send(input.instructions())
        }
    };
    let mut observed = compose(
        adapter_fn(no_state, counting.clone()),
        below(TestDoubleModel::default()),
    )
    .expect("bools fit");
    observed.close();
    let observed_refusal = run(&mut observed, &ShotSet::one(0), &[true]).expect_err("closed");
    let profiling = adapter_fn(no_state, counting).state_report(|(): &()| 0_u64);
    let mut profiled = compose(profiling, below(TestDoubleModel::default())).expect("bools fit");
    profiled.close();
    let profiled_refusal = run(&mut profiled, &ShotSet::one(0), &[true]).expect_err("closed");
    assert_eq!(
        (observed_refusal, profiled_refusal),
        (Rejected::Closed, Rejected::Closed)
    );
    assert_eq!(ran.get(), 0, "neither adapter's logic ran");
}

/// Closing an observer of two shots' calls gives the report of the
/// executor below, as if no adapter were there.
#[test]
fn closing_gives_the_report_below() {
    let two = shots(&[0, 1]);
    let mut observed = passing_over(TestDoubleModel::default());
    let mut bare = below(TestDoubleModel::default());
    step(
        &mut observed,
        &two,
        &[true],
        "the call runs through the adapter",
    );
    step(&mut bare, &two, &[true], "the call runs alone");
    assert_eq!(
        format!("{:?}", observed.close()),
        format!("{:?}", bare.close())
    );
}

/// The state of a logger holds a file, here a counted handle: closing the
/// adapter drops it.
#[test]
fn closing_drops_the_state() {
    let file = Rc::new(());
    let logging = {
        let file = Rc::clone(&file);
        adapter_fn(
            move |_| Ok(Rc::clone(&file)),
            |_: &mut Rc<()>,
             input: &ProgramInput<'_, bool>,
             below: &mut Below<'_, bool>,
             _: &mut Outcomes<'_>| { below.send(input.instructions()) },
        )
    };
    let mut executor = compose(logging, below(TestDoubleModel::default())).expect("bools fit");
    step(&mut executor, &ShotSet::one(0), &[true], "the call runs");
    assert_eq!(Rc::strong_count(&file), 3, "the state holds the file");
    executor.close();
    assert_eq!(Rc::strong_count(&file), 2, "the state was dropped");
}

/// The logic of a profiler that counts its calls in its state, then sends
/// each call down; it fails on a call of no instruction.
fn count_calls(
    calls: &mut u64,
    input: &ProgramInput<'_, bool>,
    below: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    *calls += 1;
    if input.instructions().is_empty() {
        return Err("an empty call".to_owned().into());
    }
    below.send(input.instructions())
}

/// A profiler with one state, reporting its count of calls, upstream of
/// the tests' executor.
fn profiled() -> impl Executor<InstructionBatch<bool>> + Close<Report = (u64, impl Debug + Fold)> {
    compose(
        adapter_fn(|_| Ok(0), count_calls).state_report(|calls: &u64| *calls),
        below(TestDoubleModel::default()),
    )
    .expect("bools fit")
}

/// A profiler with one state, called twice: closing it gives its count
/// beside the report of the executor below, which is what the executor
/// alone gives for the same calls.
#[test]
fn a_one_state_adapters_report_is_its_state_beside_the_report_below() {
    let mut profiled = profiled();
    let mut bare = below(TestDoubleModel::default());
    step(
        &mut profiled,
        &shots(&[0, 1]),
        &[true],
        "the first call runs",
    );
    step(
        &mut profiled,
        &ShotSet::one(1),
        &[true],
        "the second call runs",
    );
    step(
        &mut bare,
        &shots(&[0, 1]),
        &[true],
        "the first call runs alone",
    );
    step(
        &mut bare,
        &ShotSet::one(1),
        &[true],
        "the second runs alone",
    );
    let (calls, report) = profiled.close();
    assert_eq!(calls, 2);
    assert_eq!(format!("{report:?}"), format!("{:?}", bare.close()));
}

/// A profiler with one state, called on shot 0, then on shot 1: the state
/// is made once, at the first call, from shot 0's context, and shot 1's
/// call counts on from it, so the counts span both shots (`MakeState`: a
/// one-state adapter's state is not a shot's).
#[test]
fn a_one_state_adapters_state_spans_the_shots_of_its_calls() {
    let made_for = Rc::new(RefCell::new(Vec::new()));
    let counts = Rc::new(RefCell::new(Vec::new()));
    let profiler = {
        let (made_for, counts) = (Rc::clone(&made_for), Rc::clone(&counts));
        adapter_fn(
            move |context: ShotContext| {
                made_for.borrow_mut().push(context.shot());
                Ok(0)
            },
            move |calls: &mut u32,
                  input: &ProgramInput<'_, bool>,
                  below: &mut Below<'_, bool>,
                  _: &mut Outcomes<'_>| {
                *calls += 1;
                counts.borrow_mut().push(*calls);
                below.send(input.instructions())
            },
        )
    };
    let mut executor = compose(profiler, below(TestDoubleModel::default())).expect("bools fit");
    step(
        &mut executor,
        &ShotSet::one(0),
        &[true],
        "shot 0's call runs",
    );
    step(
        &mut executor,
        &ShotSet::one(1),
        &[true],
        "shot 1's call runs",
    );
    assert_eq!(*made_for.borrow(), [0], "one state, from shot 0");
    assert_eq!(*counts.borrow(), [1, 2], "shot 1's call counts on");
}

/// A profiler with one state whose first call fails in its logic, after
/// it counted it: the state made for that call is dropped, so the report
/// counts nothing.
#[test]
fn a_failed_call_reports_no_state_made_for_it() {
    let mut profiled = profiled();
    let (_, end) = run(&mut profiled, &ShotSet::one(0), &[]).expect("the call is accepted");
    end.expect_err("the logic fails on an empty call");
    let (calls, _) = profiled.close();
    assert_eq!(calls, 0);
}

/// A profiler with one state that counts a call, then ignores the refusal
/// of its send below, shot 0 busy: the call is refused all the same, so
/// the state made for it is not reported, whatever the logic returned.
#[test]
fn a_call_refused_below_reports_no_state_made_for_it_whatever_the_logic_returns() {
    let ignoring_the_refusal = |calls: &mut u64,
                                input: &ProgramInput<'_, bool>,
                                below: &mut Below<'_, bool>,
                                _: &mut Outcomes<'_>| {
        *calls += 1;
        let _refused = below.send(input.instructions());
        Ok(())
    };
    let mut profiled = compose(
        adapter_fn(|_| Ok(0), ignoring_the_refusal).state_report(|calls: &u64| *calls),
        below(TestDoubleModel {
            refusing: Some(0),
            ..TestDoubleModel::default()
        }),
    )
    .expect("bools fit");
    run(&mut profiled, &ShotSet::one(0), &[true]).expect_err("shot 0 is busy");
    let (calls, _) = profiled.close();
    assert_eq!(calls, 0);
}

/// A profiler with one state whose second call fails in its logic: the
/// state made by the first call is held, as the failed call left it, and
/// reported, though the adapter failed.
#[test]
fn a_state_a_later_call_failed_on_is_reported_as_the_call_left_it() {
    let mut profiled = profiled();
    step(
        &mut profiled,
        &ShotSet::one(0),
        &[true],
        "the first call runs",
    );
    let (_, end) = run(&mut profiled, &ShotSet::one(0), &[]).expect("the call is accepted");
    end.expect_err("the logic fails on an empty call");
    let (calls, _) = profiled.close();
    assert_eq!(calls, 2);
}

/// A profiler with one state whose only call goes below, where its reply
/// waits and fails once the call returned: the call succeeded as it was
/// served, so the state made for it is kept, and reported, though the
/// adapter failed.
#[test]
fn a_state_whose_calls_reply_below_failed_later_is_reported() {
    let (executor, held, _) = holding();
    let mut profiled = compose(
        adapter_fn(|_| Ok(0), count_calls).state_report(|calls: &u64| *calls),
        executor,
    )
    .expect("bools fit");
    let mut reply = profiled
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[false],
        })
        .expect("the call is accepted");
    (held.borrow_mut().pop())
        .expect("the send's reply waits below")
        .fail("lost".into());
    let end = reply.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(
        end,
        ReplyStatus::Failed("lost".into()),
        "the reply below failed"
    );
    let (calls, ()) = profiled.close();
    assert_eq!(calls, 1);
}

/// A profiler with one state whose call panics in its logic, after it
/// counted it: the state made for that call is dropped, so the report
/// counts nothing.
#[test]
fn a_call_that_panicked_reports_no_state_made_for_it() {
    let counting_then_panicking = |calls: &mut u64,
                                   _: &ProgramInput<'_, bool>,
                                   _: &mut Below<'_, bool>,
                                   _: &mut Outcomes<'_>|
     -> Result<(), AdapterError> {
        *calls += 1;
        panic!("the logic has a bug")
    };
    let mut profiled = compose(
        adapter_fn(|_| Ok(0), counting_then_panicking).state_report(|calls: &u64| *calls),
        below(TestDoubleModel::default()),
    )
    .expect("bools fit");
    catch_unwind(AssertUnwindSafe(|| {
        run(&mut profiled, &ShotSet::one(0), &[true])
    }))
    .expect_err("the panic reaches the caller");
    let (calls, _) = profiled.close();
    assert_eq!(calls, 0);
}

/// A profiler whose report has a bug, called on two shots, then closed:
/// the close closes the executor below, then the report's panic reaches
/// the closer.
#[test]
fn a_close_whose_state_report_panics_closes_below_then_raises_it() {
    let (counted, closes) = close_counted(below(TestDoubleModel::default()));
    let mut profiled = compose(
        adapter_fn(|_| Ok(0), count_calls)
            .state_report(|_: &u64| -> u64 { panic!("the report has a bug") }),
        counted,
    )
    .expect("bools fit");
    step(&mut profiled, &shots(&[0, 1]), &[true], "the call runs");
    let panicked = catch_unwind(AssertUnwindSafe(|| profiled.close()))
        .expect_err("the panic reaches the caller");
    assert_eq!(panic_text(&*panicked), Some("the report has a bug"));
    assert_eq!(closes.get(), 1, "the closes below");
}

/// A profiler whose report type has a bug: its `Default` panics. Called
/// on two shots, then closed: the close closes the executor below, then
/// the panic reaches the closer.
#[test]
fn a_close_whose_reports_default_panics_closes_below_then_raises_it() {
    let (counted, closes) = close_counted(below(TestDoubleModel::default()));
    let mut profiled = compose(
        adapter_fn(no_calls, count_and_pass).state_report(|calls: &u64| Unmade(*calls)),
        counted,
    )
    .expect("bools fit");
    step(&mut profiled, &shots(&[0, 1]), &[true], "the call runs");
    let panicked = catch_unwind(AssertUnwindSafe(|| profiled.close()))
        .expect_err("the panic reaches the caller");
    assert_eq!(
        panic_text(&*panicked),
        Some("the report's default has a bug")
    );
    assert_eq!(closes.get(), 1, "the closes below");
}

/// A profiler whose report has a bug, over an executor whose close has
/// one too: the report's panic, the first, reaches the closer once the
/// executor below is closed, and the close below's is left to the panic
/// hook.
#[test]
fn a_state_reports_panic_wins_over_the_close_belows() {
    let (counted, closes) = close_counted(PanicsOnClose(below(TestDoubleModel::default())));
    let mut profiled = compose(
        adapter_fn(no_calls, count_and_pass)
            .state_report(|_: &u64| -> u64 { panic!("the report has a bug") }),
        counted,
    )
    .expect("bools fit");
    step(&mut profiled, &shots(&[0, 1]), &[true], "the call runs");
    let panicked = catch_unwind(AssertUnwindSafe(|| profiled.close()))
        .expect_err("the panic reaches the caller");
    assert_eq!(
        (panic_text(&*panicked), closes.get()),
        (Some("the report has a bug"), 1)
    );
}

/// An observer and a profiler with a report, each holding a state that
/// fails to power down (a bug of its code), over an executor whose close
/// has a bug too, called once, then closed: the state's panic, the first,
/// reaches the closer once the state is freed and the executor below
/// closed, the close below's left to the panic hook.
#[test]
fn a_state_drops_panic_wins_over_the_close_belows() {
    let freed = Freed::default();
    let (counted, observed_closes) =
        close_counted(PanicsOnClose(below(TestDoubleModel::default())));
    let observing = adapter_fn(powering_down_badly(&freed, None), send_down);
    let mut observed = compose(observing, counted).expect("bools fit");
    step(&mut observed, &ShotSet::one(0), &[true], "the call runs");
    let observed_panic =
        catch_unwind(AssertUnwindSafe(|| observed.close())).expect_err("the drop panics");
    let (counted, profiled_closes) =
        close_counted(PanicsOnClose(below(TestDoubleModel::default())));
    let profiling = adapter_fn(powering_down_badly(&freed, None), send_down)
        .state_report(|_: &PowersDownBadly| 0_u64);
    let mut profiled = compose(profiling, counted).expect("bools fit");
    step(&mut profiled, &ShotSet::one(0), &[true], "the call runs");
    let profiled_panic =
        catch_unwind(AssertUnwindSafe(|| profiled.close())).expect_err("the drop panics");
    assert_eq!(
        (
            observed_panic.downcast_ref(),
            profiled_panic.downcast_ref(),
            freed_so_far(&freed)
        ),
        (
            Some(&PowerDownFailed(0)),
            Some(&PowerDownFailed(0)),
            vec![0, 0]
        ),
        "the panics raised, and the states freed"
    );
    assert_eq!(
        (observed_closes.get(), profiled_closes.get()),
        (1, 1),
        "the observer's and the profiler's closes below"
    );
}

/// An observer and a profiler with a report, whose code has no bug, each
/// over an executor whose close has one: the close below's panic reaches
/// the closer.
#[test]
fn a_close_belows_panic_reaches_the_closer_when_nothing_panicked_before() {
    let mut observed = compose(
        adapter_fn(no_state, pass),
        PanicsOnClose(below(TestDoubleModel::default())),
    )
    .expect("bools fit");
    step(&mut observed, &shots(&[0, 1]), &[true], "the call runs");
    let observed_panic =
        catch_unwind(AssertUnwindSafe(|| observed.close())).expect_err("the close panics");
    let counting = adapter_fn(no_calls, count_and_pass).state_report(|calls: &u64| *calls);
    let mut profiled =
        compose(counting, PanicsOnClose(below(TestDoubleModel::default()))).expect("bools fit");
    step(&mut profiled, &shots(&[0, 1]), &[true], "the call runs");
    let profiled_panic =
        catch_unwind(AssertUnwindSafe(|| profiled.close())).expect_err("the close panics");
    assert_eq!(
        (panic_text(&*observed_panic), panic_text(&*profiled_panic)),
        (Some("the close has a bug"), Some("the close has a bug"))
    );
}

/// A report whose `Default` has a bug, and panics.
#[derive(Debug)]
pub(super) struct Unmade(u64);

impl Default for Unmade {
    fn default() -> Self {
        panic!("the report's default has a bug")
    }
}

impl Fold for Unmade {
    fn fold(&mut self, other: Self) {
        self.0 += other.0;
    }
}

/// A profiler with two bugs: its report panics as `close` reads a state
/// that fails to power down. The report's panic reaches the closer once
/// the state is freed and the executor below closed, and the state's,
/// freed in a catch of its own, is left to the panic hook, rather than
/// abort the process.
#[test]
fn a_state_reports_panic_reaches_the_closer_though_the_states_drop_panics_too() {
    let freed = Freed::default();
    let (counted, closes) = close_counted(below(TestDoubleModel::default()));
    let profiling = adapter_fn(powering_down_badly(&freed, None), send_down)
        .state_report(|_: &PowersDownBadly| -> u64 { panic!("the report has a bug") });
    let mut profiled = compose(profiling, counted).expect("bools fit");
    step(&mut profiled, &ShotSet::one(0), &[true], "the call runs");
    let panicked =
        catch_unwind(AssertUnwindSafe(|| profiled.close())).expect_err("the report panics");
    assert_eq!(
        (panic_text(&*panicked), freed_so_far(&freed), closes.get()),
        (Some("the report has a bug"), vec![0], 1)
    );
}

/// A profiler with one state, called once, then closed twice: the second
/// close reports no state, as an executor's second report is empty.
#[test]
fn a_second_close_reports_no_state() {
    let mut profiled = profiled();
    step(&mut profiled, &ShotSet::one(0), &[true], "the call runs");
    let (first, _) = profiled.close();
    assert_eq!(first, 1, "the first close reports the state");
    let (second, _) = profiled.close();
    assert_eq!(second, 0);
}

/// A logic with a bug panics: the panic reaches the caller, and the
/// adapter fails, so later calls are refused.
#[test]
fn a_logic_that_panics_fails_the_adapter() {
    let buggy = |(): &mut (),
                 _: &ProgramInput<'_, bool>,
                 _: &mut Below<'_, bool>,
                 _: &mut Outcomes<'_>|
     -> Result<(), AdapterError> { panic!("the logic has a bug") };
    let mut executor = compose(
        adapter_fn(no_state, buggy),
        below(TestDoubleModel::default()),
    )
    .expect("bools fit");
    catch_unwind(AssertUnwindSafe(|| {
        run(&mut executor, &ShotSet::one(0), &[true])
    }))
    .expect_err("the panic reaches the caller");
    let later = run(&mut executor, &ShotSet::one(0), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// A lowering whose check has a bug, and a profiler whose state closure
/// has one, each called once: the panic reaches the caller, and the
/// adapter fails, so later calls are refused, as after a logic's panic.
#[test]
fn a_check_or_a_state_closure_that_panics_fails_the_adapter() {
    let buggy_check =
        |_: &ProgramInput<'_, bool>| -> Result<(), Rejected> { panic!("the check has a bug") };
    let mut lowered = compose(
        adapter_fn(no_state, pass).check(buggy_check),
        below(TestDoubleModel::default()),
    )
    .expect("bools fit");
    let buggy_state = |_: ShotContext| -> Result<u64, Rejected> { panic!("the state has a bug") };
    let mut profiled = compose(
        adapter_fn(buggy_state, count_calls),
        below(TestDoubleModel::default()),
    )
    .expect("bools fit");
    let refusals = (
        refusal_after_a_panic(&mut lowered, &ShotSet::one(0)),
        refusal_after_a_panic(&mut profiled, &ShotSet::one(0)),
    );
    assert!(
        matches!(
            refusals,
            (Rejected::EarlierFailure(_), Rejected::EarlierFailure(_))
        ),
        "the lowering's and the profiler's later calls: {refusals:?}"
    );
}

/// Calls `executor` on `shots`, a call that panics, then again: the
/// second call's refusal.
fn refusal_after_a_panic(
    executor: &mut impl Executor<InstructionBatch<bool>>,
    shots: &ShotSet,
) -> Rejected {
    catch_unwind(AssertUnwindSafe(|| run(&mut *executor, shots, &[true])))
        .expect_err("the panic reaches the caller");
    run(executor, shots, &[true]).expect_err("the adapter failed")
}

/// A decoder that writes a logical outcome, tag 9, for a call on shots 2
/// and 5 and sends nothing: the outcome answers each shot of the call.
#[test]
fn an_outcome_written_answers_every_shot_of_the_call() {
    let decoding = |(): &mut (),
                    _: &ProgramInput<'_, bool>,
                    _: &mut Below<'_, bool>,
                    outcomes: &mut Outcomes<'_>| {
        outcomes.push(Outcome::new(9, Value::One));
        Ok(())
    };
    let mut executor = compose(
        adapter_fn(no_state, decoding),
        below(TestDoubleModel::default()),
    )
    .expect("bools fit");
    let (written, end) = run(&mut executor, &shots(&[2, 5]), &[true]).expect("accepted");
    end.expect("the logic does not fail");
    assert_eq!(values(&written), [(2, 9, Value::One), (5, 9, Value::One)]);
}

/// A decoder that writes its logical outcome after it sent the call down:
/// the reply gives what the logic wrote first, then what the send read.
#[test]
fn what_the_logic_writes_comes_before_what_its_sends_read() {
    let decoding = |(): &mut (),
                    input: &ProgramInput<'_, bool>,
                    below: &mut Below<'_, bool>,
                    outcomes: &mut Outcomes<'_>| {
        below.send(input.instructions())?;
        outcomes.push(Outcome::new(9, Value::One));
        Ok(())
    };
    let mut executor = compose(
        adapter_fn(no_state, decoding),
        below(TestDoubleModel::default()),
    )
    .expect("bools fit");
    let (out, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("accepted");
    end.expect("the call ends well");
    let tags: Vec<u32> = out.iter().map(|o| o.tag).collect();
    assert_eq!(tags, [9, 0]);
}

/// A logic that sends the call down twice: each send is one call below,
/// whose reads come in the order of the sends.
#[test]
fn each_send_is_one_call_below() {
    let model = TestDoubleModel::default();
    let seen = Rc::clone(&model.seen);
    let twice = |(): &mut (),
                 input: &ProgramInput<'_, bool>,
                 below: &mut Below<'_, bool>,
                 _: &mut Outcomes<'_>| {
        below.send(input.instructions())?;
        below.send(&[false, true])
    };
    let mut executor = compose(adapter_fn(no_state, twice), below(model)).expect("bools fit");
    let (out, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("accepted");
    end.expect("the call ends well");
    assert_eq!(seen.borrow().calls[&0], 2, "two calls below");
    let tags: Vec<u32> = out.iter().map(|o| o.tag).collect();
    assert_eq!(tags, [0, 1], "the first send's read, then the second's");
}

/// A decoder whose state cannot be made, its tables loading: the call is
/// refused with that refusal, before the logic runs.
#[test]
fn a_state_that_cannot_be_made_refuses_the_call_before_the_logic_runs() {
    let ran = Rc::new(Cell::new(0));
    let decoder = {
        let ran = Rc::clone(&ran);
        adapter_fn(
            |_| -> Result<(), Rejected> { Err(Rejected::Unavailable("loading".into())) },
            move |(): &mut (),
                  input: &ProgramInput<'_, bool>,
                  below: &mut Below<'_, bool>,
                  _: &mut Outcomes<'_>| {
                ran.set(ran.get() + 1);
                below.send(input.instructions())
            },
        )
    };
    let mut executor = compose(decoder, below(TestDoubleModel::default())).expect("bools fit");
    let refused = run(&mut executor, &ShotSet::one(0), &[true]).expect_err("loading");
    assert_eq!(refused, Rejected::Unavailable("loading".into()));
    assert_eq!(ran.get(), 0, "the logic did not run");
}

/// The state of a decoder cannot be made the first time, its tables
/// loading: the next call makes it, and runs.
#[test]
fn a_state_that_cannot_be_made_is_made_again_at_the_next_call() {
    let decoder = {
        let mut tries = 0;
        adapter_fn(
            move |_| {
                tries += 1;
                if tries == 1 {
                    Err(Rejected::Unavailable("loading".into()))
                } else {
                    Ok(())
                }
            },
            pass,
        )
    };
    let mut executor = compose(decoder, below(TestDoubleModel::default())).expect("bools fit");
    run(&mut executor, &ShotSet::one(0), &[true]).expect_err("loading");
    step(
        &mut executor,
        &ShotSet::one(0),
        &[true],
        "the state is made",
    );
}

/// The check of a lowering that has no rule for a skip (`false`): it
/// refuses a call holding one as malformed.
fn no_skip(input: &ProgramInput<'_, bool>) -> Result<(), Rejected> {
    if input.instructions().contains(&false) {
        Err(Rejected::Malformed("no rule for a skip".into()))
    } else {
        Ok(())
    }
}

/// A lowering with no rule for a skip, which checks for one, called with a
/// skip: the call is refused with the check's refusal, before the state
/// is made and before the logic runs, so nothing goes below.
#[test]
fn the_check_refuses_a_call_before_the_state_is_made_and_the_logic_runs() {
    let (made, ran) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let lowering = {
        let (made, ran) = (Rc::clone(&made), Rc::clone(&ran));
        adapter_fn(
            move |_| {
                made.set(made.get() + 1);
                Ok(())
            },
            move |(): &mut (),
                  input: &ProgramInput<'_, bool>,
                  below: &mut Below<'_, bool>,
                  _: &mut Outcomes<'_>| {
                ran.set(ran.get() + 1);
                below.send(input.instructions())
            },
        )
        .check(no_skip)
    };
    let mut lowered = (lowering >> below(TestDoubleModel::default())).expect("bools fit");
    let refused = run(&mut lowered, &ShotSet::one(0), &[true, false]).expect_err("a skip");
    assert_eq!(refused, Rejected::Malformed("no rule for a skip".into()));
    assert_eq!(
        (made.get(), ran.get()),
        (0, 0),
        "no state made, no logic run"
    );
}

/// The same lowering called with a skip, then with reads only: the
/// refusal leaves it serving, and the call it accepts gives what it gives
/// with no check.
#[test]
fn a_call_the_check_refuses_leaves_the_adapter_serving() {
    let three = shots(&[0, 1, 2]);
    let checked = adapter_fn(no_state, pass).check(no_skip);
    let mut lowered = (checked >> below(TestDoubleModel::default())).expect("bools fit");
    let mut unchecked = passing_over(TestDoubleModel::default());
    run(&mut lowered, &three, &[true, false]).expect_err("a skip");
    let (through, end) = run(&mut lowered, &three, &[true, true]).expect("reads only");
    end.expect("the call ends well, checked");
    let (alone, end) = run(&mut unchecked, &three, &[true, true]).expect("accepted");
    end.expect("the call ends well, unchecked");
    assert!(!through.is_empty(), "the batch reads bits");
    assert_eq!(values(&through), values(&alone));
}

/// A lowering with no rule for a skip, which checks for one and counts its
/// checks, closed, then called with a skip: the call is refused as closed,
/// before the check runs.
#[test]
fn a_closed_adapter_refuses_a_call_before_its_check_runs() {
    let checks = Rc::new(Cell::new(0));
    let counted_check = {
        let checks = Rc::clone(&checks);
        move |input: &ProgramInput<'_, bool>| {
            checks.set(checks.get() + 1);
            no_skip(input)
        }
    };
    let checked = adapter_fn(no_state, pass).check(counted_check);
    let mut lowered = (checked >> below(TestDoubleModel::default())).expect("bools fit");
    lowered.close();
    let refused = run(&mut lowered, &ShotSet::one(0), &[true, false]).expect_err("closed");
    assert_eq!((refused, checks.get()), (Rejected::Closed, 0));
}

/// A noise adapter with one state, drawing with the id `noise`, over a
/// simulator given the run's seed, first called on shots 5 and 11: its
/// state seeds from the context of shot 5, the first shot it serves, in
/// the run, bound to the noise's id.
#[test]
fn a_drawing_adapters_state_seeds_from_the_first_shot_it_serves() {
    const SEED: u64 = 0x5EED;
    let noise_id = ComponentId::named("noise");
    let seeds = Rc::new(RefCell::new(Vec::new()));
    let noise = {
        let seeds = Rc::clone(&seeds);
        adapter_fn(
            move |context: ShotContext| {
                seeds.borrow_mut().push(context.seed());
                Ok(())
            },
            pass,
        )
        .drawing(noise_id)
    };
    let simulator = executor_fn(
        Capabilities::default().with_qubits(4),
        |_| Ok(()),
        |(): &mut (), _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
    )
    .seed(SEED)
    .shots(OnePerShot);
    let mut executor = (noise >> simulator).expect("bools fit");
    step(
        &mut executor,
        &shots(&[5, 11]),
        &[true],
        "the first call runs",
    );
    step(&mut executor, &shots(&[2]), &[true], "the second call runs");
    let first = ShotContext::new(SEED, 5).rng_seed(noise_id);
    assert_eq!(*seeds.borrow(), [first]);
}

/// A noise adapter with one state, which states it draws with the id
/// `noise`, over a simulator given the run's seed, which offers replayable
/// shots: the state would seed from the first shot of its first call, so
/// a shot's noise would depend on the shots it is called with, and the
/// adapter does not offer replayable shots upstream.
#[test]
fn a_drawing_adapter_is_not_replayable() {
    let simulator = executor_fn(
        Capabilities::default().with_qubits(4).with_replayable(true),
        |_| Ok(()),
        |(): &mut (), _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
    )
    .seed(0x5EED)
    .shots(OnePerShot);
    assert!(
        simulator.capabilities().replayable,
        "the simulator replays its shots"
    );
    let noise = adapter_fn(no_state, pass).drawing(ComponentId::named("noise"));
    let executor = (noise >> simulator).expect("bools fit");
    assert!(!executor.capabilities().replayable);
}

/// A noise adapter with one state, which states it draws with the id
/// `noise`, over a simulator given the run's seed, which keeps each shot's
/// state across calls: a program with feedback, run through the noise,
/// continues each shot in a later call, so the adapter offers resumable
/// shots upstream.
#[test]
fn a_drawing_adapter_passes_resumable_shots_up() {
    let simulator = executor_fn(
        Capabilities::default()
            .with_qubits(4)
            .with_resumable_shots(true),
        |_| Ok(()),
        |(): &mut (), _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
    )
    .seed(0x5EED)
    .shots(OnePerShot);
    assert!(
        simulator.capabilities().resumable_shots,
        "the simulator keeps each shot's state across calls"
    );
    let noise = adapter_fn(no_state, pass).drawing(ComponentId::named("noise"));
    let executor = (noise >> simulator).expect("bools fit");
    assert!(executor.capabilities().resumable_shots);
}

/// A noise adapter with one state, drawing with the id `noise`, that
/// skips its call's read when its draw says so, over a simulator given the
/// run's seed, called on four shots: its logic runs once for the call, and
/// its send goes below as one call over the four shots, so they all share
/// its draw.
#[test]
fn a_one_state_adapters_draw_is_shared_by_the_shots_of_a_call() {
    let noise = adapter_fn(
        |context: ShotContext| Ok(context.seed()),
        |seed: &mut u64,
         _: &ProgramInput<'_, bool>,
         below: &mut Below<'_, bool>,
         _: &mut Outcomes<'_>| {
            let lost = *seed & 1 == 1;
            *seed >>= 1;
            below.send(&[!lost])
        },
    )
    .drawing(ComponentId::named("noise"));
    let simulator = executor_fn(
        Capabilities::default().with_qubits(4),
        |_| Ok(()),
        |(): &mut (), _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
    )
    .seed(0x5EED)
    .shots(OnePerShot);
    let (simulator, calls) = recorded(simulator);
    let mut executor = (noise >> simulator).expect("bools fit");
    step(
        &mut executor,
        &shots(&[0, 1, 2, 3]),
        &[true],
        "the call runs",
    );
    let shots_below: Vec<_> = calls.borrow().iter().map(|(s, _)| s.clone()).collect();
    assert_eq!(shots_below, [vec![0, 1, 2, 3]]);
}

/// A simulator given the run's seed, a state per shot, that offers every
/// capability it can, each distinct from the default: at most 5 qubits and
/// 8 shots at once, resumable and replayable shots, resets, each shot's log
/// probability, the bits' instruction set, and draws with the id `readout`
/// beside its own.
fn ample_simulator()
-> OnePerShotExecutor<bool, (), impl MakeState<()>, impl ExecutorLogic<bool, ()>> {
    let promise = Capabilities::default()
        .with_qubits(5)
        .with_max_shots(8)
        .with_resumable_shots(true)
        .with_replayable(true)
        .with_resets(true)
        .with_instruction_set(Some(Arc::new(bits_set())))
        .drawing(ComponentId::named("readout"))
        .expect("the simulator draws with no other id");
    executor_fn(
        promise,
        |_| Ok(()),
        |(): &mut (), _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
    )
    .log_probability((|(): &()| -0.5) as fn(&()) -> f64)
    .seed(0x5EED)
    .shots(OnePerShot)
}

/// A simulator given the run's seed, a state per shot, that offers only
/// what it must: at most `qubits` qubits and `max_shots` shots at once
/// (`None`: it states no limit), no resumable or replayable shots, no
/// resets, no log probability and no instruction set.
fn scant_simulator(
    qubits: Option<u32>,
    max_shots: Option<u64>,
) -> OnePerShotExecutor<bool, (), impl MakeState<()>, impl ExecutorLogic<bool, ()>> {
    let promise = Capabilities {
        qubits,
        max_shots,
        ..Capabilities::default()
    };
    executor_fn(
        promise,
        |_| Ok(()),
        |(): &mut (), _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
    )
    .seed(0x5EED)
    .shots(OnePerShot)
}

/// The needs a chain on `executor` refuses, in order, for
/// a job beyond [`scant_simulator`]`(Some(3), Some(2))` in every need: its
/// program needs 4 qubits, resumable and replayable shots, resets, each
/// shot's log probability and the bits' instruction set, on 3 shots.
fn unmet_needs_of_a_job_beyond_the_scant_simulator<E>(executor: E) -> Vec<Need>
where
    E: Executor<InstructionBatch<bool>> + Describe + Close,
{
    let chain = compose(driver(), executor).expect("bools fit");
    let needs = Needs::default()
        .with_qubits(4)
        .with_resumable_shots(true)
        .with_replayable(true)
        .with_resets(true)
        .with_log_probability(true)
        .with_instruction_set(Some(Arc::new(bits_set())));
    let program = StaticProgram::new(needs, vec![vec![true]]);
    let Err(JobFaults(faults)) = chain.run(program, [0, 1, 2]) else {
        panic!("the job needs what the executor below does not offer");
    };
    (faults.iter())
        .map(|fault| match fault {
            Rejected::Unmet(unmet) => unmet.need,
            other => panic!("the job's faults are its unmet needs: {other:?}"),
        })
        .collect()
}

/// [`ample_simulator`], which offers every capability it can, under an
/// adapter with one state that passes each call down: drawing nothing, it
/// offers all the simulator offers; drawing with the id `noise`, it offers
/// it all but the stated exceptions (no log probability, no replayable
/// shots), with `noise` added to the ids drawn with.
#[test]
fn it_offers_what_the_executor_below_offers() {
    let ample = ample_simulator().capabilities();
    let observer = (adapter_fn(no_state, pass) >> ample_simulator()).expect("bools fit");
    assert_eq!(observer.capabilities(), ample);
    let noise = ComponentId::named("noise");
    let drawing = (adapter_fn(no_state, pass).drawing(noise) >> ample_simulator())
        .expect("the simulator is given the run's seed");
    let mut expected = ample
        .drawing(noise)
        .expect("nothing below draws with `noise`");
    expected.log_probability = false;
    expected.replayable = false;
    assert_eq!(drawing.capabilities(), expected);
}

/// Two [`scant_simulator`]s, which offer only what they must, one bounded
/// (3 qubits, 2 shots at once) and one not, each under an adapter with one
/// state that passes each call down, drawing nothing or drawing with the id
/// `noise`: it offers nothing the simulator does not, `noise` among the ids
/// drawn with apart; and a job beyond the bounded simulator in every need,
/// given to the chain, is refused with each need unmet, as it is with no
/// adapter.
#[test]
fn it_offers_nothing_the_executor_below_does_not() {
    let noise = ComponentId::named("noise");
    for (qubits, max_shots) in [(Some(3), Some(2)), (None, None)] {
        let scant = || scant_simulator(qubits, max_shots);
        let offered = scant().capabilities();
        let observer = (adapter_fn(no_state, pass) >> scant()).expect("bools fit");
        assert_eq!(observer.capabilities(), offered);
        let drawing = (adapter_fn(no_state, pass).drawing(noise) >> scant())
            .expect("the simulator is given the run's seed");
        let expected = offered
            .drawing(noise)
            .expect("nothing below draws with `noise`");
        assert_eq!(drawing.capabilities(), expected);
    }
    let bounded = || scant_simulator(Some(3), Some(2));
    let every_need = [
        Need::Qubits,
        Need::ResumableShots,
        Need::Resets,
        Need::Replayable,
        Need::LogProbability,
        Need::InstructionSet,
        Need::Shots,
    ];
    assert_eq!(
        unmet_needs_of_a_job_beyond_the_scant_simulator(bounded()),
        every_need,
        "the simulator meets none of the job's needs"
    );
    let observer = (adapter_fn(no_state, pass) >> bounded()).expect("bools fit");
    assert_eq!(
        unmet_needs_of_a_job_beyond_the_scant_simulator(observer),
        every_need
    );
    let drawing = (adapter_fn(no_state, pass).drawing(noise) >> bounded())
        .expect("the simulator is given the run's seed");
    assert_eq!(
        unmet_needs_of_a_job_beyond_the_scant_simulator(drawing),
        every_need
    );
}

/// An executor below that offers what it is made with, and answers each
/// call done, reading nothing.
pub(super) struct Offering(pub(super) Capabilities);

impl Executor<InstructionBatch<bool>> for Offering {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, _: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        Ok(ImmediateReply::default())
    }
}

impl Describe for Offering {
    fn capabilities(&self) -> Capabilities {
        self.0.clone()
    }
}

impl Close for Offering {
    type Report = ();

    fn close(&mut self) {}
}

/// Each yes/no capability alone, given the run's seed beside it: resumable
/// shots, resets, replayable shots, then each shot's log probability.
pub(super) fn each_capability_alone() -> [Capabilities; 4] {
    let seeded = Capabilities {
        run_seed: Some(0x5EED),
        ..Capabilities::default()
    };
    [
        seeded.clone().with_resumable_shots(true),
        seeded.clone().with_resets(true),
        seeded.clone().with_replayable(true),
        seeded.with_log_probability(true),
    ]
}

/// A device below, given the run's seed, that offers one yes/no capability
/// alone ([`each_capability_alone`]), each in turn, under an adapter with
/// one state that passes each call down: drawing nothing, it offers that
/// capability alone, as the device does; drawing with the id `noise`, it
/// offers it too, unless it is a stated exception (the log probability,
/// replayable shots), with `noise` added to the ids drawn with.
#[test]
fn it_passes_each_capability_up_on_its_own() {
    let noise = ComponentId::named("noise");
    for lone in each_capability_alone() {
        let observer =
            compose(adapter_fn(no_state, pass), Offering(lone.clone())).expect("bools fit");
        assert_eq!(observer.capabilities(), lone);
        let drawing = compose(
            adapter_fn(no_state, pass).drawing(noise),
            Offering(lone.clone()),
        )
        .expect("the device is given the run's seed");
        let mut expected = lone
            .drawing(noise)
            .expect("nothing below draws with `noise`");
        expected.log_probability = false;
        expected.replayable = false;
        assert_eq!(drawing.capabilities(), expected);
    }
}

/// A lowering offers upstream the set it accepts, not the one below.
#[test]
fn a_rewriter_offers_the_set_it_accepts() {
    let lowering = adapter_fn(no_state, lower)
        .accepts(flip_first_set())
        .sends(reversed_set());
    let executor = compose(lowering, TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits have every instruction it sends");
    let offered = executor.capabilities().instruction_set;
    assert_eq!(offered.as_deref(), Some(&flip_first_set()));
}

/// A lowering that draws with the id `noise`, its options given in another
/// order (the set it sends before the set it accepts, a check and a report
/// of its state after them), over the bits, given the run's seed: it offers
/// the set it accepts and the id it draws with, as given in the usual order,
/// and each read reaches the bits as the instruction it names.
#[test]
fn its_options_given_in_any_order_make_the_same_adapter() {
    let noise = ComponentId::named("noise");
    let usual = adapter_fn(no_state, lower)
        .accepts(flip_first_set())
        .sends(reversed_set())
        .drawing(noise);
    let usual = compose(usual, TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits have every instruction it sends");
    let reordered = adapter_fn(no_state, lower)
        .sends(reversed_set())
        .accepts(flip_first_set())
        .drawing(noise)
        .check(|_: &ProgramInput<'_, InstructionCall>| Ok(()))
        .state_report(|(): &()| ());
    let mut reordered = compose(
        reordered,
        TestDoubleExecutor::with_model(Bits::default(), 7),
    )
    .expect("the bits have every instruction it sends");
    let offered = reordered.capabilities();
    assert_eq!(offered.instruction_set.as_deref(), Some(&flip_first_set()));
    assert!(offered.rng_ids.contains(noise), "{offered:?}");
    assert_eq!(offered, usual.capabilities());
    let mut bare = TestDoubleExecutor::with_model(Bits::default(), 7);
    let two = shots(&[0, 1]);
    for (program, written) in flip_and_toss_flip_first()
        .iter()
        .zip(flip_and_toss(&bits_set()))
    {
        let through = values(&outcomes(&mut reordered, &two, program));
        assert_eq!(through, values(&outcomes(&mut bare, &two, &written)));
    }
}

/// A lowering that sends `h`, which the bits lack, is refused when the
/// chain is built, naming `h`.
#[test]
fn a_rewriter_sending_what_the_executor_lacks_is_refused_when_composed() {
    let lowering = adapter_fn(no_state, lower)
        .accepts(flip_first_set())
        .sends(reversed_set().with("h", [Block]));
    let Err(faults) = compose(lowering, TestDoubleExecutor::with_model(Bits::default(), 7)) else {
        panic!("the bits lack h");
    };
    let missing = InstructionMismatch::Missing {
        mnemonic: "h".to_owned(),
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert_eq!(faults.0[0].error, BuildError::Instructions(vec![missing]));
}

/// Shot 0's reply below waits until the test ends it: the call's reply
/// passes up the read as it comes, pending, then ends when it ends.
#[test]
fn a_reply_below_passes_up_as_it_comes() {
    let model = TestDoubleModel {
        deferred: vec![0],
        ..TestDoubleModel::default()
    };
    let seen = Rc::clone(&model.seen);
    let mut executor = passing_over(model);
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[true],
        })
        .expect("shot 0 accepts");
    let mut out = Vec::new();
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Pending
    );
    assert_eq!(out.len(), 1, "the read came: {out:?}");
    let sender = seen
        .borrow_mut()
        .replies
        .get_mut(&0)
        .expect("deferred")
        .remove(0);
    sender.done();
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Done
    );
}

/// Shot 0's reply below waits until the test fails it, after the call of
/// an observer returned: the call's reply ends with the failure as it gives
/// it, and the adapter fails, so a later call is refused, though it would
/// not go down.
#[test]
fn a_send_whose_reply_fails_after_the_call_returned_fails_the_call_and_the_adapter() {
    let model = TestDoubleModel {
        deferred: vec![0],
        ..TestDoubleModel::default()
    };
    let seen = Rc::clone(&model.seen);
    let mut executor =
        compose(adapter_fn(no_state, pass_unless_empty), below(model)).expect("bools fit");
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[true],
        })
        .expect("shot 0 accepts");
    let mut out = Vec::new();
    let before = reply.poll_events(&mut out, Waker::noop());
    assert_eq!(before, ReplyStatus::Pending, "the send's reply is held");
    let sender = seen
        .borrow_mut()
        .replies
        .get_mut(&0)
        .expect("deferred")
        .remove(0);
    sender.fail("lost".into());
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Failed("lost".into())
    );
    let later = run(&mut executor, &ShotSet::one(1), &[]).expect_err("the adapter failed");
    assert_eq!(later, Rejected::EarlierFailure("lost".into()));
}

/// The logic of a decoder that runs each call below and writes each
/// readout it reads with its value flipped.
fn flip_what_runs(
    (): &mut (),
    input: &ProgramInput<'_, bool>,
    below: &mut Below<'_, bool>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    let read = below.run(input.instructions())?;
    outcomes.extend(read.into_iter().map(|outcome| ShotOutcome {
        value: (outcome.value == Value::Zero).into(),
        ..outcome
    }));
    Ok(())
}

/// A decoder that flips what its run reads, on three shots: the call gives
/// what the logic wrote, the bits below flipped, and not what the run read
/// as well.
#[test]
fn what_a_run_reads_passes_up_only_as_the_logic_writes_it() {
    let three = shots(&[0, 1, 2]);
    let mut decoded = compose(
        adapter_fn(no_state, flip_what_runs),
        below(TestDoubleModel::default()),
    )
    .expect("bools fit");
    let (through, end) = run(&mut decoded, &three, &[true, true]).expect("accepted");
    end.expect("the call ends well");
    let (bare, _) = run(
        &mut below(TestDoubleModel::default()),
        &three,
        &[true, true],
    )
    .expect("the bits accept it");
    let flipped: Vec<_> = (values(&bare).into_iter())
        .map(|(shot, tag, value)| (shot, tag, (value == Value::Zero).into()))
        .collect();
    assert_eq!(flipped.len(), 6, "each shot reads two bits");
    assert_eq!(values(&through), flipped);
}

/// A decoder that runs each call on shots 2, 0 and 1: the executor below is
/// called once, on the call's shots, with what the logic runs.
#[test]
fn a_run_goes_below_as_one_call_on_the_calls_shots() {
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let mut decoded = (adapter_fn(no_state, flip_what_runs) >> executor).expect("bools fit");
    step(&mut decoded, &shots(&[2, 0, 1]), &[true], "the call runs");
    assert_eq!(*calls.borrow(), [(vec![2, 0, 1], vec![true])]);
}

/// A simulator below that reads tags 29 down to 0, on shots 2, 0 and 1,
/// as a decoder's syndrome rounds read many: a decoder's run gives the
/// readouts by shot, each shot's in the order it read them, as the shot's
/// reply keeps them, not by tag. The readouts are many, 90, because the
/// standard library's unstable sort sorts a few by insertion, which keeps
/// their order: with 6, a sort that is not stable would not be seen.
#[test]
fn a_run_gives_its_readouts_by_shot_each_in_the_order_read() {
    let backwards = executor_fn(
        Capabilities::default().with_qubits(2),
        |_| Ok(()),
        |(): &mut (), _: &ProgramInput<'_, bool>, outcomes: &mut Outcomes<'_>| {
            for tag in (0..30).rev() {
                outcomes.push(Outcome::new(tag, Value::One));
            }
            Ok(())
        },
    )
    .seed(7)
    .shots(OnePerShot);
    let read = Rc::new(RefCell::new(Vec::new()));
    let reading = {
        let read = Rc::clone(&read);
        adapter_fn(
            no_state,
            move |(): &mut (),
                  input: &ProgramInput<'_, bool>,
                  below: &mut Below<'_, bool>,
                  _: &mut Outcomes<'_>| {
                read.borrow_mut().extend(below.run(input.instructions())?);
                Ok(())
            },
        )
    };
    let mut executor = (reading >> backwards).expect("bools fit");
    step(&mut executor, &shots(&[2, 0, 1]), &[true], "the call runs");
    let by_shot: Vec<_> = [0, 1, 2]
        .into_iter()
        .flat_map(|shot| (0..30).rev().map(move |tag| (shot, tag, Value::One)))
        .collect();
    assert_eq!(values(&read.borrow()), by_shot);
}

/// An observer sends each call down twice, over an executor below that
/// answers its calls on other threads, as placed executors do: after the
/// call returned, the second send's reply gives tag 1, still pending or
/// ended, while the first send's has given nothing yet. The call's reply
/// passes tag 1 up at once, then tag 0, the first send's, once its reply
/// gives it: the tags match, but a later send's readouts pass up before
/// those an earlier send gives later, not in the order the shot read
/// them, whether the later send's reply ended or not.
#[test]
fn a_later_sends_readouts_pass_up_before_those_an_earlier_send_gives_later() {
    for ended in [false, true] {
        let senders = Senders::default();
        let mut observed = compose(
            adapter_fn(no_state, send_twice),
            HeldOpen(Rc::clone(&senders)),
        )
        .expect("bools fit");
        let mut reply = observed
            .call(InstructionBatchRef {
                shots: &ShotSet::one(0),
                instructions: &[true],
            })
            .expect("the call is accepted");
        let second = senders.borrow_mut().pop().expect("the second send is open");
        let first = senders.borrow_mut().pop().expect("the first send is open");
        second.push(one(0, 1));
        let second = if ended {
            second.done();
            None
        } else {
            Some(second)
        };
        let mut read = Vec::new();
        assert_eq!(
            reply.poll_events(&mut read, Waker::noop()),
            ReplyStatus::Pending
        );
        assert_eq!(
            read,
            [one(0, 1)],
            "the second send's readout (ended: {ended})"
        );
        first.push(one(0, 0));
        first.done();
        if let Some(second) = second {
            second.done();
        }
        wait(reply, &mut read).expect("both sends end well");
        assert_eq!(read, [one(0, 1), one(0, 0)], "ended: {ended}");
    }
}

/// An observer sends each call down once, over an executor below whose
/// reply reads tag 0 within the call, so the send's poll at once takes it,
/// then tag 1 after the call returned: the call's reply passes up tag 0,
/// then tag 1, the order the send's reply gave them.
#[test]
fn a_sends_readouts_pass_up_in_the_order_its_reply_gives_them() {
    let senders = Senders::default();
    let mut observed = compose(
        adapter_fn(no_state, pass),
        ReadsTagZeroAtOnce(Rc::clone(&senders)),
    )
    .expect("bools fit");
    let reply = observed
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[true],
        })
        .expect("the call is accepted");
    let sent = senders.borrow_mut().pop().expect("the send is open");
    sent.push(one(0, 1));
    sent.done();
    let mut read = Vec::new();
    wait(reply, &mut read).expect("the send ends well");
    assert_eq!(read, [one(0, 0), one(0, 1)]);
}

/// An adapter's logic sends a call down, then runs it below, over a device
/// that answers on the test's thread while the caller waits on its own.
/// The send's reply ends, with a readout, while the run's is still open:
/// the call has not returned, so the caller has no reply to read it from.
/// It returns only once the run's reply ended and the logic returned: a
/// logic that waits below holds its call's reply, the readouts its sends
/// already gave included.
#[test]
fn a_call_returns_only_once_its_logic_returns_though_a_send_below_already_read() {
    const A_WHILE: Duration = Duration::from_millis(200);
    let (calls, answers) = mpsc::channel();
    let mut executor =
        compose(adapter_fn(no_state, send_then_run), HandsOver(calls)).expect("bools fit");
    let (returned, call_returned) = mpsc::channel();
    let caller = thread::spawn(move || {
        let reply = executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(0),
                instructions: &[true],
            })
            .expect("the call is accepted");
        returned
            .send(())
            .expect("the test waits for the call to return");
        let mut read = Vec::new();
        let end = wait(reply, &mut read);
        executor.close();
        (read, end)
    });
    let sent = answers.recv_timeout(PATIENT).expect("the send goes below");
    sent.push(one(0, 0));
    sent.done();
    let ran = answers.recv_timeout(PATIENT).expect("the run goes below");
    assert_eq!(
        call_returned.recv_timeout(A_WHILE),
        Err(RecvTimeoutError::Timeout),
        "the call waits for its logic, which waits on the run below"
    );
    ran.push(one(0, 1));
    ran.done();
    call_returned
        .recv_timeout(PATIENT)
        .expect("the call returns once the run ended");
    let (mut read, end) = caller.join().expect("the caller does not panic");
    end.expect("the send and the run end well");
    read.sort_by_key(|readout| readout.tag);
    assert_eq!(read, [one(0, 0), one(0, 1)]);
}

/// An observer that runs each call below, and passes up what it read, over
/// an adapter whose logic sends each call down, then runs it, over a device
/// that tags each readout with its count of reads so far: the shot reads
/// tag 0 at the send, then tag 1 at the run, but the adapter's reply gives
/// what its logic wrote, the run's tag 1, before what its send read. The
/// observer passes them up in the order of that reply, not in the order
/// the shot read them.
#[test]
fn an_observer_that_runs_passes_up_each_shots_readouts_in_the_replys_order() {
    let send_then_run = |(): &mut (),
                         input: &ProgramInput<'_, bool>,
                         below: &mut Below<'_, bool>,
                         outcomes: &mut Outcomes<'_>| {
        below.send(input.instructions())?;
        outcomes.extend(below.run(input.instructions())?);
        Ok(())
    };
    let counting = || {
        executor_fn(
            Capabilities::default(),
            |_| Ok(0),
            |reads: &mut u32, input: &ProgramInput<'_, bool>, outcomes: &mut Outcomes<'_>| {
                for &bit in input.instructions() {
                    let tag = *reads;
                    outcomes.push(Outcome::new(tag, bit.into()));
                    *reads += 1;
                }
                Ok(())
            },
        )
    };
    let tags = |out: &[ShotOutcome]| out.iter().map(|o| o.tag).collect::<Vec<_>>();
    let mut below_alone = (adapter_fn(no_state, send_then_run) >> counting()).expect("bools fit");
    let (reply, end) = run(&mut below_alone, &ShotSet::one(4), &[true]).expect("accepted");
    end.expect("the call ends well");
    assert_eq!(
        tags(&reply),
        [1, 0],
        "the reply below gives the run's first"
    );
    let observing = |(): &mut (),
                     input: &ProgramInput<'_, bool>,
                     below: &mut Below<'_, bool>,
                     outcomes: &mut Outcomes<'_>| {
        outcomes.extend(below.run(input.instructions())?);
        Ok(())
    };
    let below = (adapter_fn(no_state, send_then_run) >> counting()).expect("bools fit");
    let mut observed = (adapter_fn(no_state, observing) >> below).expect("bools fit");
    let (passed_up, end) = run(&mut observed, &ShotSet::one(4), &[true]).expect("accepted");
    end.expect("the call ends well");
    assert_eq!(tags(&passed_up), tags(&reply));
}

/// A decoder that ignores its run's refusal, and one that passes it on:
/// the run below on busy shot 0 was the call's first call below, so both
/// calls are refused, with the refusal below.
#[test]
fn a_first_run_refused_refuses_the_call_whatever_the_logic_returns() {
    let busy = || TestDoubleModel {
        refusing: Some(0),
        ..TestDoubleModel::default()
    };
    let ignoring = |(): &mut (),
                    input: &ProgramInput<'_, bool>,
                    below: &mut Below<'_, bool>,
                    _: &mut Outcomes<'_>| {
        let _ = below.run(input.instructions());
        Ok(())
    };
    let mut ignores = (adapter_fn(no_state, ignoring) >> below(busy())).expect("bools fit");
    let mut passes_on = (adapter_fn(no_state, flip_what_runs) >> below(busy())).expect("bools fit");
    let when_ignored = run(&mut ignores, &ShotSet::one(0), &[true]).expect_err("shot 0 is busy");
    let when_passed_on =
        run(&mut passes_on, &ShotSet::one(0), &[true]).expect_err("shot 0 is busy");
    assert_eq!(when_ignored, Rejected::Unavailable("busy".into()));
    assert_eq!(when_passed_on, Rejected::Unavailable("busy".into()));
}

/// A decoder that runs a read, then sends a batch the executor below
/// refuses: the run went down, so the call is not refused but fails, and
/// so does the adapter.
#[test]
fn a_send_refused_after_a_run_fails_the_call_and_the_adapter() {
    let model = TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    };
    let run_then_send = |(): &mut (),
                         _: &ProgramInput<'_, bool>,
                         below: &mut Below<'_, bool>,
                         _: &mut Outcomes<'_>| {
        below.run(&[true])?;
        below.send(&[false])
    };
    let mut executor = (adapter_fn(no_state, run_then_send) >> below(model)).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("the run went down");
    end.expect_err("the send was refused");
    let later = run(&mut executor, &ShotSet::one(0), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// A code's decoder that reads a syndrome bit below and sends a correction
/// (`false`, which reads nothing) where the bit is one, called on each of
/// eight shots in turn: a correction goes below after the shots whose bit
/// is one, and only after those. Each shot's bit comes from its own seed,
/// so the shot reads it alone as among the eight.
#[test]
fn a_logic_sends_below_a_call_it_chose_from_what_its_run_read() {
    let eight = shots(&[0, 1, 2, 3, 4, 5, 6, 7]);
    let (bare, _) =
        run(&mut below(TestDoubleModel::default()), &eight, &[true]).expect("the bits accept it");
    let ones: Vec<ShotId> = (bare.iter())
        .filter(|outcome| outcome.value == Value::One)
        .map(|outcome| outcome.shot)
        .collect();
    assert!(
        !ones.is_empty() && ones.len() < 8,
        "some syndrome bits are one, some zero: {ones:?}"
    );
    let correct_on_one = |(): &mut (),
                          _: &ProgramInput<'_, bool>,
                          below: &mut Below<'_, bool>,
                          _: &mut Outcomes<'_>| {
        let syndrome = below.run(&[true])?;
        if syndrome.iter().any(|outcome| outcome.value == Value::One) {
            below.send(&[false])?;
        }
        Ok(())
    };
    let (executor, calls) = recorded(below(TestDoubleModel::default()));
    let mut decoder = (adapter_fn(no_state, correct_on_one) >> executor).expect("bools fit");
    for shot in 0..8 {
        step(&mut decoder, &ShotSet::one(shot), &[true], "the call runs");
    }
    let expected: Vec<_> = (0..8)
        .flat_map(|shot| {
            let correction = ones.contains(&shot).then(|| (vec![shot], vec![false]));
            [Some((vec![shot], vec![true])), correction]
        })
        .flatten()
        .collect();
    assert_eq!(*calls.borrow(), expected);
}

/// Shot 0's batches fail below, and a decoder ignores its run's failure:
/// the call fails with the failure below all the same, and so does the
/// adapter.
#[test]
fn a_run_whose_reply_fails_fails_the_call_and_the_adapter_whatever_the_logic_returns() {
    let ignoring = |(): &mut (),
                    input: &ProgramInput<'_, bool>,
                    below: &mut Below<'_, bool>,
                    _: &mut Outcomes<'_>| {
        let _ = below.run(input.instructions());
        Ok(())
    };
    let model = TestDoubleModel {
        failing: Some(0),
        ..TestDoubleModel::default()
    };
    let mut executor = (adapter_fn(no_state, ignoring) >> below(model)).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("shot 0 accepts");
    assert_eq!(end, Err("lost".to_owned()));
    let later = run(&mut executor, &ShotSet::one(1), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// Shot 0's batches fail below, "lost", and a decoder returns its own
/// error, "no syndrome", when its run fails: the call fails with the
/// failure below, which says what went wrong, not the decoder's.
#[test]
fn a_call_failed_below_ends_with_the_failure_below_whatever_the_logics_err() {
    let mapping = |(): &mut (),
                   input: &ProgramInput<'_, bool>,
                   below: &mut Below<'_, bool>,
                   _: &mut Outcomes<'_>| {
        below
            .run(input.instructions())
            .map_err(|_| "no syndrome".to_owned())?;
        Ok(())
    };
    let model = TestDoubleModel {
        failing: Some(0),
        ..TestDoubleModel::default()
    };
    let mut executor = (adapter_fn(no_state, mapping) >> below(model)).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("shot 0 accepts");
    assert_eq!(end, Err("lost".to_owned()));
}

/// Shot 0's batches are cancelled below, and a decoder ignores its run's
/// result: the run cannot read them, so the call fails all the same, where
/// a send's reply cancelled ends it cancelled
/// (`a_reply_below_cancelled_ends_the_call_cancelled`), and so does the
/// adapter.
#[test]
fn a_run_whose_reply_is_cancelled_fails_the_call_and_the_adapter_whatever_the_logic_returns() {
    let ignoring = |(): &mut (),
                    input: &ProgramInput<'_, bool>,
                    below: &mut Below<'_, bool>,
                    _: &mut Outcomes<'_>| {
        let _ = below.run(input.instructions());
        Ok(())
    };
    let model = TestDoubleModel {
        cancelling: Some(0),
        ..TestDoubleModel::default()
    };
    let mut executor = (adapter_fn(no_state, ignoring) >> below(model)).expect("bools fit");
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[true],
        })
        .expect("shot 0 accepts");
    let end = reply.poll_events(&mut Vec::new(), Waker::noop());
    assert!(matches!(end, ReplyStatus::Failed(_)), "{end:?}");
    let later = run(&mut executor, &ShotSet::one(1), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// An executor with a bug, that answers every call with the readouts it
/// was made with, whatever the call's shots and instructions.
struct Answering(Vec<ShotOutcome>);

impl Executor<InstructionBatch<bool>> for Answering {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, _: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        Ok(ImmediateReply::done(self.0.clone()))
    }
}

impl Describe for Answering {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default().with_qubits(1)
    }
}

impl Close for Answering {
    type Report = ();

    fn close(&mut self) {}
}

/// Readout `tag` of shot `shot`, a one.
fn one(shot: ShotId, tag: u32) -> ShotOutcome {
    ShotOutcome::new(shot, tag, Value::One)
}

/// An executor below that answers a run on the target chain's shots 2
/// and 5, and also a shot it was not given: before them (1), between them
/// (3), or after them (9). The reply is not that of the run, so the call
/// fails, naming the shot, and so does the adapter.
#[test]
fn a_run_answered_for_a_shot_it_did_not_run_fails_the_call_and_the_adapter() {
    let two = shots(&[2, 5]);
    for stray in [1, 3, 9] {
        let mut executor = compose(
            adapter_fn(no_state, flip_what_runs),
            Answering(vec![one(2, 0), one(stray, 0), one(5, 0)]),
        )
        .expect("bools fit");
        let (_, end) = run(&mut executor, &two, &[true]).expect("shots 2 and 5 accept");
        let Err(reason) = end else {
            panic!("shot {stray} did not run, so the call fails");
        };
        assert!(reason.contains(&format!("shot {stray},")), "{reason}");
        let later = run(&mut executor, &two, &[true]).expect_err("the adapter failed");
        assert!(
            matches!(later, Rejected::EarlierFailure(_)),
            "shot {stray}: {later:?}"
        );
    }
}

/// An executor below that reads tag 1 of shot 3 twice, on a run on shots
/// 0 and 3, which both read tag 1: in a row, with other readouts between
/// them, after another tag of shot 3, and among the 21 readouts of a
/// decoder's rounds, of which it is the smallest tag. The two cannot be
/// told apart, so the call fails, naming the tag and the shot, and so
/// does the adapter.
#[test]
fn a_run_that_reads_a_tag_of_a_shot_twice_fails_the_call_and_the_adapter() {
    let rounds = (1..21).chain([1]).map(|tag| one(3, tag));
    for answer in [
        vec![one(0, 1), one(3, 1), one(3, 1)],
        vec![one(3, 1), one(0, 1), one(3, 0), one(3, 1)],
        vec![one(3, 0), one(3, 1), one(0, 1), one(3, 2), one(3, 1)],
        [one(0, 1)].into_iter().chain(rounds).collect(),
    ] {
        let mut executor =
            compose(adapter_fn(no_state, flip_what_runs), Answering(answer)).expect("bools fit");
        let (_, end) = run(&mut executor, &shots(&[0, 3]), &[true]).expect("shots 0, 3 accept");
        let reason = end.expect_err("tag 1 of shot 3 was read twice");
        assert!(reason.contains("tag 1 of shot 3 twice"), "{reason}");
        let later = run(&mut executor, &ShotSet::one(0), &[true]).expect_err("the adapter failed");
        assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
    }
}

/// Shot 0's batches fail below: a decoder that sends again after its run
/// failed gets the run's failure, and nothing more goes down.
#[test]
fn nothing_goes_down_after_a_run_failed() {
    let model = TestDoubleModel {
        failing: Some(0),
        ..TestDoubleModel::default()
    };
    let seen = Rc::clone(&model.seen);
    let sent = Rc::new(RefCell::new(None));
    let retrying = {
        let sent = Rc::clone(&sent);
        adapter_fn(
            no_state,
            move |(): &mut (),
                  input: &ProgramInput<'_, bool>,
                  below: &mut Below<'_, bool>,
                  _: &mut Outcomes<'_>| {
                let _ = below.run(input.instructions());
                *sent.borrow_mut() = Some(below.send(input.instructions()));
                Ok(())
            },
        )
    };
    let mut executor = (retrying >> below(model)).expect("bools fit");
    let (_, end) = run(&mut executor, &ShotSet::one(0), &[true]).expect("shot 0 accepts");
    end.expect_err("the run failed");
    assert_eq!(*sent.borrow(), Some(Err("lost".into())));
    assert_eq!(seen.borrow().calls.get(&0), Some(&1), "the run alone ran");
}

/// A decoder on shots 0, 1 and 2 that writes a readout of shot 1 alone:
/// the call gives it for shot 1, and nothing for the others, where an
/// [`Outcome`] answers every shot of the call
/// (`an_outcome_written_answers_every_shot_of_the_call`).
#[test]
fn a_readout_written_for_a_shot_answers_that_shot_alone() {
    let decoding = |(): &mut (),
                    _: &ProgramInput<'_, bool>,
                    _: &mut Below<'_, bool>,
                    outcomes: &mut Outcomes<'_>| {
        outcomes.extend([one(1, 9)]);
        Ok(())
    };
    let mut executor =
        (adapter_fn(no_state, decoding) >> below(TestDoubleModel::default())).expect("bools fit");
    let (written, end) = run(&mut executor, &shots(&[0, 1, 2]), &[true]).expect("accepted");
    end.expect("the logic does not fail");
    assert_eq!(values(&written), [(1, 9, Value::One)]);
}

/// A decoder with a bug, that writes a readout of shot 9 in a call on
/// shots 2 and 0: it panics, the panic reaches the caller, and the adapter
/// fails.
#[test]
fn a_readout_written_for_a_shot_the_logic_does_not_serve_panics() {
    let buggy = |(): &mut (),
                 _: &ProgramInput<'_, bool>,
                 _: &mut Below<'_, bool>,
                 outcomes: &mut Outcomes<'_>| {
        outcomes.extend([one(9, 0)]);
        Ok(())
    };
    let mut executor =
        (adapter_fn(no_state, buggy) >> below(TestDoubleModel::default())).expect("bools fit");
    catch_unwind(AssertUnwindSafe(|| {
        run(&mut executor, &shots(&[2, 0]), &[true])
    }))
    .expect_err("the panic reaches the caller");
    let later = run(&mut executor, &ShotSet::one(0), &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// The state of a profiler before its first call: no call counted.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature a state closure has"
)]
fn no_calls(_: ShotContext) -> Result<u64, Rejected> {
    Ok(0)
}

/// The logic of a profiler: sends each call down as it is, then counts it
/// in its state, so that a call refused below is not counted.
fn count_and_pass<I>(
    calls: &mut u64,
    input: &ProgramInput<'_, I>,
    below: &mut Below<'_, I>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    below.send(input.instructions())?;
    *calls += 1;
    Ok(())
}

/// The logic of a decoder over the device: runs each call below, then
/// writes what it read, as a decoder writes what it decodes from the
/// readouts below, whatever its state.
fn decode<S>(
    _: &mut S,
    input: &ProgramInput<'_, Op>,
    below: &mut Below<'_, Op>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    let read = below.run(input.instructions())?;
    outcomes.extend(read);
    Ok(())
}

/// The check of a decoder with no rule for a stop, that decodes each tag
/// once a call: it refuses a stop, then a tag read twice, before anything
/// runs.
fn decodable(input: &ProgramInput<'_, Op>) -> Result<(), Rejected> {
    let ops = input.instructions();
    if let Some(at) = ops.iter().position(|&op| op == Op::Stop) {
        return Err(Rejected::Instruction {
            at,
            reason: "the decoder has no rule for a stop".into(),
        });
    }
    check_unique_tags(readouts(ops))
}

/// A profiler over the device, closed: it refuses every call, as the
/// device alone does (the conformance kit's check).
#[test]
fn a_closed_profiler_refuses_every_call() {
    let mut profiled = (adapter_fn(no_calls, count_and_pass) >> device()).expect("ops fit");
    conformance::rejects_calls_once_closed::<InstructionBatch<Op>, _>(
        &mut profiled,
        InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Flip],
        },
    );
}

/// A profiler over the device, reporting its count of calls, called once:
/// its second close reports nothing, its count included, as the device's
/// does (the conformance kit's check).
#[test]
fn a_profiler_reports_once() {
    let mut profiled = (adapter_fn(no_calls, count_and_pass).state_report(|calls: &u64| *calls)
        >> device())
    .expect("ops fit");
    step(
        &mut profiled,
        &ShotSet::one(0),
        &[Op::Flip],
        "the call runs",
    );
    conformance::reports_once(&mut profiled);
}

/// A decoder with no rule for a stop, over the device: a call with a stop
/// is refused by the decoder's check, and the chain runs the next call
/// (the conformance kit's check), as the device runs one after a refusal
/// of its own.
#[test]
fn a_call_the_adapter_refuses_leaves_the_chain_usable() {
    let mut decoded = (adapter_fn(no_state, decode).check(decodable) >> device()).expect("ops fit");
    let shot = ShotSet::one(0);
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<Op>, _>(
        &mut decoded,
        InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Flip, Op::Stop],
        },
        InstructionBatchRef {
            shots: &shot,
            instructions: &[Op::Flip, Op::Read(0)],
        },
    );
}

/// A profiler over a device that holds two shots at once, called on three
/// shots: the device refuses the call, the profiler refuses it with that
/// refusal, and the chain runs a call on two shots (the conformance kit's
/// check).
#[test]
fn a_call_refused_below_leaves_the_chain_usable() {
    let mut profiled = (adapter_fn(no_calls, count_and_pass)
        >> device_promising(promise().with_max_shots(2)))
    .expect("ops fit");
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<Op>, _>(
        &mut profiled,
        InstructionBatchRef {
            shots: &shots(&[0, 1, 2]),
            instructions: &[Op::Flip],
        },
        InstructionBatchRef {
            shots: &shots(&[0, 1]),
            instructions: &[Op::Flip, Op::Read(0)],
        },
    );
}

/// The batches of `refuses_calls_after_a_failure` on the device: a stop
/// on shots 0 and 1, then a flip on shot 2, which the stop did not name,
/// and a read on shot 0.
fn after_a_stop(check: impl FnOnce(InstructionBatchRef<'_, Op>, &[InstructionBatchRef<'_, Op>])) {
    let (stopped, other) = (shots(&[0, 1]), ShotSet::one(2));
    check(
        InstructionBatchRef {
            shots: &stopped,
            instructions: &[Op::Stop],
        },
        &[
            InstructionBatchRef {
                shots: &other,
                instructions: &[Op::Flip],
            },
            InstructionBatchRef {
                shots: &ShotSet::one(0),
                instructions: &[Op::Read(0)],
            },
        ],
    );
}

/// A profiler over the device, whose call's send stops the device: the
/// call fails with the device's reason, and the chain refuses every later
/// call, naming it (the conformance kit's check), as the device alone
/// does.
#[test]
fn after_a_failure_below_the_chain_refuses_every_later_call() {
    let mut profiled = (adapter_fn(no_calls, count_and_pass) >> device()).expect("ops fit");
    after_a_stop(|failing, later| {
        conformance::refuses_calls_after_a_failure(&mut profiled, failing, later);
    });
}

/// A decoder over the device, whose run below stops the device: the call
/// fails with the device's reason, and the chain refuses every later call,
/// naming it (the conformance kit's check).
#[test]
fn after_a_run_failed_below_the_chain_refuses_every_later_call() {
    let mut decoded = (adapter_fn(no_state, decode) >> device()).expect("ops fit");
    after_a_stop(|failing, later| {
        conformance::refuses_calls_after_a_failure(&mut decoded, failing, later);
    });
}

/// A profiler over the device placed on its own thread, whose call's send
/// stops the device there: the failure comes up through the placement
/// with its reason, and the chain refuses every later call, naming it
/// (the conformance kit's check).
#[test]
fn after_a_failure_below_on_its_own_thread_the_chain_refuses_every_later_call() {
    let mut profiled =
        (adapter_fn(no_calls, count_and_pass) >> device().run_on(OwnThread)).expect("ops fit");
    ends_soon(move || {
        after_a_stop(|failing, later| {
            conformance::refuses_calls_after_a_failure(&mut profiled, failing, later);
        });
    });
}

/// A decoder over the device, its check refusing a tag read twice: a tag
/// repeated within a batch is refused, and only that (the conformance
/// kit's check).
#[test]
fn a_decoder_refuses_a_tag_repeated_within_a_batch() {
    let mut decoded = (adapter_fn(no_state, decode).check(decodable) >> device()).expect("ops fit");
    conformance::refuses_a_tag_repeated_within_a_batch(
        &mut decoded,
        &shots(&[0, 1]),
        &Op::Read(0),
        &Op::Read(1),
        &Op::ReadInverted(0),
    );
}

/// A profiler over the device: a second call reads the flip of the first,
/// on each shot, as the device alone does (the conformance kit's check of
/// `resumable_shots`).
#[test]
fn a_profiler_continues_a_shot_from_one_call_to_the_next() {
    conformance::continues_a_shot_from_one_call_to_the_next(
        || (adapter_fn(no_calls, count_and_pass) >> device()).expect("ops fit"),
        &shots(&[0, 1]),
        &[Op::Flip],
        &[Op::Read(0)],
    );
}

/// A profiler over a simulator whose shots each read bits drawn from
/// their own seed: each of shots 1, 3 and 5 reads alone, among the
/// others, and split into calls, what it reads among them (the
/// conformance kit's check of `replayable`).
#[test]
fn a_profiler_gives_each_shot_alone_what_it_gives_among_others() {
    let ops: Vec<_> = (0..16).map(Op::Read).collect();
    conformance::gives_each_shot_alone_what_it_gives_among_others(
        || {
            let simulator = executor_fn(promise(), draw_value, read_drawn_bits)
                .seed(7)
                .shots(OnePerShot);
            (adapter_fn(no_calls, count_and_pass) >> simulator).expect("ops fit")
        },
        &shots(&[1, 3, 5]),
        &[&ops[..8], &ops[8..]],
    );
}

/// A profiler over a simulator of the tests' set: a call outside the set
/// is refused below, at its index, and the profiler refuses it with that
/// refusal (the conformance kit's check).
#[test]
fn a_profiler_refuses_calls_outside_the_set_below() {
    let flip = flip_and_toss(&bits_set())[0][0].clone();
    let simulator = TestDoubleExecutor::with_model(Bits::default(), 7);
    conformance::refuses_calls_outside_its_set(
        &mut (adapter_fn(no_calls, count_and_pass) >> simulator).expect("the set fits"),
        &shots(&[0, 1]),
        &bits_set(),
        &flip,
    );
}

/// A profiler composed over the device after the device stopped: its
/// first call is refused with the device's failure, as the device alone
/// refuses it.
#[test]
fn an_adapter_over_a_failed_executor_refuses_with_its_failure() {
    let mut device = device();
    let (_, end) = run(&mut device, &ShotSet::one(0), &[Op::Stop]).expect("the stop is sent");
    let reason = end.expect_err("the device stopped");
    let mut profiled = (adapter_fn(no_calls, count_and_pass) >> device).expect("ops fit");
    let refused =
        run(&mut profiled, &ShotSet::one(1), &[Op::Flip]).expect_err("the device below stopped");
    assert_eq!(refused, Rejected::EarlierFailure(reason));
}

/// An executor, around `E`, that counts the closes it is given.
struct CloseCounted<E> {
    executor: E,
    closes: Rc<Cell<u32>>,
}

/// `executor`, its closes counted, and the count.
fn close_counted<E>(executor: E) -> (CloseCounted<E>, Rc<Cell<u32>>) {
    let closes = Rc::default();
    let counted = CloseCounted {
        executor,
        closes: Rc::clone(&closes),
    };
    (counted, closes)
}

impl<E: Executor<InstructionBatch<bool>>> Executor<InstructionBatch<bool>> for CloseCounted<E> {
    type Reply = E::Reply;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<E::Reply, Rejected> {
        self.executor.call(batch)
    }
}

impl<E: Describe> Describe for CloseCounted<E> {
    fn capabilities(&self) -> Capabilities {
        self.executor.capabilities()
    }
}

impl<E: Close> Close for CloseCounted<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.closes.set(self.closes.get() + 1);
        self.executor.close()
    }
}

/// An observer and a profiler with a report, each over an executor and
/// closed twice: the executor below each is closed once, whether the
/// adapter reports or not, as a placed executor's close hands a job to
/// its thread.
#[test]
fn a_second_close_closes_nothing_below() {
    let (counted, observed_closes) = close_counted(below(TestDoubleModel::default()));
    let mut observed = compose(adapter_fn(no_state, pass), counted).expect("bools fit");
    observed.close();
    observed.close();
    let (counted, profiled_closes) = close_counted(below(TestDoubleModel::default()));
    let counting = adapter_fn(no_calls, count_and_pass).state_report(|calls: &u64| *calls);
    let mut profiled = compose(counting, counted).expect("bools fit");
    profiled.close();
    profiled.close();
    assert_eq!(
        (observed_closes.get(), profiled_closes.get()),
        (1, 1),
        "the observer's and the profiler's closes below"
    );
}

/// An observer and a profiler with a report, each over an executor whose
/// close panics, and closed again once the panic reached the caller: the
/// executor below each is closed once, the second close leaving it be.
#[test]
fn a_close_below_that_panicked_is_not_run_again() {
    let (counted, observed_closes) =
        close_counted(PanicsOnClose(below(TestDoubleModel::default())));
    let mut observed = compose(adapter_fn(no_state, pass), counted).expect("bools fit");
    catch_unwind(AssertUnwindSafe(|| observed.close())).expect_err("the close below panics");
    observed.close();
    let (counted, profiled_closes) =
        close_counted(PanicsOnClose(below(TestDoubleModel::default())));
    let counting = adapter_fn(no_calls, count_and_pass).state_report(|calls: &u64| *calls);
    let mut profiled = compose(counting, counted).expect("bools fit");
    catch_unwind(AssertUnwindSafe(|| profiled.close())).expect_err("the close below panics");
    profiled.close();
    assert_eq!(
        (observed_closes.get(), profiled_closes.get()),
        (1, 1),
        "the observer's and the profiler's closes below"
    );
}

/// The logic of an observer over states of any type: it sends each call
/// down.
pub(super) fn send_down<S>(
    _: &mut S,
    input: &ProgramInput<'_, bool>,
    below: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    below.send(input.instructions())
}

/// The logic of a decoder over states of any type that finds no syndrome:
/// the call fails.
pub(super) fn finds_no_syndrome<S>(
    _: &mut S,
    _: &ProgramInput<'_, bool>,
    _: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    Err("no syndrome".to_owned().into())
}

/// The logic of a decoder over states of any type, with a bug: it panics.
pub(super) fn panics_decoding<S>(
    _: &mut S,
    _: &ProgramInput<'_, bool>,
    _: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    panic!("the logic has a bug")
}

/// An observer and a profiler with a report, each holding a state that
/// fails to power down (a bug of its code), called once, then closed
/// twice: the first close frees the state and closes the executor below,
/// then the state's panic reaches the closer; the second closes nothing
/// below.
#[test]
fn a_close_whose_state_drop_panics_closes_below_then_raises_it() {
    let freed = Freed::default();
    let (counted, observed_closes) = close_counted(below(TestDoubleModel::default()));
    let observing = adapter_fn(powering_down_badly(&freed, None), send_down);
    let mut observed = compose(observing, counted).expect("bools fit");
    step(&mut observed, &ShotSet::one(0), &[true], "the call runs");
    let observed_panic =
        catch_unwind(AssertUnwindSafe(|| observed.close())).expect_err("the drop panics");
    let observed_first = observed_closes.get();
    observed.close();
    let (counted, profiled_closes) = close_counted(below(TestDoubleModel::default()));
    let profiling = adapter_fn(powering_down_badly(&freed, None), send_down)
        .state_report(|_: &PowersDownBadly| 0_u64);
    let mut profiled = compose(profiling, counted).expect("bools fit");
    step(&mut profiled, &ShotSet::one(0), &[true], "the call runs");
    let profiled_panic =
        catch_unwind(AssertUnwindSafe(|| profiled.close())).expect_err("the drop panics");
    let profiled_first = profiled_closes.get();
    profiled.close();
    assert_eq!(
        (
            observed_panic.downcast_ref(),
            profiled_panic.downcast_ref(),
            freed_so_far(&freed)
        ),
        (
            Some(&PowerDownFailed(0)),
            Some(&PowerDownFailed(0)),
            vec![0, 0]
        ),
        "the panics raised, and the states freed"
    );
    assert_eq!(
        (
            (observed_first, observed_closes.get()),
            (profiled_first, profiled_closes.get())
        ),
        ((1, 1), (1, 1)),
        "the observer's and the profiler's closes below: after the panic, after a second close"
    );
}

/// An observer whose state fails to power down ran a call, and is dropped
/// unclosed: its drop frees the state and raises no panic of its drop,
/// which only a close reports.
#[test]
fn dropped_it_frees_its_state_raising_no_panic_of_its_drop() {
    let freed = Freed::default();
    let observing = adapter_fn(powering_down_badly(&freed, None), send_down);
    let mut observed = compose(observing, below(TestDoubleModel::default())).expect("bools fit");
    step(&mut observed, &ShotSet::one(0), &[true], "the call runs");
    catch_unwind(AssertUnwindSafe(|| drop(observed)))
        .expect("a drop raises no panic of the state's drop");
    assert_eq!(freed_so_far(&freed), [0]);
}

/// The drops seen, in order, by name.
pub(super) type DropLog = Rc<RefCell<Vec<&'static str>>>;

/// A value that writes its name in a [`DropLog`] as it is dropped.
pub(super) struct Dropped(pub(super) &'static str, pub(super) DropLog);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.1.borrow_mut().push(self.0);
    }
}

/// An executor below that holds a [`Dropped`] named `below`, and answers
/// each call done, reading nothing.
pub(super) struct DroppedBelow {
    _dropped: Dropped,
}

impl DroppedBelow {
    pub(super) fn new(log: &DropLog) -> Self {
        Self {
            _dropped: Dropped("below", Rc::clone(log)),
        }
    }
}

impl Executor<InstructionBatch<bool>> for DroppedBelow {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, _: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        Ok(ImmediateReply::default())
    }
}

impl Describe for DroppedBelow {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Close for DroppedBelow {
    type Report = ();

    fn close(&mut self) {}
}

/// An observer whose state powers its device down through a connection
/// its logic holds ran a call, and is dropped unclosed: the state is
/// dropped first, while the logic, with what it holds, and the executor
/// below are still there, as `close` frees it before closing below.
#[test]
fn dropped_it_frees_its_state_before_its_closures_and_the_executor_below() {
    let log = DropLog::default();
    let (making, connection) = (Rc::clone(&log), Dropped("logic", Rc::clone(&log)));
    let observing = adapter_fn(
        move |_: ShotContext| Ok(Dropped("state", Rc::clone(&making))),
        move |_: &mut Dropped,
              input: &ProgramInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| {
            let _held = &connection;
            below.send(input.instructions())
        },
    );
    let mut observed = compose(observing, DroppedBelow::new(&log)).expect("bools fit");
    step(&mut observed, &ShotSet::one(0), &[true], "the call runs");
    drop(observed);
    let dropped = log.borrow();
    assert_eq!(dropped[0], "state");
    assert_eq!(
        dropped.len(),
        3,
        "the state, the logic and below: {dropped:?}"
    );
}

/// A profiler whose four closures (state, logic, check, report) each hold a
/// handle on a connection that fails to release (a bug of the
/// implementer's) ran a call, and is dropped unclosed: its drop releases
/// each handle in a catch of its own, and raises none of their panics.
#[test]
fn dropped_it_frees_what_its_closures_capture_raising_no_panic_of_their_drops() {
    let (handle, released) = ReleasesBadly::new();
    let (making, logic, checking, reporting) =
        (handle.another(), handle.another(), handle.another(), handle);
    let profiling = adapter_fn(
        move |_: ShotContext| {
            let _held = &making;
            Ok(0_u64)
        },
        move |calls: &mut u64,
              input: &ProgramInput<'_, bool>,
              below: &mut Below<'_, bool>,
              _: &mut Outcomes<'_>| {
            let _held = &logic;
            *calls += 1;
            below.send(input.instructions())
        },
    )
    .check(move |_: &ProgramInput<'_, bool>| {
        let _held = &checking;
        Ok(())
    })
    .state_report(move |calls: &u64| {
        let _held = &reporting;
        *calls
    });
    let mut profiled = compose(profiling, below(TestDoubleModel::default())).expect("bools fit");
    step(&mut profiled, &ShotSet::one(0), &[true], "the call runs");
    catch_unwind(AssertUnwindSafe(|| drop(profiled)))
        .expect("a drop raises no panic of a capture's drop");
    assert_eq!(released.load(Ordering::SeqCst), 4);
}

/// A profiler's check and report maker each hold a handle on its
/// connection that fails to release (a bug), and the driver replaces each
/// with another: each builder frees the closure it replaces, and raises no
/// panic of its drop, as `drop` does.
#[test]
fn a_replaced_closure_is_freed_raising_no_panic_of_its_drop() {
    let (handle, released) = ReleasesBadly::new();
    let reporting = handle.another();
    let profiling = adapter_fn(|_: ShotContext| Ok(0_u64), count_calls)
        .check(move |_: &ProgramInput<'_, bool>| {
            let _held = &handle;
            Ok(())
        })
        .state_report(move |calls: &u64| {
            let _held = &reporting;
            *calls
        });
    let replaced = catch_unwind(AssertUnwindSafe(move || {
        profiling
            .check(|_: &ProgramInput<'_, bool>| Ok(()))
            .state_report(|calls: &u64| *calls)
    }))
    .expect("a builder raises no panic of the replaced closure's drop");
    assert_eq!(released.load(Ordering::SeqCst), 2);
    drop(replaced);
}

/// A decoder whose state fails to power down finds no syndrome in its
/// first call: the call frees the state made for it, whose panic then
/// reaches the caller, as an executor's would.
#[test]
fn a_failed_call_frees_the_state_made_for_it_and_raises_its_drops_panic() {
    let freed = Freed::default();
    let decoding = adapter_fn(powering_down_badly(&freed, None), finds_no_syndrome);
    let mut decoded = compose(decoding, below(TestDoubleModel::default())).expect("bools fit");
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        run(&mut decoded, &ShotSet::one(0), &[true])
    }))
    .expect_err("the state's drop panics");
    assert_eq!(
        (panicked.downcast_ref(), freed_so_far(&freed)),
        (Some(&PowerDownFailed(0)), vec![0])
    );
}

/// An observer whose state fails to power down, over an executor busy on
/// shot 0: its first call, refused below, frees the state made for it,
/// whose panic then reaches the caller, as an executor's would.
#[test]
fn a_refused_call_frees_the_state_made_for_it_and_raises_its_drops_panic() {
    let freed = Freed::default();
    let busy = TestDoubleModel {
        refusing: Some(0),
        ..TestDoubleModel::default()
    };
    let observing = adapter_fn(powering_down_badly(&freed, None), send_down);
    let mut observed = compose(observing, below(busy)).expect("bools fit");
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        run(&mut observed, &ShotSet::one(0), &[true])
    }))
    .expect_err("the state's drop panics");
    assert_eq!(
        (panicked.downcast_ref(), freed_so_far(&freed)),
        (Some(&PowerDownFailed(0)), vec![0])
    );
}

/// A decoder whose state fails to power down, and whose logic panics (two
/// bugs), in its first call: the logic's panic reaches the caller, and the
/// state, freed as it unwinds, leaves its own to the panic hook, rather
/// than abort the process.
#[test]
fn a_logics_panic_reaches_the_caller_though_its_new_states_drop_panics_too() {
    let freed = Freed::default();
    let decoding = adapter_fn(powering_down_badly(&freed, None), panics_decoding);
    let mut decoded = compose(decoding, below(TestDoubleModel::default())).expect("bools fit");
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        run(&mut decoded, &ShotSet::one(0), &[true])
    }))
    .expect_err("the logic panics");
    assert_eq!(
        (panic_text(&*panicked), freed_so_far(&freed)),
        (Some("the logic has a bug"), vec![0])
    );
}

/// A decoder whose logic failed on a call: closing it closes the executor
/// below all the same.
#[test]
fn a_failed_adapter_closes_what_is_below() {
    let failing = |(): &mut (),
                   _: &ProgramInput<'_, bool>,
                   _: &mut Below<'_, bool>,
                   _: &mut Outcomes<'_>| { Err("no syndrome".into()) };
    let (counted, closes) = close_counted(below(TestDoubleModel::default()));
    let mut decoded = compose(adapter_fn(no_state, failing), counted).expect("bools fit");
    let (_, end) = run(&mut decoded, &ShotSet::one(0), &[true]).expect("the call is accepted");
    end.expect_err("the logic failed");
    decoded.close();
    assert_eq!(closes.get(), 1);
}

/// A logic with a bug that panicked: closing the adapter closes the
/// executor below all the same.
#[test]
fn an_adapter_whose_logic_panicked_closes_what_is_below() {
    let buggy = |(): &mut (),
                 _: &ProgramInput<'_, bool>,
                 _: &mut Below<'_, bool>,
                 _: &mut Outcomes<'_>|
     -> Result<(), AdapterError> { panic!("the logic has a bug") };
    let (counted, closes) = close_counted(below(TestDoubleModel::default()));
    let mut executor = compose(adapter_fn(no_state, buggy), counted).expect("bools fit");
    catch_unwind(AssertUnwindSafe(|| {
        run(&mut executor, &ShotSet::one(0), &[true])
    }))
    .expect_err("the panic reaches the caller");
    executor.close();
    assert_eq!(closes.get(), 1);
}

/// The senders of the replies a [`HeldOpen`] executor gave, in the order
/// of its calls, shared with the test.
type Senders = Rc<RefCell<Vec<ReplySender<ShotOutcome>>>>;

/// An executor below that holds each call's reply open, for the test to
/// end, and keeps no rule of its own: a call in flight ends only as the
/// test ends it.
struct HeldOpen(Senders);

impl Executor<InstructionBatch<bool>> for HeldOpen {
    type Reply = IncrementalReply<ShotOutcome>;

    fn call(&mut self, _: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        let (sender, reply) = reply_channel();
        self.0.borrow_mut().push(sender);
        Ok(reply)
    }
}

impl Describe for HeldOpen {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Close for HeldOpen {
    type Report = ();

    fn close(&mut self) {}
}

/// An executor below that reads tag 0 on the call's first shot within
/// its call, then holds the reply open, as [`HeldOpen`] does.
struct ReadsTagZeroAtOnce(Senders);

impl Executor<InstructionBatch<bool>> for ReadsTagZeroAtOnce {
    type Reply = IncrementalReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        let (sender, reply) = reply_channel();
        let shot = *batch.shots.iter().next().expect("a call has a shot");
        sender.push(one(shot, 0));
        self.0.borrow_mut().push(sender);
        Ok(reply)
    }
}

impl Describe for ReadsTagZeroAtOnce {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Close for ReadsTagZeroAtOnce {
    type Report = ();

    fn close(&mut self) {}
}

/// An executor below that hands each call's reply sender to the test, on
/// whatever thread the test answers it from, as a device does, and keeps
/// no rule of its own.
struct HandsOver(mpsc::Sender<ReplySender<ShotOutcome>>);

impl Executor<InstructionBatch<bool>> for HandsOver {
    type Reply = IncrementalReply<ShotOutcome>;

    fn call(&mut self, _: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        let (sender, reply) = reply_channel();
        self.0
            .send(sender)
            .expect("the test receives every call's reply sender");
        Ok(reply)
    }
}

impl Describe for HandsOver {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Close for HandsOver {
    type Report = ();

    fn close(&mut self) {}
}

/// The logic of an adapter that sends each call down, then runs it below
/// and writes what the run read.
fn send_then_run(
    (): &mut (),
    input: &ProgramInput<'_, bool>,
    below: &mut Below<'_, bool>,
    out: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    below.send(input.instructions())?;
    out.extend(below.run(input.instructions())?);
    Ok(())
}

/// An executor below whose replies are pending for `pending_for` polls,
/// then done, reading nothing, and fail to release their connection as
/// they are dropped (a bug): each counts itself in `released`, then
/// panics.
struct ReleasingBadly {
    released: Rc<Cell<usize>>,
    pending_for: usize,
}

impl ReleasingBadly {
    /// Its replies stay pending.
    fn pending(released: &Rc<Cell<usize>>) -> Self {
        Self {
            released: Rc::clone(released),
            pending_for: usize::MAX,
        }
    }
}

/// A [`ReleasingBadly`]'s reply: pending for `pending_for` more polls.
struct ReleasedBadly {
    released: Rc<Cell<usize>>,
    pending_for: usize,
}

impl Reply for ReleasedBadly {
    type Item = ShotOutcome;

    fn poll_events(&mut self, _: &mut Vec<ShotOutcome>, _: &Waker) -> ReplyStatus {
        if self.pending_for == 0 {
            return ReplyStatus::Done;
        }
        self.pending_for -= 1;
        ReplyStatus::Pending
    }
}

impl Drop for ReleasedBadly {
    fn drop(&mut self) {
        self.released.set(self.released.get() + 1);
        panic!("a reply fails to release its connection");
    }
}

impl Executor<InstructionBatch<bool>> for ReleasingBadly {
    type Reply = ReleasedBadly;

    fn call(&mut self, _: InstructionBatchRef<'_, bool>) -> Result<ReleasedBadly, Rejected> {
        Ok(ReleasedBadly {
            released: Rc::clone(&self.released),
            pending_for: self.pending_for,
        })
    }
}

impl Describe for ReleasingBadly {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Close for ReleasingBadly {
    type Report = ();

    fn close(&mut self) {}
}

/// The logic of an observer that sends each call down twice.
fn send_twice(
    (): &mut (),
    input: &ProgramInput<'_, bool>,
    below: &mut Below<'_, bool>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    below.send(input.instructions())?;
    below.send(input.instructions())
}

/// An observer sends each call down twice, over an executor whose replies
/// stay pending and panic as they are dropped: the call's reply, dropped
/// unread, frees both, each in a catch of its own, and raises no panic of
/// their drops, rather than abort the process.
#[test]
fn a_dropped_replys_pending_sends_are_freed_raising_no_panic_of_their_drops() {
    let released = Rc::new(Cell::new(0));
    let below = ReleasingBadly::pending(&released);
    let mut observed = compose(adapter_fn(no_state, send_twice), below).expect("bools fit");
    let reply = observed
        .call(InstructionBatchRef {
            shots: &shots(&[0]),
            instructions: &[true],
        })
        .expect("the call is accepted");
    catch_unwind(AssertUnwindSafe(|| drop(reply))).expect("a drop raises no panic");
    assert_eq!(released.get(), 2);
}

/// An observer sends a call down twice, then its logic panics (a bug),
/// over an executor whose replies stay pending and panic as they are
/// dropped: the logic's panic reaches the caller once both replies are
/// freed, each in a catch of its own, rather than abort the process.
#[test]
fn a_failed_calls_pending_sends_are_freed_raising_no_panic_of_their_drops() {
    let released = Rc::new(Cell::new(0));
    let below = ReleasingBadly::pending(&released);
    let buggy = |(): &mut (),
                 input: &ProgramInput<'_, bool>,
                 below: &mut Below<'_, bool>,
                 outcomes: &mut Outcomes<'_>|
     -> Result<(), AdapterError> {
        send_twice(&mut (), input, below, outcomes)?;
        panic!("the logic has a bug")
    };
    let mut observed = compose(adapter_fn(no_state, buggy), below).expect("bools fit");
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        observed.call(InstructionBatchRef {
            shots: &shots(&[0]),
            instructions: &[true],
        })
    }))
    .expect_err("the logic panics");
    assert_eq!(panic_text(&*panicked), Some("the logic has a bug"));
    assert_eq!(released.get(), 2);
}

/// An observer sends a call down, then runs it below, over an executor
/// whose replies end at once and panic as they are dropped: each reply,
/// dropped once ended, its call having run, has its panic absorbed, and
/// the call is done.
#[test]
fn a_reply_from_below_whose_drop_panics_once_ended_leaves_the_call_done() {
    let released = Rc::new(Cell::new(0));
    let below = ReleasingBadly {
        released: Rc::clone(&released),
        pending_for: 0,
    };
    let send_then_run = |(): &mut (),
                         input: &ProgramInput<'_, bool>,
                         below: &mut Below<'_, bool>,
                         _: &mut Outcomes<'_>|
     -> Result<(), AdapterError> {
        below.send(input.instructions())?;
        below.run(input.instructions()).map(drop)
    };
    let mut observed = compose(adapter_fn(no_state, send_then_run), below).expect("bools fit");
    let reply = catch_unwind(AssertUnwindSafe(|| {
        observed.call(InstructionBatchRef {
            shots: &shots(&[0]),
            instructions: &[true],
        })
    }))
    .expect("no drop's panic reaches the call")
    .expect("the call is accepted");
    assert_eq!(released.get(), 2);
    assert_eq!(wait(reply, &mut Vec::new()), Ok(()));
}

/// An observer with calls in flight on shots 0 and 1: shot 0's fails
/// below, and once its reply is seen failed, shot 1's, still open below
/// though it has a readout, ends at its next poll with that failure and
/// none of its events, rather than waiting on.
#[test]
fn a_call_in_flight_ends_at_an_earlier_failure_below() {
    let senders = Senders::default();
    let mut observed =
        compose(adapter_fn(no_state, pass), HeldOpen(Rc::clone(&senders))).expect("bools fit");
    let mut call = |j| {
        observed
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[true],
            })
            .expect("the executor below accepts every call")
    };
    let (failing, mut waiting) = (call(0), call(1));
    let first = senders.borrow_mut().remove(0);
    first.fail("lost".into());
    senders.borrow()[0].push(one(1, 0));
    wait(failing, &mut Vec::new()).expect_err("shot 0 failed below");
    let mut events = Vec::new();
    let status = waiting.poll_events(&mut events, Waker::noop());
    assert_eq!(
        (status, events),
        (
            ReplyStatus::Failed("an earlier call failed: lost".into()),
            vec![]
        )
    );
}

/// An observer over a placed executor whose placement holds its jobs, so
/// that no call below starts, is dropped with two calls' replies pending.
/// The first call's reply, polled first, ends `Cancelled`, as the placed
/// executor's own do, and so fails the adapter: the second call's ends
/// `Failed` with that earlier failure, where the placed executor alone
/// ends both `Cancelled`
/// (`dropped_its_calls_not_yet_started_end_cancelled_in_either_poll_order`,
/// in `src/tests/executor_fn/one_per_shot/placed.rs`).
#[test]
fn dropped_over_a_placed_executor_its_second_pending_reply_fails_for_the_firsts_cancel() {
    let (placement, _jobs) = placed::holding();
    let mut observed = compose(
        adapter_fn(
            no_state,
            |(): &mut (),
             input: &ProgramInput<'_, placed::Op>,
             below: &mut Below<'_, placed::Op>,
             _: &mut Outcomes<'_>| below.send(input.instructions()),
        ),
        recording(&Threads::default())
            .run_on(placement)
            .shots(OnePerShot),
    )
    .expect("the observer passes every op");
    let mut call = |j| {
        observed
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[placed::Op::Flip],
            })
            .expect("an open executor queues a call")
    };
    let (first, second) = (call(0), call(1));
    drop(observed);
    assert_eq!(
        (polled_now(first), polled_now(second)),
        (
            (vec![], ReplyStatus::Cancelled),
            (
                vec![],
                ReplyStatus::Failed(
                    "an earlier call failed: the call was cancelled before it ran to its end"
                        .into()
                )
            )
        )
    );
}

/// The scene above, the replies polled the other way: the second call's
/// reply, polled first, ends `Cancelled` and fails the adapter, but a
/// later call's failure does not end an earlier call's reply, so the first
/// call's ends `Cancelled` too, as its own send was.
#[test]
fn dropped_over_a_placed_executor_its_pending_replies_polled_last_first_both_end_cancelled() {
    let (placement, _jobs) = placed::holding();
    let mut observed = compose(
        adapter_fn(
            no_state,
            |(): &mut (),
             input: &ProgramInput<'_, placed::Op>,
             below: &mut Below<'_, placed::Op>,
             _: &mut Outcomes<'_>| below.send(input.instructions()),
        ),
        recording(&Threads::default())
            .run_on(placement)
            .shots(OnePerShot),
    )
    .expect("the observer passes every op");
    let mut call = |j| {
        observed
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[placed::Op::Flip],
            })
            .expect("an open executor queues a call")
    };
    let (first, second) = (call(0), call(1));
    drop(observed);
    let second = polled_now(second);
    assert_eq!(
        (polled_now(first), second),
        (
            (vec![], ReplyStatus::Cancelled),
            (vec![], ReplyStatus::Cancelled)
        )
    );
}
