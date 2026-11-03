// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod conditions;
mod one_per_shot;

use std::cell::Cell;
use std::collections::BTreeMap;
use std::f64::consts::LN_2;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Waker;
use std::thread::{self, ThreadId};
use std::time::Duration;

use crate::{
    Below, BuildError, CallInput, Capabilities, Close, Describe, Executor, ExecutorFn,
    ExecutorReport, Fold, InstructionBatch, InstructionBatchRef, OnePerShot, Outcome, Outcomes,
    OwnThread, PlacementJob, Rejected, Reply, ReplyStatus, RngId, ShotContext, ShotId, ShotOutcome,
    ShotSet, TypedInstruction, Value, adapter_fn, check_unique_tags, compose, conformance,
    executor_fn, shot_seed, wait,
};

/// The run's seed in these tests.
const SEED: u64 = 7;

/// An instruction of the tests' one-bit device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

impl TypedInstruction for Op {}

/// A shot's state: one bit, zero when made.
pub(in crate::tests) struct Bit {
    one: bool,
}

impl Bit {
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the form of a state closure, passed by name"
    )]
    pub(in crate::tests) fn new(_: ShotContext) -> Result<Self, Rejected> {
        Ok(Self { one: false })
    }
}

/// The logic: flips and reads the bit; fails at `Stop`, after the
/// readouts before it.
pub(in crate::tests) fn run_bit(
    bit: &mut Bit,
    input: &CallInput<'_, Op>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    for op in input.instructions() {
        match *op {
            Op::Flip => bit.one = !bit.one,
            Op::Read(tag) => outcomes.push(Outcome::new(tag, bit.one.into())),
            Op::ReadInverted(tag) => outcomes.push(Outcome::new(tag, (!bit.one).into())),
            Op::Stop => return Err("the device stopped".to_owned()),
        }
    }
    Ok(())
}

/// The promise of the tests' device: it keeps each shot from one call to the
/// next, and a shot reads alone what it reads among others, as each of the
/// tests' states does; no other limit stated.
pub(in crate::tests) fn promise() -> Capabilities {
    Capabilities::default()
        .with_resumable_shots(true)
        .with_replayable(true)
}

/// The promise of a device that states every limit it can: four qubits,
/// eight shots at once, no resumable shots, no replay, no reset. What a wrapper
/// raised in it would show.
fn narrow_promise() -> Capabilities {
    Capabilities {
        resumable_shots: false,
        replayable: false,
        resets: false,
        ..Capabilities::default()
    }
    .with_qubits(4)
    .with_max_shots(8)
}

/// The state of a simulator whose state is a value drawn from its shot's
/// seed: that value.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the form of a state closure, passed by name"
)]
pub(in crate::tests) fn draw_value(context: ShotContext) -> Result<u64, Rejected> {
    Ok(context.seed())
}

/// The logic of [`draw_value`]'s simulator: its readout under tag `t` is
/// bit `t` of the value drawn.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the form of a logic, passed by name"
)]
pub(in crate::tests) fn read_drawn_bits(
    drawn: &mut u64,
    input: &CallInput<'_, Op>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    for op in input.instructions() {
        if let Op::Read(tag) = *op {
            let one = (*drawn >> tag) & 1 == 1;
            outcomes.push(Outcome::new(tag, one.into()));
        }
    }
    Ok(())
}

/// A shot's state that counts the flips its calls made: a device whose
/// states report something, as a simulator's report the bond dimension
/// they reached. It knows its shot, so that its report names it.
pub(in crate::tests) struct Flipped {
    shot: ShotId,
    flips: u32,
}

impl Flipped {
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the form of a state closure, passed by name"
    )]
    pub(in crate::tests) fn new(context: ShotContext) -> Result<Self, Rejected> {
        Ok(Self {
            shot: context.shot(),
            flips: 0,
        })
    }
}

/// The logic of [`Flipped`]: counts the flips; fails at `Stop`, after
/// the flips before it. It reads nothing.
pub(in crate::tests) fn count_flips(
    state: &mut Flipped,
    input: &CallInput<'_, Op>,
    _: &mut Outcomes<'_>,
) -> Result<(), String> {
    for op in input.instructions() {
        match op {
            Op::Flip => state.flips += 1,
            Op::Read(_) | Op::ReadInverted(_) => {}
            Op::Stop => return Err("the device stopped".to_owned()),
        }
    }
    Ok(())
}

/// The report of [`Flipped`] states: per shot, the flips its state made.
/// Folding adds them shot by shot, so a state reported twice, or not at
/// all, shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::tests) struct Flips(BTreeMap<ShotId, u32>);

impl Flips {
    /// The report of states that made `flips`, each with its shot.
    pub(in crate::tests) fn of<const N: usize>(flips: [(ShotId, u32); N]) -> Self {
        Self(BTreeMap::from(flips))
    }
}

impl Fold for Flips {
    fn fold(&mut self, other: Self) {
        self.0.fold(other.0);
    }
}

/// The report of `state`, a [`Flipped`]: its flips, under its shot.
pub(in crate::tests) fn flips(state: &Flipped) -> Flips {
    Flips::of([(state.shot, state.flips)])
}

/// The device's executor, from its state and its logic.
fn bit_executor()
-> impl Executor<InstructionBatch<Op>> + Describe + Close<Report = ExecutorReport<()>> {
    executor_fn(promise(), Bit::new, run_bit)
}

/// The reply of `executor` to `ops` on `shot`: its events and its end; or
/// the refusal.
fn send(
    executor: &mut impl Executor<InstructionBatch<Op>>,
    shot: ShotId,
    ops: &[Op],
) -> Result<(Vec<ShotOutcome>, Result<(), String>), Rejected> {
    let reply = executor.call(InstructionBatchRef {
        shots: &ShotSet::one(shot),
        instructions: ops,
    })?;
    let mut outcomes = Vec::new();
    let end = wait(reply, &mut outcomes);
    Ok((outcomes, end))
}

/// The outcomes of `executor`'s reply to `ops` on `shot`, which it runs.
fn run(
    executor: &mut impl Executor<InstructionBatch<Op>>,
    shot: ShotId,
    ops: &[Op],
) -> Vec<ShotOutcome> {
    let (outcomes, end) = send(executor, shot, ops).expect("the executor runs the call");
    end.expect("the logic does not fail");
    outcomes
}

/// The `(tag, value)` of each of `outcomes`.
fn values(outcomes: &[ShotOutcome]) -> Vec<(u32, Value)> {
    outcomes.iter().map(|o| (o.tag, o.value)).collect()
}

/// The note's simple form: the state and the logic as closures, whose
/// argument types the compiler infers from `executor_fn`'s bounds.
#[test]
fn closures_need_no_type_annotations() {
    let mut executor = executor_fn(
        promise(),
        |_| Ok(Bit { one: true }),
        |bit, input, outcomes| {
            for op in input.instructions() {
                if let Op::Read(tag) = *op {
                    outcomes.push(Outcome::new(tag, bit.one.into()));
                }
            }
            Ok(())
        },
    );
    let (outcomes, end) = send(&mut executor, 0, &[Op::Read(0)]).expect("the batch runs");
    end.expect("the logic does not fail");
    assert_eq!(values(&outcomes), [(0, Value::One)]);
}

/// The note's form for a logic that grows: the state's constructor and
/// the logic, plain functions, passed by name.
#[test]
fn plain_functions_are_passed_by_name() {
    let mut executor = executor_fn(promise(), Bit::new, run_bit);
    let reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Flip, Op::Read(0)],
        })
        .expect("the batch runs");
    let mut outcomes = Vec::new();
    wait(reply, &mut outcomes).expect("the logic does not fail");
    assert_eq!(outcomes, [ShotOutcome::new(0, 0, Value::One)]);
}

/// Data every call needs, here a device whose readout is inverted: held
/// once, by the logic, which runs it on the state.
struct Readout {
    inverted: bool,
}

impl Readout {
    fn run(
        &self,
        bit: &mut Bit,
        input: &CallInput<'_, Op>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), String> {
        bit.one ^= self.inverted;
        run_bit(bit, input, outcomes)
    }
}

/// The note's form for logic with shared data: the logic captures it,
/// and calls it with the state, with no type annotations.
#[test]
fn a_logic_may_capture_data_it_runs_the_state_with() {
    let readout = Readout { inverted: true };
    let mut executor = executor_fn(promise(), Bit::new, move |bit, input, outcomes| {
        readout.run(bit, input, outcomes)
    });
    let (outcomes, end) = send(&mut executor, 0, &[Op::Read(0)]).expect("the batch runs");
    end.expect("the logic does not fail");
    assert_eq!(values(&outcomes), [(0, Value::One)]);
}

/// A device flips its bit in a first call, and reads it in a second: the
/// state is made once, at the first call, and the second reads the flip.
#[test]
fn the_state_is_made_at_the_first_call_and_kept_for_the_next() {
    let made = Rc::new(Cell::new(0));
    let counted = Rc::clone(&made);
    let mut executor = executor_fn(
        promise(),
        move |shot| {
            counted.set(counted.get() + 1);
            Bit::new(shot)
        },
        run_bit,
    );
    assert_eq!(made.get(), 0, "no state before the first call");
    run(&mut executor, 3, &[Op::Flip]);
    assert_eq!(made.get(), 1, "the first call makes the state");
    let (outcomes, _) = send(&mut executor, 3, &[Op::Read(0)]).expect("the second call runs");
    assert_eq!(values(&outcomes), [(0, Value::One)]);
    assert_eq!(made.get(), 1, "the second call keeps it");
}

/// The state is made from the context of the shot of the first call.
#[test]
fn the_state_is_made_for_the_shot_of_the_first_call() {
    let shot = Rc::new(Cell::new(None));
    let seen = Rc::clone(&shot);
    let mut executor = executor_fn(
        promise(),
        move |context: ShotContext| {
            seen.set(Some(context.shot()));
            Bit::new(context)
        },
        run_bit,
    );
    run(&mut executor, 5, &[Op::Flip]);
    assert_eq!(shot.get(), Some(5));
}

/// An executor holds one state: after shot 5's first call, a call on
/// shot 6, or on shots 5 and 6, is refused naming shot 6.
#[test]
fn a_call_on_a_shot_other_than_the_one_it_holds_is_refused() {
    let mut executor = bit_executor();
    run(&mut executor, 5, &[Op::Flip]);
    assert_eq!(
        send(&mut executor, 6, &[Op::Read(0)]).err(),
        Some(Rejected::ShotNotHeld(6))
    );
    let both = ShotSet::range(5..=6).expect("5..=6 is not empty");
    let batch = InstructionBatchRef {
        shots: &both,
        instructions: &[Op::Read(0)],
    };
    assert_eq!(executor.call(batch).err(), Some(Rejected::ShotNotHeld(6)));
}

/// An executor holding shot 5's state, flipped, refuses a call on shots 5
/// and 6: shot 5 runs on from its state as it was, reading the flip once.
#[test]
fn a_call_refused_for_another_shot_leaves_the_held_state_as_it_was() {
    let mut executor = bit_executor();
    run(&mut executor, 5, &[Op::Flip]);
    let both = ShotSet::range(5..=6).expect("5..=6 is not empty");
    let batch = InstructionBatchRef {
        shots: &both,
        instructions: &[Op::Flip],
    };
    executor.call(batch).err().expect("shot 6 is not held");
    let (outcomes, _) = send(&mut executor, 5, &[Op::Read(0)]).expect("shot 5 runs on");
    assert_eq!(values(&outcomes), [(0, Value::One)]);
}

/// An executor that holds one state, first called on shots 5 and 6:
/// the call is refused naming shot 6, and makes no state.
#[test]
fn a_first_call_on_two_shots_is_refused_and_makes_no_state() {
    let made = Rc::new(Cell::new(0));
    let counted = Rc::clone(&made);
    let mut executor = executor_fn(
        promise(),
        move |shot| {
            counted.set(counted.get() + 1);
            Bit::new(shot)
        },
        run_bit,
    );
    let both = ShotSet::range(5..=6).expect("5..=6 is not empty");
    let batch = InstructionBatchRef {
        shots: &both,
        instructions: &[Op::Read(0)],
    };
    assert_eq!(executor.call(batch).err(), Some(Rejected::ShotNotHeld(6)));
    assert_eq!(made.get(), 0);
}

/// A device that holds shot 5 is called on shot 6 with a batch its check
/// refuses: the check's refusal comes first, as the facility's other
/// form gives it, whatever the shots.
#[test]
fn the_check_refuses_a_call_before_its_shots_are_checked() {
    let mut executor = executor_fn(promise(), Bit::new, run_bit)
        .check(|input| check_unique_tags(readouts(input.instructions())));
    run(&mut executor, 5, &[Op::Flip]);
    let refused = send(&mut executor, 6, &[Op::Read(1), Op::Read(1)]).err();
    assert!(
        matches!(refused, Some(Rejected::Instruction { at: 1, .. })),
        "{refused:?}"
    );
}

/// A device busy at a first call on shot 0: its state cannot be made, so
/// the call is refused with why, and nothing runs.
#[test]
fn a_state_that_cannot_be_made_refuses_the_call() {
    let ran = Rc::new(Cell::new(0));
    let counted = Rc::clone(&ran);
    let mut executor = executor_fn(
        promise(),
        |_| Err::<Bit, _>(Rejected::Unavailable("the device is busy".to_owned())),
        move |bit, input, outcomes| {
            counted.set(counted.get() + 1);
            run_bit(bit, input, outcomes)
        },
    );
    assert_eq!(
        send(&mut executor, 0, &[Op::Flip]).err(),
        Some(Rejected::Unavailable("the device is busy".to_owned()))
    );
    assert_eq!(ran.get(), 0, "the refused call runs nothing");
}

/// A device busy at a first call on shot 0, free at the next, on shot 1:
/// the state is made at that call, for shot 1, so shot 0 is not held.
#[test]
fn a_state_refused_at_a_first_call_is_made_at_the_next_for_its_shot() {
    let mut busy = true;
    let mut executor = executor_fn(
        promise(),
        move |shot| {
            if std::mem::take(&mut busy) {
                return Err(Rejected::Unavailable("the device is busy".to_owned()));
            }
            Bit::new(shot)
        },
        run_bit,
    );
    send(&mut executor, 0, &[Op::Flip]).expect_err("the device is busy at the first call");
    send(&mut executor, 1, &[Op::Flip])
        .expect("the next call makes the state")
        .1
        .expect("the logic does not fail");
    assert_eq!(
        send(&mut executor, 0, &[Op::Flip]).err(),
        Some(Rejected::ShotNotHeld(0)),
        "the state is shot 1's"
    );
}

/// The readouts of a batch, each with the index of its instruction.
pub(in crate::tests) fn readouts(ops: &[Op]) -> impl Iterator<Item = (usize, u32)> + '_ {
    ops.iter().enumerate().filter_map(|(at, op)| match *op {
        Op::Read(tag) | Op::ReadInverted(tag) => Some((at, tag)),
        _ => None,
    })
}

/// The device's check refuses a batch that reads twice under one tag:
/// refused before the state is made, and before the logic runs.
#[test]
fn the_check_refuses_a_call_before_the_state_is_made_and_the_logic_runs() {
    let (made, ran) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (counted_made, counted_ran) = (Rc::clone(&made), Rc::clone(&ran));
    let mut executor = executor_fn(
        promise(),
        move |shot| {
            counted_made.set(counted_made.get() + 1);
            Bit::new(shot)
        },
        move |bit, input, outcomes| {
            counted_ran.set(counted_ran.get() + 1);
            run_bit(bit, input, outcomes)
        },
    )
    .check(|input| check_unique_tags(readouts(input.instructions())));
    let refused = send(&mut executor, 0, &[Op::Read(1), Op::Read(1)]).err();
    assert!(
        matches!(refused, Some(Rejected::Instruction { at: 1, .. })),
        "{refused:?}"
    );
    assert_eq!((made.get(), ran.get()), (0, 0));
    run(&mut executor, 0, &[Op::Read(1)]);
    assert_eq!((made.get(), ran.get()), (1, 1));
}

/// A device reads its bit, then stops: the reply hands over the readout
/// before the stop, then fails with the logic's reason.
#[test]
fn a_failed_logic_keeps_the_outcomes_before_it() {
    let mut executor = bit_executor();
    let (outcomes, end) =
        send(&mut executor, 0, &[Op::Read(0), Op::Stop, Op::Read(1)]).expect("the call runs");
    assert_eq!(values(&outcomes), [(0, Value::Zero)]);
    assert_eq!(end, Err("the device stopped".to_owned()));
}

/// Once its device stopped, the executor refuses every call: the
/// conformance kit's check.
#[test]
fn after_a_failed_call_it_refuses_every_later_call() {
    let shots = ShotSet::one(0);
    conformance::refuses_calls_after_a_failure::<InstructionBatch<Op>, _>(
        &mut bit_executor(),
        InstructionBatchRef {
            shots: &shots,
            instructions: &[Op::Stop],
        },
        &[
            InstructionBatchRef {
                shots: &shots,
                instructions: &[Op::Flip],
            },
            InstructionBatchRef {
                shots: &shots,
                instructions: &[Op::Flip],
            },
        ],
    );
}

/// A caller sends a call that stops the device, then a flip before it
/// waits for the first reply: the logic ran inside the first call, so
/// the flip is refused, naming the stop. The conformance kit's check.
#[test]
fn a_call_in_flight_ends_at_an_earlier_failure() {
    let shots = ShotSet::one(0);
    conformance::ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<Op>, _>(
        &mut bit_executor(),
        InstructionBatchRef {
            shots: &shots,
            instructions: &[Op::Stop],
        },
        InstructionBatchRef {
            shots: &shots,
            instructions: &[Op::Flip],
        },
    );
}

/// The logic of a device whose driver has a bug: where the device stops,
/// after the instructions before it ran, the device's driver panics instead of
/// failing the call.
fn panic_at_stop(
    bit: &mut Bit,
    input: &CallInput<'_, Op>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    run_bit(bit, input, outcomes).map_err(|reason| panic!("{reason}"))
}

/// A device driver with a bug panics half-way through a call, after it flipped
/// the bit: the panic reaches the caller, and, as a `Mutex` is poisoned,
/// the executor refuses every later call, naming the panic, rather than
/// run one on a state the caller's program never made.
#[test]
fn after_a_panic_inside_call_it_refuses_every_later_call() {
    let mut executor = executor_fn(promise(), Bit::new, panic_at_stop);
    panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut executor, 0, &[Op::Flip, Op::Stop])
    }))
    .expect_err("the device driver's panic reaches the caller");
    assert_eq!(
        send(&mut executor, 0, &[Op::Read(0)]).err(),
        Some(Rejected::EarlierFailure(
            "the executor panicked during the call".to_owned()
        ))
    );
}

/// Once closed, the executor refuses every call: the conformance kit's
/// check.
#[test]
fn closed_it_refuses_every_call() {
    let shots = ShotSet::one(0);
    conformance::rejects_calls_once_closed::<InstructionBatch<Op>, _>(
        &mut bit_executor(),
        InstructionBatchRef {
            shots: &shots,
            instructions: &[Op::Flip],
        },
    );
}

/// An executor that ran a call reports once, its state's report
/// included: the conformance kit's check.
#[test]
fn a_second_close_reports_nothing() {
    let mut executor = executor_fn(promise(), Flipped::new, count_flips)
        .log_probability(|_| -1.0)
        .state_report(flips);
    run(&mut executor, 0, &[Op::Flip]);
    conformance::reports_once(&mut executor);
}

/// The device's executor, whose check refuses a batch that reads twice
/// under one tag.
fn checked_bit_executor() -> impl Executor<InstructionBatch<Op>> {
    executor_fn(promise(), Bit::new, run_bit)
        .check(|input| check_unique_tags(readouts(input.instructions())))
}

/// A batch its check refused leaves the executor usable: the conformance
/// kit's check.
#[test]
fn a_refused_call_leaves_it_usable() {
    let shots = ShotSet::one(0);
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<Op>, _>(
        &mut checked_bit_executor(),
        InstructionBatchRef {
            shots: &shots,
            instructions: &[Op::Read(0), Op::Read(0)],
        },
        InstructionBatchRef {
            shots: &shots,
            instructions: &[Op::Flip, Op::Read(0)],
        },
    );
}

/// The check refuses a tag repeated within a batch, and only that, at the
/// instruction that repeats it: the conformance kit's checks.
#[test]
fn a_tag_repeated_within_a_batch_is_refused() {
    let mut executor = checked_bit_executor();
    let shot_0 = ShotSet::one(0);
    // The device has one bit, so `a_again` reads `a`'s. The first check still
    // catches a refusal of a repeated qubit, since `b` reads that bit too and
    // `[a, b]` must be accepted; the index check alone could not.
    let (a, b, a_again) = (Op::Read(0), Op::Read(1), Op::ReadInverted(0));
    conformance::refuses_a_tag_repeated_within_a_batch(&mut executor, &shot_0, &a, &b, &a_again);
    conformance::refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut executor,
        &shot_0,
        &a,
        &Op::Flip,
        &a_again,
    );
}

/// A second call reads the flip of the first: the conformance kit's check
/// of `resumable_shots`. The checks that a shot alone gives what it gives among
/// others, and that a call changes no shot it does not name, do not apply:
/// the executor holds one shot.
#[test]
fn a_second_call_continues_its_shot() {
    conformance::continues_a_shot_from_one_call_to_the_next(
        bit_executor,
        &ShotSet::one(0),
        &[Op::Flip],
        &[Op::Read(0)],
    );
}

/// An executor built after another flipped its shot reads the shot as
/// made: the conformance kit's check that a new executor's shots start as
/// made, whatever an earlier executor left.
#[test]
fn a_new_executor_starts_its_shot_as_made() {
    conformance::starts_its_shots_as_made_whatever_an_earlier_executor_left(
        bit_executor,
        &ShotSet::one(0),
        &[Op::Flip],
        &[Op::Read(0)],
    );
}

/// A device holding a resource releases it when its executor closes: the
/// state is dropped then.
#[test]
fn closing_drops_the_state() {
    /// A state that says when it is dropped.
    struct Held(Rc<Cell<bool>>);
    impl Drop for Held {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let flag = Rc::clone(&dropped);
    let mut executor = executor_fn(
        promise(),
        move |_| Ok(Held(Rc::clone(&flag))),
        |_: &mut Held, _: &CallInput<'_, Op>, _: &mut Outcomes<'_>| Ok(()),
    );
    run(&mut executor, 0, &[Op::Flip]);
    assert!(!dropped.get(), "the state is kept until close");
    executor.close();
    assert!(dropped.get());
}

/// A call on shot 7 reads twice: each outcome is named by shot 7.
#[test]
fn each_outcome_is_named_by_the_shot_it_ran_on() {
    let mut executor = bit_executor();
    let (outcomes, _) = send(&mut executor, 7, &[Op::Read(0), Op::Read(1)]).expect("the call runs");
    assert_eq!(outcomes.iter().map(|o| o.shot).collect::<Vec<_>>(), [7, 7]);
}

/// A logic reads under tags 3, 1 and 2: the reply gives them in that
/// order, the order it reads them in, not of the tags.
#[test]
fn outcomes_keep_the_order_the_logic_pushes_them_in() {
    let mut executor = bit_executor();
    let (outcomes, _) =
        send(&mut executor, 0, &[Op::Read(3), Op::Read(1), Op::Read(2)]).expect("the call runs");
    assert_eq!(
        outcomes.iter().map(|o| o.tag).collect::<Vec<_>>(),
        [3, 1, 2]
    );
}

/// A bridge, an executor over a backend of its own, reads one readout of
/// its own, then passes up those it got from the backend: the reply holds
/// its own first, then those passed up, in the order they came, each named
/// by the call's shot.
#[test]
fn extended_outcomes_follow_those_pushed_before_in_order() {
    let mut executor = executor_fn(promise(), Bit::new, |_, input, outcomes| {
        outcomes.push(Outcome::new(9, true.into()));
        let from_below: Vec<_> = (input.instructions().iter())
            .filter_map(|op| match *op {
                Op::Read(tag) => Some(Outcome::new(tag, false.into())),
                _ => None,
            })
            .collect();
        outcomes.extend(from_below);
        Ok(())
    });
    let (outcomes, end) =
        send(&mut executor, 4, &[Op::Read(3), Op::Read(1)]).expect("the call runs");
    end.expect("the logic does not fail");
    assert_eq!(
        outcomes
            .iter()
            .map(|o| (o.shot, o.tag, o.value))
            .collect::<Vec<_>>(),
        [(4, 9, Value::One), (4, 3, Value::Zero), (4, 1, Value::Zero)]
    );
}

/// A device that states no limit, or every limit it can, holds one shot
/// without a shot strategy: it describes one, and the rest of its promise
/// as given; a device that promises no shot is not raised to one.
#[test]
fn without_a_shot_strategy_the_promise_is_lowered_to_one_shot() {
    let none = promise().with_max_shots(0);
    for (given, described) in [
        (promise(), promise().with_max_shots(1)),
        (narrow_promise(), narrow_promise().with_max_shots(1)),
        (none.clone(), none),
    ] {
        let executor = executor_fn(given, Bit::new, run_bit);
        assert_eq!(executor.capabilities(), described);
    }
}

/// A device that promises no shot (none free on it) is called on shot 0:
/// the call is refused for want of room, `ShotNotHeld(0)`, as under
/// `OnePerShot`, and no state is made, so the promise holds.
#[test]
fn a_promise_of_no_shots_refuses_the_first_call() {
    let made = Rc::new(Cell::new(0));
    let counted = Rc::clone(&made);
    let mut executor = executor_fn(
        promise().with_max_shots(0),
        move |shot| {
            counted.set(counted.get() + 1);
            Bit::new(shot)
        },
        run_bit,
    );
    assert_eq!(
        send(&mut executor, 0, &[Op::Flip]).err(),
        Some(Rejected::ShotNotHeld(0))
    );
    assert_eq!(made.get(), 0, "no state is made");
}

/// A device that promises no shot is first called on shots 5 and 6: the
/// call is refused for want of room naming its first shot,
/// `ShotNotHeld(5)`, as under `OnePerShot`, not for a second shot.
#[test]
fn a_promise_of_no_shots_refuses_a_first_call_on_several_shots_naming_its_first() {
    let mut executor = executor_fn(promise().with_max_shots(0), Bit::new, run_bit);
    let refused = executor.call(InstructionBatchRef {
        shots: &ShotSet::new(vec![5, 6]).expect("two shots"),
        instructions: &[Op::Flip],
    });
    assert_eq!(refused.err(), Some(Rejected::ShotNotHeld(5)));
}

/// A device promises each shot's log probability, or does not say: the
/// executor promises it exactly when it is told how to read it from the
/// state, since only then can it report it.
#[test]
fn log_probability_is_promised_only_when_a_function_gives_it() {
    for given in [true, false] {
        let promise = promise().with_log_probability(given);
        let without = executor_fn(promise.clone(), Bit::new, run_bit);
        assert!(!without.capabilities().log_probability, "given {given}");
        let with = executor_fn(promise, Bit::new, run_bit).log_probability(|_| 0.0);
        assert!(with.capabilities().log_probability, "given {given}");
    }
}

/// A device whose bit was flipped by a fair coin drew with probability ½:
/// its executor reports ln ½ for the shot of its state when it closes.
#[test]
fn closing_reports_the_log_probability_of_the_state_it_made() {
    let mut executor = executor_fn(promise(), Bit::new, run_bit)
        .log_probability(|bit| if bit.one { -LN_2 } else { 0.0 });
    run(&mut executor, 4, &[Op::Flip]);
    let report = executor.close();
    assert_eq!(report.log_p.get(4), Some(-LN_2));
}

/// An executor closed before any call made no state: its report gives no
/// log probability, for no shot.
#[test]
fn an_executor_that_made_no_state_reports_no_log_probability() {
    let mut executor = executor_fn(promise(), Bit::new, run_bit).log_probability(|_| 0.0);
    assert!(executor.close().log_p.is_empty());
}

/// A device that cannot tell how likely its state is, given no reader of
/// the log probability, runs shot 0, whatever its form: its report gives
/// no log probability for that shot (absence, not 0, which would claim a
/// certain draw).
#[test]
fn an_executor_not_given_how_to_read_it_reports_no_log_probability_for_a_shot_that_ran() {
    let mut plain = executor_fn(promise(), Bit::new, run_bit);
    run(&mut plain, 0, &[Op::Flip]);
    assert_eq!(plain.close().log_p.get(0), None, "an executor_fn");
    let mut per_shot = executor_fn(promise(), Bit::new, run_bit).shots(OnePerShot);
    run(&mut per_shot, 0, &[Op::Flip]);
    assert_eq!(per_shot.close().log_p.get(0), None, "one state per shot");
    let mut placed = executor_fn(promise(), Bit::new, run_bit)
        .run_on(OwnThread)
        .shots(OnePerShot);
    run(&mut placed, 0, &[Op::Flip]);
    assert_eq!(placed.close().log_p.get(0), None, "each shot placed");
}

/// A simulator given the run's seed holds one state, for shot 4, which
/// draws from the shot's seed: one state can draw, as a state per shot
/// does.
#[test]
fn a_state_given_the_runs_seed_draws_from_its_shots_seed() {
    let ops: Vec<_> = (0..16).map(Op::Read).collect();
    let mut executor = executor_fn(promise(), draw_value, read_drawn_bits).seed(SEED);
    let outcomes = run(&mut executor, 4, &ops);
    let drawn = shot_seed(SEED, 4);
    let expected: Vec<_> = (0..16)
        .map(|tag| (tag, Value::from((drawn >> tag) & 1 == 1)))
        .collect();
    assert_eq!(values(&outcomes), expected);
}

/// A simulator built with one seed, then given the run's seed, draws on
/// shot 4 what it draws given the run's seed alone: a later seed replaces
/// the one given before.
#[test]
fn a_later_seed_replaces_the_one_given_before() {
    let ops: Vec<_> = (0..16).map(Op::Read).collect();
    let drawing = || executor_fn(promise(), draw_value, read_drawn_bits);
    let earlier = run(&mut drawing().seed(SEED ^ 1), 4, &ops);
    let given = run(&mut drawing().seed(SEED), 4, &ops);
    assert_ne!(
        earlier, given,
        "the two seeds draw different bits on shot 4"
    );
    let reseeded = run(&mut drawing().seed(SEED ^ 1).seed(SEED), 4, &ops);
    assert_eq!(reseeded, given);
}

/// A simulator whose states draw a value from their shot's seed reads the
/// same bits on shot 4 whatever its shot form (one state, a state per
/// shot, placed either way), since the seed is given apart from the form,
/// and a shot's seed depends only on the run's seed and the shot.
#[test]
fn a_shot_draws_the_same_values_in_every_shot_form() {
    let ops: Vec<_> = (0..16).map(Op::Read).collect();
    let drawing = || executor_fn(promise(), draw_value, read_drawn_bits).seed(SEED);
    let one_state = run(&mut drawing(), 4, &ops);
    let forms = [
        run(&mut drawing().shots(OnePerShot), 4, &ops),
        run(&mut drawing().run_on(OwnThread), 4, &ops),
        run(&mut drawing().shots(OnePerShot).run_on(OwnThread), 4, &ops),
        run(&mut drawing().run_on(OwnThread).shots(OnePerShot), 4, &ops),
    ];
    for (form, outcomes) in forms.iter().enumerate() {
        assert_eq!(outcomes, &one_state, "form {form}");
    }
}

/// A simulator given the run's seed first, then its check, how to read its
/// log probability and its states' report, draws on shot 4 what it draws
/// given the seed last: the methods that shape the logic keep the seed.
#[test]
fn the_seed_is_kept_by_the_methods_that_shape_the_logic() {
    let ops: Vec<_> = (0..16).map(Op::Read).collect();
    let seed_first = executor_fn(promise(), draw_value, read_drawn_bits)
        .seed(SEED)
        .check(|_: &CallInput<'_, Op>| Ok(()))
        .log_probability(|_: &u64| 0.0)
        .state_report(|_: &u64| ());
    let seed_last = executor_fn(promise(), draw_value, read_drawn_bits)
        .check(|_: &CallInput<'_, Op>| Ok(()))
        .log_probability(|_: &u64| 0.0)
        .state_report(|_: &u64| ())
        .seed(SEED);
    assert_eq!(
        run(&mut seed_first.shots(OnePerShot), 4, &ops),
        run(&mut seed_last.shots(OnePerShot), 4, &ops)
    );
}

/// A simulator whose states count their flips, given a check that refuses
/// a readout's tag repeated, how to read its log probability (minus its
/// flips) and its states' report, in the order they are declared and in the
/// reverse one: each refuses the same batch, offers a log probability, and
/// reports the same flips and log probability for the same calls. Each
/// option keeps the others given before it.
#[test]
fn its_options_given_in_any_order_make_the_same_executor() {
    fn checked_then_closed<E>(mut executor: E) -> (Flips, Option<f64>)
    where
        E: Executor<InstructionBatch<Op>> + Describe + Close<Report = ExecutorReport<Flips>>,
    {
        assert!(
            executor.capabilities().log_probability,
            "it is given how to read a state's log probability"
        );
        send(&mut executor, 0, &[Op::Read(1), Op::Read(1)])
            .expect_err("the check refuses a readout's tag repeated");
        run(&mut executor, 0, &[Op::Flip, Op::Flip, Op::Flip]);
        let report = executor.close();
        (report.states, report.log_p.get(0))
    }
    let check = |input: &CallInput<'_, Op>| check_unique_tags(readouts(input.instructions()));
    let log_p = |state: &Flipped| -f64::from(state.flips);
    let declared = executor_fn(promise(), Flipped::new, count_flips)
        .check(check)
        .log_probability(log_p)
        .state_report(flips);
    let reversed = executor_fn(promise(), Flipped::new, count_flips)
        .state_report(flips)
        .log_probability(log_p)
        .check(check);
    let expected = (Flips::of([(0, 3)]), Some(-3.0));
    assert_eq!(checked_then_closed(declared), expected, "declared order");
    assert_eq!(checked_then_closed(reversed), expected, "reverse order");
}

/// A device's state that draws, in an executor not given the run's seed,
/// has none to draw from, whatever its shot form: drawing panics, naming
/// the method that gives it, rather than draw from a seed it was not
/// given. Where the state is made on a placement's thread, the caller sees
/// the panic as its reply failing.
#[test]
fn without_the_runs_seed_a_state_has_none_to_draw_from() {
    let ops = [Op::Read(0)];
    let drawing = || executor_fn(promise(), draw_value, read_drawn_bits);
    let panics: [Box<dyn FnOnce()>; 3] = [
        Box::new(|| drop(run(&mut drawing(), 0, &ops))),
        Box::new(|| drop(run(&mut drawing().shots(OnePerShot), 0, &ops))),
        Box::new(|| {
            drop(run(
                &mut drawing().run_on(OwnThread).shots(OnePerShot),
                0,
                &ops,
            ));
        }),
    ];
    for (form, draw) in panics.into_iter().enumerate() {
        let panic = panic::catch_unwind(AssertUnwindSafe(draw))
            .expect_err("a state drawing with no run's seed panics");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .expect("the panic has a message");
        assert!(message.contains(".seed(seed)"), "form {form}: {message}");
    }
    let failed = (
        Vec::new(),
        Err("the executor panicked during the call".to_owned()),
    );
    let one_state = send(&mut drawing().run_on(OwnThread), 0, &ops);
    assert_eq!(one_state, Ok(failed.clone()), "one state, placed");
    let one_per_shot = send(&mut drawing().shots(OnePerShot).run_on(OwnThread), 0, &ops);
    assert_eq!(one_per_shot, Ok(failed), "one state per shot, placed");
}

/// A device's promise copied from the capabilities of one given the run's
/// seed, then given to an executor made without `.seed`: that executor
/// gives no run's seed, so a noise model that draws, composed over it, is
/// refused when the chain is built, rather than draw from the seed the
/// copy carried.
#[test]
fn an_executor_given_no_seed_gives_none_whatever_its_promise_carries() {
    const NOISE: RngId = RngId::named("noise");
    let seeded = executor_fn(promise(), Bit::new, run_bit)
        .seed(7)
        .capabilities();
    let unseeded = executor_fn(seeded, Bit::new, run_bit);
    let noise = adapter_fn(
        |context: ShotContext| Ok(context.seed()),
        |_: &mut u64,
         input: &CallInput<'_, Op>,
         below: &mut Below<'_, Op>,
         _: &mut Outcomes<'_>| { below.send(input.instructions()) },
    )
    .drawing(NOISE);
    let Err(faults) = compose(noise, unseeded) else {
        panic!("no run's seed reaches the noise model")
    };
    let errors: Vec<_> = faults.0.into_iter().map(|fault| fault.error).collect();
    assert_eq!(errors, [BuildError::NoRunSeed(NOISE)]);
}

/// A device busy at its first call refuses it, runs a call, refuses one
/// on another shot and one its check refuses, then stops in a fifth: its
/// report counts the two it accepted.
#[test]
fn the_report_counts_the_calls_it_accepted() {
    let mut busy = true;
    let mut executor = executor_fn(
        promise(),
        move |shot| {
            if std::mem::take(&mut busy) {
                return Err(Rejected::Unavailable("the device is busy".to_owned()));
            }
            Bit::new(shot)
        },
        run_bit,
    )
    .check(|input| check_unique_tags(readouts(input.instructions())));
    send(&mut executor, 0, &[Op::Flip]).expect_err("the device is busy");
    run(&mut executor, 0, &[Op::Flip]);
    send(&mut executor, 1, &[Op::Flip]).expect_err("shot 1 is not held");
    send(&mut executor, 0, &[Op::Read(0), Op::Read(0)]).expect_err("the check refuses it");
    let (_, end) = send(&mut executor, 0, &[Op::Stop]).expect("the call runs");
    end.expect_err("the device stops");
    assert_eq!(executor.close().calls, 2);
}

/// A device whose states count their flips, flipped three times over two
/// calls on shot 4: its executor's report is its state's, as the calls
/// left it.
#[test]
fn closing_reports_the_state_it_made() {
    let mut executor = executor_fn(promise(), Flipped::new, count_flips).state_report(flips);
    run(&mut executor, 4, &[Op::Flip, Op::Flip]);
    run(&mut executor, 4, &[Op::Flip]);
    assert_eq!(executor.close().states, Flips::of([(4, 3)]));
}

/// An executor closed before any call made no state: its report is the
/// empty one.
#[test]
fn an_executor_that_made_no_state_reports_no_state() {
    let mut executor = executor_fn(promise(), Flipped::new, count_flips).state_report(flips);
    assert_eq!(executor.close().states, Flips::default());
}

/// A device that flips once, then flips and stops: the failed executor
/// keeps its state until it closes, which reports it as the failed call
/// left it.
#[test]
fn a_failed_executor_still_reports_its_state() {
    let mut executor = executor_fn(promise(), Flipped::new, count_flips).state_report(flips);
    run(&mut executor, 0, &[Op::Flip]);
    let (_, end) = send(&mut executor, 0, &[Op::Flip, Op::Stop]).expect("the call runs");
    end.expect_err("the device stops");
    assert_eq!(executor.close().states, Flips::of([(0, 2)]));
}

/// A failed call reports no state made for it: a device that flips, then
/// stops, on its first call, on shot 0, makes the state the call fails
/// with, so its report has none, and no log probability. Chosen so that
/// every order of the wrappers gives the same report (a state per shot, or
/// placed); `futures`' `try_join_all` and rayon's `collect::<Result<_, _>>()`
/// drop the others' results too.
#[test]
fn a_failed_call_reports_no_state_made_for_it() {
    let mut executor = executor_fn(promise(), Flipped::new, count_flips)
        .log_probability(|state: &Flipped| f64::from(state.flips))
        .state_report(flips);
    let (_, end) = send(&mut executor, 0, &[Op::Flip, Op::Stop]).expect("the call runs");
    end.expect_err("the device stops");
    let report = executor.close();
    assert_eq!(
        (report.states, report.log_p.get(0)),
        (Flips::default(), None)
    );
}

/// The payload of the panic of a [`PowersDownBadly`]'s drop: its shot.
#[derive(Debug, PartialEq)]
pub(in crate::tests) struct PowerDownFailed(pub(in crate::tests) ShotId);

/// The shots whose states were freed, in the order they were.
pub(in crate::tests) type Freed = Arc<Mutex<Vec<ShotId>>>;

/// A device's state that fails to power down, a bug of every state of its
/// type: its drop records its shot in `freed`, then panics with
/// [`PowerDownFailed`].
pub(in crate::tests) struct PowersDownBadly {
    shot: ShotId,
    freed: Freed,
}

impl Drop for PowersDownBadly {
    fn drop(&mut self) {
        (self.freed.lock().expect("nothing panics holding the lock")).push(self.shot);
        panic::panic_any(PowerDownFailed(self.shot));
    }
}

/// The state closure of a device whose states power down badly, which
/// refuses `refused`, if any, as a device with no room left for it.
pub(in crate::tests) fn powering_down_badly(
    freed: &Freed,
    refused: Option<ShotId>,
) -> impl FnMut(ShotContext) -> Result<PowersDownBadly, Rejected> + Clone + Send + 'static {
    let freed = Arc::clone(freed);
    move |context| {
        if Some(context.shot()) == refused {
            return Err(Rejected::Unavailable("no room left".to_owned()));
        }
        Ok(PowersDownBadly {
            shot: context.shot(),
            freed: Arc::clone(&freed),
        })
    }
}

/// The shots freed so far, in the order they were.
pub(in crate::tests) fn freed_so_far(freed: &Freed) -> Vec<ShotId> {
    freed
        .lock()
        .expect("nothing panics holding the lock")
        .clone()
}

/// A device's connection, reached through a context per thread, as a GPU
/// is: each closure of an executor holds a handle on it
/// ([`ConnectionHandle`]), whose drop closes the context of the thread it
/// is dropped on; and each shot's session ([`Session`]) releases its
/// resources through the context of the thread it is dropped on. So a
/// session dropped after any closure's handle, on the same thread, leaks.
#[derive(Clone, Default)]
pub(in crate::tests) struct Connection {
    /// The threads whose context a handle's drop closed.
    closed_on: Arc<Mutex<Vec<ThreadId>>>,
    /// What the device logs as its shots' sessions end: `released shot j`,
    /// or `shot j leaked` when the context was closed, in the order they
    /// ended.
    log: Arc<Mutex<Vec<String>>>,
}

impl Connection {
    /// A handle on this connection, for a closure to hold.
    pub(in crate::tests) fn handle(&self) -> ConnectionHandle {
        ConnectionHandle(self.clone())
    }

    /// What the device logged so far.
    pub(in crate::tests) fn logged(&self) -> Vec<String> {
        (self.log.lock().expect("nothing panics holding the lock")).clone()
    }

    /// Whether the context of the thread this runs on was closed.
    fn closed_here(&self) -> bool {
        let closed_on = self.closed_on.lock();
        (closed_on.expect("nothing panics holding the lock")).contains(&thread::current().id())
    }
}

/// A handle on a device's connection, which a closure holds: dropped, it
/// closes the context of the thread it is dropped on. A copy is another
/// handle on the same connection.
#[derive(Clone)]
pub(in crate::tests) struct ConnectionHandle(Connection);

impl ConnectionHandle {
    /// The session of `shot`, which releases its resources through this
    /// handle's connection.
    pub(in crate::tests) fn session(&self, shot: ShotId) -> Session {
        Session {
            shot,
            connection: self.0.clone(),
        }
    }
}

impl Drop for ConnectionHandle {
    fn drop(&mut self) {
        let closed_on = self.0.closed_on.lock();
        (closed_on.expect("nothing panics holding the lock")).push(thread::current().id());
    }
}

/// A shot's session on a device, a state whose drop releases its shot's
/// resources through the context of the thread it is dropped on, or leaks
/// them once a handle's drop closed that context.
pub(in crate::tests) struct Session {
    shot: ShotId,
    connection: Connection,
}

impl Drop for Session {
    fn drop(&mut self) {
        let line = if self.connection.closed_here() {
            format!("shot {} leaked", self.shot)
        } else {
            format!("released shot {}", self.shot)
        };
        let log = self.connection.log.lock();
        (log.expect("nothing panics holding the lock")).push(line);
    }
}

/// A device whose every closure (its state closure, logic, check, log
/// probability and report maker) holds a handle on `connection`, through
/// which its shots' sessions release their resources: an executor that
/// drops any closure before a session leaks it.
#[expect(
    clippy::type_complexity,
    reason = "each closure is named by its signature, so that the tests can \
              make the device's shot forms from it"
)]
pub(in crate::tests) fn connected_device<I>(
    connection: &Connection,
) -> ExecutorFn<
    I,
    Session,
    impl FnMut(ShotContext) -> Result<Session, Rejected> + Send + 'static,
    impl FnMut(&mut Session, &CallInput<'_, I>, &mut Outcomes<'_>) -> Result<(), String>
    + Clone
    + Send
    + 'static,
    impl FnMut(&CallInput<'_, I>) -> Result<(), Rejected> + Send + 'static,
    impl FnMut(&Session) -> f64 + Clone + Send + 'static,
    impl FnMut(&Session) + Clone + Send + 'static,
> {
    let making = connection.handle();
    let (running, checking) = (connection.handle(), connection.handle());
    let (reading, reporting) = (connection.handle(), connection.handle());
    executor_fn(
        Capabilities::default(),
        move |context: ShotContext| Ok(making.session(context.shot())),
        move |_: &mut Session, _: &CallInput<'_, I>, _: &mut Outcomes<'_>| {
            let _held = &running;
            Ok(())
        },
    )
    .check(move |_: &CallInput<'_, I>| {
        let _held = &checking;
        Ok(())
    })
    .log_probability(move |_: &Session| {
        let _held = &reading;
        0.0
    })
    .state_report(move |_: &Session| {
        let _held = &reporting;
    })
}

/// The payload of the panic of a [`ReleasesBadly`]'s drop.
#[derive(Debug, PartialEq)]
pub(in crate::tests) struct ReleaseFailed;

/// What a device driver's closure captures that fails to release as it is
/// dropped, such as a handle on a device's connection, a bug of the
/// device driver's: its drop counts itself in the count it was made with, then
/// panics with [`ReleaseFailed`].
pub(in crate::tests) struct ReleasesBadly(Arc<AtomicUsize>);

impl ReleasesBadly {
    /// A handle, and the count of the handles of its kind released so far.
    pub(in crate::tests) fn new() -> (Self, Arc<AtomicUsize>) {
        let released = Arc::new(AtomicUsize::new(0));
        (Self(Arc::clone(&released)), released)
    }

    /// Another handle, counted where this one is.
    pub(in crate::tests) fn another(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl Drop for ReleasesBadly {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
        panic::panic_any(ReleaseFailed);
    }
}

/// A logic, of any device, that runs and reads nothing, holding
/// `handle` (a [`ReleasesBadly`], shared by its copies) for as long as it
/// is held.
pub(in crate::tests) fn holding_a_handle<S, I>(
    handle: Arc<ReleasesBadly>,
) -> impl FnMut(&mut S, &CallInput<'_, I>, &mut Outcomes<'_>) -> Result<(), String>
+ Clone
+ Send
+ 'static {
    move |_, _, _| {
        let _held = &handle;
        Ok(())
    }
}

/// A logic, of any device, that runs, and reads nothing.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the form of a logic, passed by name"
)]
pub(in crate::tests) fn runs<S, I>(
    _: &mut S,
    _: &CallInput<'_, I>,
    _: &mut Outcomes<'_>,
) -> Result<(), String> {
    Ok(())
}

/// A logic, of any device, that stops: the call fails.
pub(in crate::tests) fn stops<S, I>(
    _: &mut S,
    _: &CallInput<'_, I>,
    _: &mut Outcomes<'_>,
) -> Result<(), String> {
    Err("the device stopped".to_owned())
}

/// A logic, of any device, whose driver has a bug: it panics.
pub(in crate::tests) fn panics<S, I>(
    _: &mut S,
    _: &CallInput<'_, I>,
    _: &mut Outcomes<'_>,
) -> Result<(), String> {
    panic!("the device's driver panics")
}

/// The text of a panic raised with a literal message.
pub(in crate::tests) fn panic_text(payload: &(dyn std::any::Any + Send)) -> Option<&'static str> {
    payload.downcast_ref::<&str>().copied()
}

/// A device whose state fails to power down (a device driver's bug) stops in
/// its first call, on shot 0: the call frees the state made for it, whose
/// drop's panic reaches the caller.
#[test]
fn a_failed_call_frees_the_state_made_for_it_and_raises_its_drops_panic() {
    let freed = Freed::default();
    let mut executor = executor_fn(promise(), powering_down_badly(&freed, None), stops);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| send(&mut executor, 0, &[Op::Flip])))
        .expect_err("the state's drop panics");
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0]);
}

/// A device whose state fails to power down stops in its first call, on
/// shot 0, and the state's drop panics as the call frees it: the call
/// failed first, so later calls are refused with the logic's reason,
/// not the panic's.
#[test]
fn after_a_failed_calls_state_drop_panics_later_calls_get_the_logics_reason() {
    let freed = Freed::default();
    let mut executor = executor_fn(promise(), powering_down_badly(&freed, None), stops);
    panic::catch_unwind(AssertUnwindSafe(|| send(&mut executor, 0, &[Op::Flip])))
        .expect_err("the state's drop panics");
    assert_eq!(
        send(&mut executor, 0, &[Op::Flip]).err(),
        Some(Rejected::EarlierFailure("the device stopped".to_owned()))
    );
}

/// A caller panics while it holds the executor of a device whose state
/// fails to power down, after a call on shot 0: the executor, dropped as
/// the caller's panic unwinds, frees the state, and leaves that drop's
/// panic to the panic hook, so that the caller's panic reaches its caller
/// rather than abort the process.
#[test]
fn dropped_as_a_panic_unwinds_it_frees_its_state_raising_none() {
    let freed = Freed::default();
    let mut executor = executor_fn(promise(), powering_down_badly(&freed, None), runs);
    let (_, end) = send(&mut executor, 0, &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let panicked = panic::catch_unwind(AssertUnwindSafe(move || {
        let _held = executor;
        panic!("the caller panics");
    }))
    .expect_err("the caller panics");
    assert_eq!(panic_text(&*panicked), Some("the caller panics"));
    assert_eq!(freed_so_far(&freed), [0]);
}

/// The same, with the executor placed on a thread of its own after its
/// call (`.run_on(..)`), and dropped before its first call there: it is
/// dropped where it is, as the caller's panic unwinds, and frees its state
/// the same way.
#[test]
fn placed_and_dropped_as_a_panic_unwinds_it_frees_its_state_raising_none() {
    let freed = Freed::default();
    let mut executor = executor_fn(promise(), powering_down_badly(&freed, None), runs);
    let (_, end) = send(&mut executor, 0, &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let placed = executor.run_on(OwnThread);
    let panicked = panic::catch_unwind(AssertUnwindSafe(move || {
        let _held = placed;
        panic!("the caller panics");
    }))
    .expect_err("the caller panics");
    assert_eq!(panic_text(&*panicked), Some("the caller panics"));
    assert_eq!(freed_so_far(&freed), [0]);
}

/// A device driver with two bugs: its logic panics in the first call on shot
/// 0, and its state, made for that call, fails to power down as the
/// logic's panic unwinds. The logic's panic reaches the caller, and
/// the state's is left to the panic hook, rather than abort the process.
#[test]
fn a_logics_panic_reaches_the_caller_though_its_new_states_drop_panics_too() {
    let freed = Freed::default();
    let mut executor = executor_fn(promise(), powering_down_badly(&freed, None), panics);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| send(&mut executor, 0, &[Op::Flip])))
        .expect_err("the logic panics");
    assert_eq!(panic_text(&*panicked), Some("the device's driver panics"));
    assert_eq!(freed_so_far(&freed), [0]);
}

/// A device driver with two bugs: its log probability panics as `close` reads
/// shot 0's state, which fails to power down as that panic unwinds. The
/// log probability's panic reaches the closer, and the state's is left to
/// the panic hook, rather than abort the process.
#[test]
fn a_report_closures_panic_reaches_the_closer_though_the_states_drop_panics_too() {
    let freed = Freed::default();
    let mut executor = executor_fn(promise(), powering_down_badly(&freed, None), runs)
        .log_probability(|_: &PowersDownBadly| panic!("the log probability panics"));
    let (_, end) = send(&mut executor, 0, &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| executor.close()))
        .expect_err("the log probability panics");
    assert_eq!(panic_text(&*panicked), Some("the log probability panics"));
    assert_eq!(freed_so_far(&freed), [0]);
}

/// A device whose state fails to power down ran a call on shot 0, and is
/// dropped unclosed: its drop frees the state and raises no panic of its
/// drop, which only a close reports.
#[test]
fn dropped_it_frees_its_state_raising_no_panic_of_its_drop() {
    let freed = Freed::default();
    let mut executor = executor_fn(promise(), powering_down_badly(&freed, None), runs);
    let (_, end) = send(&mut executor, 0, &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    panic::catch_unwind(AssertUnwindSafe(|| drop(executor)))
        .expect("a drop raises no panic of the state's drop");
    assert_eq!(freed_so_far(&freed), [0]);
}

/// The same device, closed: the close frees the state, and the panic of
/// its drop reaches the closer.
#[test]
fn close_frees_its_state_then_raises_its_drops_panic() {
    let freed = Freed::default();
    let mut executor = executor_fn(promise(), powering_down_badly(&freed, None), runs);
    let (_, end) = send(&mut executor, 0, &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| executor.close()))
        .expect_err("the state's drop panics");
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0]);
}

/// When the `Default` of a [`DefaultPanicsOnce`] panics, by its test: at
/// its `n`-th call once armed with `n`, just before the close it is to
/// fail; 0 when spent by that panic, or not armed.
static DEFAULT_ARMED: [AtomicUsize; 4] = [
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
];

/// The thread on which a [`DefaultPanicsOnce`]'s `Default` panicked, by
/// its test.
static DEFAULT_PANICKED_ON: [Mutex<Option<ThreadId>>; 4] = [const { Mutex::new(None) }; 4];

/// A state's report whose `Default` has a bug, a bug of the device driver's: it
/// panics once, at the call [`DEFAULT_ARMED`] is armed with for test
/// `TEST`, on a thread it records in [`DEFAULT_PANICKED_ON`].
#[derive(Debug, PartialEq)]
struct DefaultPanicsOnce<const TEST: usize>(u32);

impl<const TEST: usize> Default for DefaultPanicsOnce<TEST> {
    fn default() -> Self {
        let calls_left = DEFAULT_ARMED[TEST]
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .unwrap_or(0);
        if calls_left == 1 {
            *DEFAULT_PANICKED_ON[TEST]
                .lock()
                .expect("no test panics holding it") = Some(thread::current().id());
        }
        assert!(calls_left != 1, "the report's default has a bug");
        Self(0)
    }
}

impl<const TEST: usize> Fold for DefaultPanicsOnce<TEST> {
    fn fold(&mut self, other: Self) {
        self.0 += other.0;
    }
}

/// A device's state that holds the device, which it releases as it is
/// dropped, after a while (it powers down), counted in `released`: a
/// close that did not wait for it would not see it released.
struct HoldsDevice {
    released: Arc<AtomicUsize>,
}

impl Drop for HoldsDevice {
    fn drop(&mut self) {
        thread::sleep(Duration::from_millis(10));
        self.released.fetch_add(1, Ordering::SeqCst);
    }
}

/// What a form of an executor gave, closed as its report's `Default`
/// panics, after a call on each of its shots: the states it held, and
/// those released when the panic reached the closer.
struct ClosedByADefaultsPanic<const TEST: usize> {
    form: &'static str,
    held: usize,
    released: usize,
    /// The report of a second close.
    second: ExecutorReport<DefaultPanicsOnce<TEST>>,
}

/// Where a form's report's `Default` panics.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PanicsOn {
    TheClosersThread,
    APlacement,
}

/// Each form of a device of [`HoldsDevice`] states, whose report's
/// `Default` has a bug, after a call on each of its shots (one; or
/// three, a state per shot), closed as that `Default` panics, then closed
/// again. It panics at its first call, or, shots placed each, at its
/// second too, which is made on a shot's placement (the first makes the
/// report of the shots' reports, on the closer's thread): each form
/// checks the thread it panicked on.
fn closed_by_a_defaults_panic<const TEST: usize>() -> Vec<ClosedByADefaultsPanic<TEST>> {
    fn closed_twice<const TEST: usize, E>(
        form: &'static str,
        shots: &[ShotId],
        (default_panics_at, panics_on): (usize, PanicsOn),
        place: impl FnOnce(&Arc<AtomicUsize>) -> E,
    ) -> ClosedByADefaultsPanic<TEST>
    where
        E: Executor<InstructionBatch<Op>> + Close<Report = ExecutorReport<DefaultPanicsOnce<TEST>>>,
    {
        let released = Arc::new(AtomicUsize::new(0));
        let mut executor = place(&released);
        for &shot in shots {
            run(&mut executor, shot, &[Op::Flip]);
        }
        DEFAULT_ARMED[TEST].store(default_panics_at, Ordering::SeqCst);
        let panicked = panic::catch_unwind(AssertUnwindSafe(|| executor.close()))
            .expect_err("the report's default panics");
        assert_eq!(
            panic_text(&*panicked),
            Some("the report's default has a bug"),
            "{form}: the panic reaches the closer"
        );
        let panicked_on = DEFAULT_PANICKED_ON[TEST]
            .lock()
            .expect("no test panics holding it")
            .take()
            .expect("the report's default panicked");
        let panicked_on = if panicked_on == thread::current().id() {
            PanicsOn::TheClosersThread
        } else {
            PanicsOn::APlacement
        };
        assert_eq!(panicked_on, panics_on, "{form}: where the default panicked");
        let released_at_the_panic = released.load(Ordering::SeqCst);
        ClosedByADefaultsPanic {
            form,
            held: shots.len(),
            released: released_at_the_panic,
            second: executor.close(),
        }
    }
    let device = |released: &Arc<AtomicUsize>| {
        let released = Arc::clone(released);
        executor_fn(
            promise(),
            move |_| {
                Ok(HoldsDevice {
                    released: Arc::clone(&released),
                })
            },
            runs,
        )
        .state_report(|_: &HoldsDevice| DefaultPanicsOnce::<TEST>(1))
    };
    let per_shot = [0, 1, 2];
    let closer = (1, PanicsOn::TheClosersThread);
    let placement = (1, PanicsOn::APlacement);
    vec![
        closed_twice("one state", &[0], closer, device),
        closed_twice("a state per shot", &per_shot, closer, |released| {
            device(released).shots(OnePerShot)
        }),
        closed_twice("one state, placed", &[0], placement, |released| {
            device(released).run_on(OwnThread)
        }),
        closed_twice(
            "a state per shot, placed whole",
            &per_shot,
            placement,
            |released| device(released).shots(OnePerShot).run_on(OwnThread),
        ),
        closed_twice(
            "a state per shot, each placed",
            &per_shot,
            closer,
            |released| device(released).run_on(OwnThread).shots(OnePerShot),
        ),
        closed_twice(
            "a state per shot, each placed, panicking on a shot's placement",
            &per_shot,
            (2, PanicsOn::APlacement),
            |released| device(released).run_on(OwnThread).shots(OnePerShot),
        ),
    ]
}

/// A device whose states each hold the device, and whose report's
/// `Default` has a bug, a device driver's: closed, each form has released every
/// state when that panic reaches the closer, so that the device is free
/// at once.
#[test]
fn a_close_whose_reports_default_panics_frees_every_state_first() {
    for closed in closed_by_a_defaults_panic::<0>() {
        assert_eq!(closed.released, closed.held, "{}", closed.form);
    }
}

/// The same device: closed again after that panic, each form reports
/// nothing, its states and calls taken by the close that panicked.
#[test]
fn a_close_after_its_reports_default_panicked_reports_nothing() {
    for closed in closed_by_a_defaults_panic::<1>() {
        assert_eq!(closed.second, ExecutorReport::default(), "{}", closed.form);
    }
}

/// A device driver with two bugs: its report's `Default` panics as
/// `close` makes the report, and its states fail to power down as that
/// panic unwinds.
/// In each form whose states are freed as that panic unwinds (all but
/// shots placed each, whose drops panic on their placements), the
/// `Default`'s panic reaches the closer, every state freed, and the
/// states' are left to the panic hook, rather than abort the process.
#[test]
fn a_reports_default_panic_wins_over_its_states_drops() {
    fn closed<E>(form: &str, shots: &[ShotId], mut executor: E, freed: &Freed)
    where
        E: Executor<InstructionBatch<Op>> + Close<Report = ExecutorReport<DefaultPanicsOnce<2>>>,
    {
        for &shot in shots {
            run(&mut executor, shot, &[Op::Flip]);
        }
        DEFAULT_ARMED[2].store(1, Ordering::SeqCst);
        let panicked = panic::catch_unwind(AssertUnwindSafe(|| executor.close()))
            .expect_err("the report's default panics");
        assert_eq!(
            freed_so_far(freed).len(),
            shots.len(),
            "{form}: every state's drop ran, and panicked, as that panic unwound"
        );
        assert_eq!(
            panic_text(&*panicked),
            Some("the report's default has a bug"),
            "{form}"
        );
    }
    let report = |_: &PowersDownBadly| DefaultPanicsOnce::<2>(1);
    let per_shot = [0, 1, 2];
    let freed = Freed::default();
    let one_state = executor_fn(promise(), powering_down_badly(&freed, None), runs);
    closed("one state", &[0], one_state.state_report(report), &freed);
    let freed = Freed::default();
    let one_state = executor_fn(promise(), powering_down_badly(&freed, None), runs);
    let placed = one_state.state_report(report).run_on(OwnThread);
    closed("one state, placed", &[0], placed, &freed);
    let freed = Freed::default();
    let one_state = executor_fn(promise(), powering_down_badly(&freed, None), runs);
    let each = one_state.state_report(report).shots(OnePerShot);
    closed("a state per shot", &per_shot, each, &freed);
    let freed = Freed::default();
    let one_state = executor_fn(promise(), powering_down_badly(&freed, None), runs);
    let placed_whole = (one_state.state_report(report).shots(OnePerShot)).run_on(OwnThread);
    closed(
        "a state per shot, placed whole",
        &per_shot,
        placed_whole,
        &freed,
    );
}

/// A device placed on a thread per job, each job run after a while, whose
/// report's `Default` has a bug, a device driver's: closed while its calls are
/// still queued, each placed form
/// ends every call it accepted before that panic reaches the closer, so
/// that no reply is cancelled, as `close` waits for the calls accepted.
#[test]
fn a_close_whose_reports_default_panics_ends_every_call_queued_first() {
    fn closed<E>(form: &str, shots: &[ShotId], mut executor: E)
    where
        E: Executor<InstructionBatch<Op>> + Close<Report = ExecutorReport<DefaultPanicsOnce<3>>>,
    {
        let replies: Vec<_> = (shots.iter())
            .map(|&shot| {
                executor
                    .call(InstructionBatchRef {
                        shots: &ShotSet::one(shot),
                        instructions: &[Op::Flip],
                    })
                    .expect("an open executor queues a call")
            })
            .collect();
        DEFAULT_ARMED[3].store(1, Ordering::SeqCst);
        let panicked = panic::catch_unwind(AssertUnwindSafe(|| executor.close()))
            .expect_err("the report's default panics");
        assert_eq!(
            panic_text(&*panicked),
            Some("the report's default has a bug"),
            "{form}: the panic reaches the closer"
        );
        let ends: Vec<_> = (replies.into_iter())
            .map(|mut reply| reply.poll_events(&mut Vec::new(), Waker::noop()))
            .collect();
        assert_eq!(ends, vec![ReplyStatus::Done; shots.len()], "{form}");
    }
    let a_late_thread_per_job = |job: PlacementJob| {
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            job();
        });
    };
    let device = || {
        executor_fn(promise(), |_| Ok(()), runs).state_report(|(): &()| DefaultPanicsOnce::<3>(1))
    };
    let per_shot = [0, 1, 0, 1];
    let one_state = device().run_on(a_late_thread_per_job);
    closed("one state, placed", &[0, 0], one_state);
    let placed_whole = device().shots(OnePerShot).run_on(a_late_thread_per_job);
    closed("a state per shot, placed whole", &per_shot, placed_whole);
    let each_placed = device().run_on(a_late_thread_per_job).shots(OnePerShot);
    closed("a state per shot, each placed", &per_shot, each_placed);
}

/// A device whose state closure, logic, check, log probability and
/// report maker each hold a handle on its connection that fails to
/// release (a device driver's bug), and the executor is dropped after a
/// call: its drop releases the five handles, each closure in a catch of
/// its own, and raises no panic of their drops, as for a state's.
#[test]
fn dropped_it_frees_what_its_closures_capture_raising_no_panic_of_their_drops() {
    let (handle, released) = ReleasesBadly::new();
    let (making, checking) = (handle.another(), handle.another());
    let (reading, reporting) = (handle.another(), handle.another());
    let mut executor = executor_fn(
        promise(),
        move |context| {
            let _held = &making;
            Bit::new(context)
        },
        holding_a_handle(Arc::new(handle)),
    )
    .check(move |_: &CallInput<'_, Op>| {
        let _held = &checking;
        Ok(())
    })
    .log_probability(move |_: &Bit| {
        let _held = &reading;
        0.0
    })
    .state_report(move |_: &Bit| {
        let _held = &reporting;
    });
    let (_, end) = send(&mut executor, 0, &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    panic::catch_unwind(AssertUnwindSafe(|| drop(executor)))
        .expect("a drop raises no panic of a capture's drop");
    assert_eq!(released.load(Ordering::SeqCst), 5);
}

/// A device whose every closure holds a handle on its connection, and
/// whose session on shot 0 releases its resources through it, ran a call
/// and is dropped unclosed: the session is dropped first, before any
/// closure closes the connection, as `close` frees it first.
#[test]
fn dropped_it_frees_its_state_before_its_closures() {
    let connection = Connection::default();
    let mut executor = connected_device(&connection);
    let (_, end) = send(&mut executor, 0, &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    drop(executor);
    assert_eq!(connection.logged(), ["released shot 0"]);
}

/// A device's check, log probability and report maker each hold a handle
/// on its connection that fails to release (a device driver's bug), and the
/// caller replaces each with another: each builder frees the closure it
/// replaces, and raises no panic of its drop, as `drop` does.
#[test]
fn a_replaced_closure_is_freed_raising_no_panic_of_its_drop() {
    let (handle, released) = ReleasesBadly::new();
    let (reading, reporting) = (handle.another(), handle.another());
    let executor = executor_fn(promise(), Bit::new, runs)
        .check(move |_: &CallInput<'_, Op>| {
            let _held = &handle;
            Ok(())
        })
        .log_probability(move |_: &Bit| {
            let _held = &reading;
            0.0
        })
        .state_report(move |_: &Bit| {
            let _held = &reporting;
        });
    let replaced = panic::catch_unwind(AssertUnwindSafe(move || {
        executor
            .check(|_: &CallInput<'_, Op>| Ok(()))
            .log_probability(|_: &Bit| 0.0)
            .state_report(|_: &Bit| ())
    }))
    .expect("a builder raises no panic of the replaced closure's drop");
    assert_eq!(released.load(Ordering::SeqCst), 3);
    drop(replaced);
}

/// A caller panics while it holds the executor of a device whose logic
/// and check each hold a handle that fails to release: the executor,
/// dropped as the caller's panic unwinds, releases both handles, each in a
/// catch of its own, and the caller's panic reaches its caller rather than
/// abort the process.
#[test]
fn dropped_as_a_panic_unwinds_it_frees_what_its_closures_capture_raising_none() {
    let (handle, released) = ReleasesBadly::new();
    let checked = handle.another();
    let executor = executor_fn(promise(), Bit::new, holding_a_handle(Arc::new(handle))).check(
        move |_: &CallInput<'_, Op>| {
            let _held = &checked;
            Ok(())
        },
    );
    let panicked = panic::catch_unwind(AssertUnwindSafe(move || {
        let _held = executor;
        panic!("the caller panics");
    }))
    .expect_err("the caller panics");
    assert_eq!(panic_text(&*panicked), Some("the caller panics"));
    assert_eq!(released.load(Ordering::SeqCst), 2);
}
