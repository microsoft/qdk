// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod placed;

use std::cell::{Cell, RefCell};
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::{
    Bit, Flipped, Flips, Freed, Op, PowerDownFailed, PowersDownBadly, ReleasesBadly, count_flips,
    draw_value, flips, freed_so_far, holding_a_handle, narrow_promise, panic_text, panics,
    powering_down_badly, promise, read_drawn_bits, readouts, run_bit, runs, stops, values,
};
use crate::{
    Close, Describe, Executor, Fold, InstructionBatch, InstructionBatchRef, OnePerShot, Outcome,
    Outcomes, OwnThread, ProgramInput, Rejected, ShotContext, ShotId, ShotOutcome, ShotSet, Value,
    check_unique_tags, conformance, executor_fn, wait,
};

/// The run's seed in these tests.
const SEED: u64 = 7;

/// The reply of `executor` to `ops` on `shots`, in that order: its events
/// and its end; or the refusal.
fn send(
    executor: &mut impl Executor<InstructionBatch<Op>>,
    shots: &[ShotId],
    ops: &[Op],
) -> Result<(Vec<ShotOutcome>, Result<(), String>), Rejected> {
    let shots = ShotSet::new(shots.to_vec()).expect("the tests name each shot once");
    let reply = executor.call(InstructionBatchRef {
        shots: &shots,
        instructions: ops,
    })?;
    let mut outcomes = Vec::new();
    let end = wait(reply, &mut outcomes);
    Ok((outcomes, end))
}

/// The outcomes of `executor`'s reply to `ops` on `shots`, which it runs.
fn run(
    executor: &mut impl Executor<InstructionBatch<Op>>,
    shots: &[ShotId],
    ops: &[Op],
) -> Vec<ShotOutcome> {
    let (outcomes, end) = send(executor, shots, ops).expect("the executor runs the call");
    end.expect("the logic does not fail");
    outcomes
}

/// The shot of each of `outcomes`.
fn shots(outcomes: &[ShotOutcome]) -> Vec<ShotId> {
    outcomes.iter().map(|o| o.shot).collect()
}

/// A count, and its copy for a closure to update.
fn counter() -> (Rc<Cell<u32>>, Rc<Cell<u32>>) {
    let count = Rc::new(Cell::new(0));
    (Rc::clone(&count), count)
}

/// A state that is its shot: with [`run_shot_state`], a device that stops
/// on shot 1; with [`minus_shot`], each shot's log probability told apart.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the form of a state closure, passed by name"
)]
fn shot_state(context: ShotContext) -> Result<ShotId, Rejected> {
    Ok(context.shot())
}

/// The logic of [`shot_state`]: reads one under each readout's tag, then
/// stops on shot 1.
fn run_shot_state(
    shot: &mut ShotId,
    input: &ProgramInput<'_, Op>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    for (_, tag) in readouts(input.instructions()) {
        outcomes.push(Outcome::new(tag, Value::One));
    }
    if *shot == 1 {
        return Err("shot 1 stopped".to_owned());
    }
    Ok(())
}

/// The log probability of [`shot_state`]: minus its shot.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "the form of a log probability, passed by name"
)]
fn minus_shot(shot: &ShotId) -> f64 {
    -f64::from(*shot)
}

/// A simulator of a whole run seeded with 7: shot 2's state is made at the
/// shot's first call, from the run's seed and the shot, and serves its
/// later calls without being made again.
#[test]
fn a_shots_state_is_made_at_its_first_call_from_the_runs_seed_and_the_shot() {
    let made = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&made);
    let mut executor = executor_fn(
        promise(),
        move |context: ShotContext| {
            log.borrow_mut().push((context.run_seed(), context.shot()));
            Bit::new(context)
        },
        run_bit,
    )
    .seed(SEED)
    .shots(OnePerShot);
    run(&mut executor, &[2], &[Op::Flip]);
    assert_eq!(*made.borrow(), [(SEED, 2)]);
    run(&mut executor, &[2, 5], &[Op::Flip]);
    assert_eq!(*made.borrow(), [(SEED, 2), (SEED, 5)]);
}

/// A shot flipped in a first call reads one in the next, while a shot new
/// to that call reads zero: each call continues its shots' states.
#[test]
fn a_later_call_continues_each_shot_from_its_state() {
    let mut executor = executor_fn(promise(), Bit::new, run_bit).shots(OnePerShot);
    run(&mut executor, &[0], &[Op::Flip]);
    let outcomes = run(&mut executor, &[1, 0], &[Op::Read(0)]);
    assert_eq!(shots(&outcomes), [1, 0]);
    assert_eq!(values(&outcomes), [(0, Value::Zero), (0, Value::One)]);
}

/// A batch on shots 3, 1 and 2, in that order: the logic runs on each
/// shot in the batch's order, so their outcomes come in that order.
#[test]
fn a_batch_runs_on_each_of_its_shots_in_its_order() {
    let mut executor = executor_fn(promise(), Bit::new, run_bit).shots(OnePerShot);
    let outcomes = run(&mut executor, &[3, 1, 2], &[Op::Read(0), Op::Read(1)]);
    assert_eq!(shots(&outcomes), [3, 3, 1, 1, 2, 2]);
}

/// A simulator whose state is a value drawn from its shot's seed, and whose
/// readout under tag `t` is bit `t` of that value.
fn drawing_executor() -> impl Executor<InstructionBatch<Op>> {
    executor_fn(promise(), draw_value, read_drawn_bits)
        .seed(SEED)
        .shots(OnePerShot)
}

/// A state that draws its value from its seed: each of shots 1, 3 and 5
/// reads the same 16 bits of the value in two calls, whether it runs
/// alone, among the others, or with them split into calls in another
/// order, since its state's seed is its own: the conformance kit's check
/// of `replayable`.
#[test]
fn each_shot_alone_gives_what_it_gives_among_others() {
    let ops: Vec<_> = (0..16).map(Op::Read).collect();
    conformance::gives_each_shot_alone_what_it_gives_among_others(
        drawing_executor,
        &ShotSet::new(vec![1, 3, 5]).expect("three shots"),
        &[&ops[..8], &ops[8..]],
    );
}

/// A device of two states, called on three shots with a batch its check
/// refuses: the check's refusal comes first, not the shot it has no room
/// for.
#[test]
fn the_check_refuses_a_call_before_its_room_is_checked() {
    let mut executor = executor_fn(promise().with_states(2), Bit::new, run_bit)
        .check(|input| check_unique_tags(readouts(input.instructions())))
        .shots(OnePerShot);
    let refused = send(&mut executor, &[0, 1, 2], &[Op::Read(1), Op::Read(1)]).err();
    assert!(
        matches!(refused, Some(Rejected::Instruction { at: 1, .. })),
        "{refused:?}"
    );
}

/// A device of two states, closed, then called on three shots: the call
/// is refused as closed, not for the shot it has no room for.
#[test]
fn closed_it_refuses_a_call_before_its_room_is_checked() {
    let mut executor = executor_fn(promise().with_states(2), Bit::new, run_bit).shots(OnePerShot);
    executor.close();
    assert_eq!(
        send(&mut executor, &[0, 1, 2], &[Op::Flip]).err(),
        Some(Rejected::Closed)
    );
}

/// A simulator closed after a call, then called with a batch that its own
/// check refuses: the call is refused as closed, not by the check.
#[test]
fn a_closed_executor_says_closed_whatever_the_batch() {
    let mut executor = executor_fn(promise(), Bit::new, run_bit)
        .check(|input| check_unique_tags(readouts(input.instructions())))
        .shots(OnePerShot);
    run(&mut executor, &[0], &[Op::Flip]);
    executor.close();
    assert_eq!(
        send(&mut executor, &[0], &[Op::Read(1), Op::Read(1)]).err(),
        Some(Rejected::Closed)
    );
}

/// A device that stopped on shot 1, then called with a batch that its own
/// check refuses: the call is refused with the device's failure, not by
/// the check, so the caller learns first that the run is lost.
#[test]
fn a_failed_executor_names_its_failure_whatever_the_batch() {
    let mut executor = executor_fn(promise(), shot_state, run_shot_state)
        .check(|input| check_unique_tags(readouts(input.instructions())))
        .shots(OnePerShot);
    let (_, end) = send(&mut executor, &[1], &[]).expect("the call runs");
    end.expect_err("shot 1 stops");
    assert_eq!(
        send(&mut executor, &[0], &[Op::Read(1), Op::Read(1)]).err(),
        Some(Rejected::EarlierFailure("shot 1 stopped".to_owned()))
    );
}

/// A device of two states: a call that would hold a third shot is refused
/// with that shot, and runs nothing; a call within its two runs.
#[test]
fn a_call_beyond_the_promised_states_is_refused_with_the_first_shot_it_has_no_room_for() {
    let (ran, counted) = counter();
    let mut executor = executor_fn(
        promise().with_states(2),
        Bit::new,
        move |bit, input, outcomes| {
            counted.set(counted.get() + 1);
            run_bit(bit, input, outcomes)
        },
    )
    .shots(OnePerShot);
    assert_eq!(
        send(&mut executor, &[0, 1, 2], &[Op::Flip]).err(),
        Some(Rejected::ShotNotHeld(2))
    );
    assert_eq!(ran.get(), 0, "the refused call runs nothing");
    run(&mut executor, &[0, 1], &[Op::Flip]);
    run(&mut executor, &[1, 0], &[Op::Flip]);
    assert_eq!(
        send(&mut executor, &[0, 2], &[Op::Flip]).err(),
        Some(Rejected::ShotNotHeld(2))
    );
}

/// A device of two states, a call on three new shots: the call is refused
/// for room before any state is made, so the device makes none.
#[test]
fn a_call_refused_for_room_makes_no_state() {
    let (made, counted) = counter();
    let mut executor = executor_fn(
        promise().with_states(2),
        move |context: ShotContext| {
            counted.set(counted.get() + 1);
            Bit::new(context)
        },
        run_bit,
    )
    .shots(OnePerShot);
    send(&mut executor, &[0, 1, 2], &[Op::Flip]).expect_err("two states, three shots");
    assert_eq!(made.get(), 0);
}

/// A device whose shot 1 cannot be made, being busy: the whole call is
/// refused with the device's refusal, and no shot runs, not even shot 0,
/// whose state could be made.
#[test]
fn a_state_that_cannot_be_made_refuses_the_whole_call_with_its_refusal() {
    let (ran, counted) = counter();
    let mut executor = executor_fn(
        promise(),
        |context: ShotContext| {
            if context.shot() == 1 {
                return Err(Rejected::Unavailable("shot 1 is busy".to_owned()));
            }
            Bit::new(context)
        },
        move |bit, input, outcomes| {
            counted.set(counted.get() + 1);
            run_bit(bit, input, outcomes)
        },
    )
    .shots(OnePerShot);
    assert_eq!(
        send(&mut executor, &[0, 1], &[Op::Flip]).err(),
        Some(Rejected::Unavailable("shot 1 is busy".to_owned()))
    );
    assert_eq!(ran.get(), 0);
}

/// A device busy at a first call on shots 0 and 1: the state made for
/// shot 0 is dropped with the refused call, made again at the next, and
/// reported once.
#[test]
fn the_states_made_for_a_refused_call_are_not_kept() {
    let (made, counted) = counter();
    let mut busy = true;
    let mut executor = executor_fn(
        promise(),
        move |context: ShotContext| {
            counted.set(counted.get() + 1);
            if context.shot() == 1 && std::mem::take(&mut busy) {
                return Err(Rejected::Unavailable("the device is busy".to_owned()));
            }
            Bit::new(context)
        },
        run_bit,
    )
    .log_probability(|_| -1.0)
    .shots(OnePerShot);
    send(&mut executor, &[0, 1], &[Op::Flip]).expect_err("shot 1 is busy");
    assert_eq!(made.get(), 2, "shot 0's state, then shot 1's refusal");
    run(&mut executor, &[0, 1], &[Op::Flip]);
    assert_eq!(made.get(), 4, "both made again");
    assert_eq!(executor.close().log_p.get(0), Some(-1.0));
}

/// A device that reads, then stops, in shot 0 of a batch on shots 0, 1
/// and 2: the reply hands over shot 0's readout, made before the stop,
/// then fails with the logic's reason.
#[test]
fn a_failed_logic_keeps_the_outcomes_before_it() {
    let mut executor = executor_fn(promise(), Bit::new, run_bit).shots(OnePerShot);
    let (outcomes, end) =
        send(&mut executor, &[0, 1, 2], &[Op::Read(0), Op::Stop]).expect("the call runs");
    assert_eq!(shots(&outcomes), [0]);
    assert_eq!(end, Err("the device stopped".to_owned()));
}

/// A device that stops in shot 0 of a batch on shots 0, 1 and 2: shots 1
/// and 2 do not run, since the call failed and the executor with it.
#[test]
fn after_a_shots_failure_the_later_shots_of_its_batch_do_not_run() {
    let (ran, counted) = counter();
    let mut executor = executor_fn(promise(), Bit::new, move |bit, input, outcomes| {
        counted.set(counted.get() + 1);
        run_bit(bit, input, outcomes)
    })
    .shots(OnePerShot);
    send(&mut executor, &[0, 1, 2], &[Op::Stop])
        .expect("the call runs")
        .1
        .expect_err("shot 0 stops");
    assert_eq!(ran.get(), 1, "shot 0's logic alone");
}

/// A failed call reports no state made for it, whether its shot ran or
/// not; a state made by an earlier call is reported. Chosen so that every
/// form of the executor, placed or not, reports the same states, though a
/// placed form may run shots of the failed call that the others never
/// reach; `futures`' `try_join_all` and rayon's `collect::<Result<_, _>>()`
/// drop the others' results too. Shot 0 runs a call, then a device stops
/// on shot 1 of a batch on shots 0, 2, 1 and 3: at close, shot 0's state is
/// reported; shot 2's, new and run in full before the stop, shot 1's, which
/// stopped, and shot 3's, which did not run, are not.
#[test]
fn a_failed_call_reports_no_state_made_for_it() {
    let mut executor = executor_fn(promise(), shot_state, run_shot_state)
        .log_probability(minus_shot)
        .shots(OnePerShot);
    let (_, end) = send(&mut executor, &[0], &[]).expect("the first call runs");
    end.expect("shot 0 does not stop");
    let (_, end) = send(&mut executor, &[0, 2, 1, 3], &[]).expect("the call runs");
    end.expect_err("shot 1 stops");
    let report = executor.close();
    assert_eq!(
        [0, 1, 2, 3].map(|j| report.log_p.get(j)),
        [Some(0.0), None, None, None]
    );
}

/// Once a shot's device stopped, the executor refuses every call: the
/// conformance kit's check.
#[test]
fn after_a_failed_call_it_refuses_every_later_call() {
    let (failing, later) = (
        ShotSet::new(vec![0, 1]).expect("two shots"),
        ShotSet::one(0),
    );
    conformance::refuses_calls_after_a_failure::<InstructionBatch<Op>, _>(
        &mut executor_fn(promise(), shot_state, run_shot_state).shots(OnePerShot),
        InstructionBatchRef {
            shots: &failing,
            instructions: &[],
        },
        &[
            InstructionBatchRef {
                shots: &later,
                instructions: &[],
            },
            InstructionBatchRef {
                shots: &later,
                instructions: &[],
            },
        ],
    );
}

/// A driver sends a call on shots 0 and 1, whose device stops on shot 1,
/// then a call on shot 2 before it waits for the first reply: the failure
/// ends the whole executor, so the call on another shot is refused, naming
/// it. The conformance kit's check.
#[test]
fn a_call_in_flight_on_another_shot_ends_at_an_earlier_failure() {
    let (failing, later) = (
        ShotSet::new(vec![0, 1]).expect("two shots"),
        ShotSet::one(2),
    );
    conformance::ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<Op>, _>(
        &mut executor_fn(promise(), shot_state, run_shot_state).shots(OnePerShot),
        InstructionBatchRef {
            shots: &failing,
            instructions: &[],
        },
        InstructionBatchRef {
            shots: &later,
            instructions: &[],
        },
    );
}

/// The logic of [`shot_state`] under a driver with a bug: where shot 1
/// stops, it panics instead of failing the call.
fn panic_on_shot_1(
    shot: &mut ShotId,
    input: &ProgramInput<'_, Op>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    run_shot_state(shot, input, outcomes).map_err(|reason| panic!("{reason}"))
}

/// A driver with a bug panics on shot 1 of a batch on shots 0 and 1,
/// after shot 0 ran it: the panic reaches the caller, who never gets shot
/// 0's outcomes; as a `Mutex` is poisoned, the executor refuses every
/// later call, naming the panic, rather than run shot 0 on a state the
/// caller's program never made.
#[test]
fn after_a_panic_inside_call_it_refuses_every_later_call() {
    let mut executor = executor_fn(promise(), shot_state, panic_on_shot_1).shots(OnePerShot);
    panic::catch_unwind(AssertUnwindSafe(|| send(&mut executor, &[0, 1], &[])))
        .expect_err("the driver's panic reaches the caller");
    assert_eq!(
        send(&mut executor, &[0], &[]).err(),
        Some(Rejected::EarlierFailure(
            "the executor panicked during the call".to_owned()
        ))
    );
}

/// A device whose state closure has a bug: it panics making shot 1's
/// state, in a batch on shots 0 and 1, after it made shot 0's. No shot ran
/// the call, so the executor reports no state at close, as after a state
/// that cannot be made.
#[test]
fn the_states_made_for_a_call_a_state_closure_panicked_in_are_not_kept() {
    let mut executor = executor_fn(
        promise(),
        |context: ShotContext| {
            assert!(context.shot() != 1, "the device's driver has a bug");
            shot_state(context)
        },
        run_shot_state,
    )
    .log_probability(minus_shot)
    .shots(OnePerShot);
    panic::catch_unwind(AssertUnwindSafe(|| send(&mut executor, &[0, 1], &[])))
        .expect_err("the state closure's panic reaches the caller");
    let report = executor.close();
    assert_eq!([0, 1].map(|j| report.log_p.get(j)), [None, None]);
}

/// A call that panicked reports no state made for it, as a failed call
/// does, whether its shot ran or not; a state made by an earlier call is
/// reported. Chosen so that every form of the executor reports the same
/// states, as after a failure. Shot 0 runs a call, then a driver with a
/// bug panics on shot 1 of a batch on shots 0, 2, 1 and 3: at close, only
/// shot 0's state is reported, not shot 2's, new and run in full before
/// the panic.
#[test]
fn a_call_that_panicked_reports_no_state_made_for_it() {
    let mut executor = executor_fn(promise(), shot_state, panic_on_shot_1)
        .log_probability(minus_shot)
        .shots(OnePerShot);
    let (_, end) = send(&mut executor, &[0], &[]).expect("the first call runs");
    end.expect("shot 0 does not panic");
    panic::catch_unwind(AssertUnwindSafe(|| send(&mut executor, &[0, 2, 1, 3], &[])))
        .expect_err("the driver's panic reaches the caller");
    let report = executor.close();
    assert_eq!(
        [0, 1, 2, 3].map(|j| report.log_p.get(j)),
        [Some(0.0), None, None, None]
    );
}

/// Once closed, the executor refuses every call: the conformance kit's
/// check.
#[test]
fn closed_it_refuses_every_call() {
    let shots = ShotSet::new(vec![0, 1]).expect("two shots");
    conformance::rejects_calls_once_closed::<InstructionBatch<Op>, _>(
        &mut executor_fn(promise(), Bit::new, run_bit).shots(OnePerShot),
        InstructionBatchRef {
            shots: &shots,
            instructions: &[Op::Flip],
        },
    );
}

/// An executor that ran a call reports once, its states' reports
/// included: the conformance kit's check.
#[test]
fn a_second_close_reports_nothing() {
    let mut executor = executor_fn(promise(), Flipped::new, count_flips)
        .log_probability(|_| -1.0)
        .state_report(flips)
        .shots(OnePerShot);
    run(&mut executor, &[0, 1], &[Op::Flip]);
    conformance::reports_once(&mut executor);
}

/// A device of two states refuses a call that would hold a third shot,
/// then runs one on its two: the conformance kit's check that a refusal
/// leaves the executor usable.
#[test]
fn a_refused_call_leaves_it_usable() {
    let (three, two) = (
        ShotSet::range(0..=2).expect("three shots"),
        ShotSet::range(0..=1).expect("two shots"),
    );
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<Op>, _>(
        &mut executor_fn(promise().with_states(2), Bit::new, run_bit).shots(OnePerShot),
        InstructionBatchRef {
            shots: &three,
            instructions: &[Op::Flip],
        },
        InstructionBatchRef {
            shots: &two,
            instructions: &[Op::Flip, Op::Read(0)],
        },
    );
}

/// The device's check refuses a tag repeated within a batch of two shots,
/// and only that, at the instruction that repeats it: the conformance
/// kit's checks.
#[test]
fn a_tag_repeated_within_a_batch_is_refused() {
    let mut per_shot = executor_fn(promise(), Bit::new, run_bit)
        .check(|input| check_unique_tags(readouts(input.instructions())))
        .shots(OnePerShot);
    let two = ShotSet::range(0..=1).expect("two shots");
    // The device has one bit, so `a_again` reads `a`'s. The first check still
    // catches a refusal of a repeated qubit, since `b` reads that bit too and
    // `[a, b]` must be accepted; the index check alone could not.
    let (a, b, a_again) = (Op::Read(0), Op::Read(1), Op::ReadInverted(0));
    conformance::refuses_a_tag_repeated_within_a_batch(&mut per_shot, &two, &a, &b, &a_again);
    conformance::refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut per_shot,
        &two,
        &a,
        &Op::Flip,
        &a_again,
    );
}

/// A second call on shots 7, 2 and 5, given out of order and with gaps,
/// reads the flip of the first: the conformance kit's check of `feedback`.
#[test]
fn a_second_call_continues_each_shot() {
    conformance::continues_a_shot_from_one_call_to_the_next(
        || executor_fn(promise(), Bit::new, run_bit).shots(OnePerShot),
        &ShotSet::new(vec![7, 2, 5]).expect("three shots"),
        &[Op::Flip],
        &[Op::Read(0)],
    );
}

/// A flip on shots 3 and 9 leaves shots 1, 7 and 21, flipped before, as
/// they were, though the driver gives the shots out of order and with
/// gaps: the conformance kit's check that a call changes no shot it does
/// not name.
#[test]
fn a_call_changes_no_shot_it_does_not_name() {
    conformance::changes_no_shot_a_call_does_not_name(
        || executor_fn(promise(), Bit::new, run_bit).shots(OnePerShot),
        &ShotSet::new(vec![9, 3, 7, 21, 1]).expect("five shots"),
        &[Op::Flip],
        &[Op::Read(0)],
    );
}

/// After an executor flipped shots 0 to 3 and was closed, or dropped
/// unclosed, the next one reads them as made: the conformance kit's check
/// that a new executor starts its shots as made.
#[test]
fn a_new_executor_starts_its_shots_as_made() {
    conformance::starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || executor_fn(promise(), Bit::new, run_bit).shots(OnePerShot),
        &ShotSet::range(0..=3).expect("four shots"),
        &[Op::Flip],
        &[Op::Read(0)],
    );
}

/// A simulator ran shots 0 and 2, then 2 and 3: closing reports the log
/// probability of each shot's state once, whatever the number of its
/// calls, and none for shot 1, which never ran.
#[test]
fn closing_reports_the_log_probability_of_each_state_once() {
    let mut executor = executor_fn(promise(), shot_state, |_, _, _| Ok(()))
        .log_probability(minus_shot)
        .shots(OnePerShot);
    run(&mut executor, &[0, 2], &[]);
    run(&mut executor, &[2, 3], &[]);
    let report = executor.close();
    assert_eq!(
        [0, 1, 2, 3].map(|j| report.log_p.get(j)),
        [Some(0.0), None, Some(-2.0), Some(-3.0)]
    );
}

/// A simulator whose states count their flips ran shots 0 and 1, then
/// shot 1 again: closing folds the report of each state once, as its
/// calls left it. The example of `ExecutorFn::state_report`'s docs.
#[test]
fn closing_reports_each_state_once_as_its_calls_left_it() {
    let mut executor = executor_fn(promise(), Flipped::new, count_flips)
        .state_report(flips)
        .shots(OnePerShot);
    run(&mut executor, &[0, 1], &[Op::Flip]);
    run(&mut executor, &[1], &[Op::Flip, Op::Flip]);
    assert_eq!(executor.close().states, Flips::of([(0, 1), (1, 3)]));
}

/// An MPS simulator whose states keep the largest bond dimension they
/// reached: a flip doubles a state's bond, a readout collapses it to one.
/// Shot 0 flips twice (bond 4), then, in a later call, is read (bond 1);
/// shot 1 flips once (bond 2). The report, folded by maximum, gives the
/// peak that shot 0's state tracked across its calls, which its last call
/// left behind.
#[test]
fn a_state_reports_what_it_tracked_across_its_calls() {
    /// A shot's state: its bond dimension now, and the largest it reached.
    struct Mps {
        bond: u32,
        peak: u32,
    }
    /// The largest bond dimension of the states folded.
    #[derive(Debug, Default, PartialEq)]
    struct PeakBond(u32);
    impl Fold for PeakBond {
        fn fold(&mut self, other: Self) {
            self.0 = self.0.max(other.0);
        }
    }
    let logic = |mps: &mut Mps, input: &ProgramInput<'_, Op>, outcomes: &mut Outcomes<'_>| {
        for op in input.instructions() {
            match *op {
                Op::Flip => mps.bond *= 2,
                Op::Read(tag) | Op::ReadInverted(tag) => {
                    mps.bond = 1;
                    outcomes.push(Outcome::new(tag, Value::Zero));
                }
                Op::Stop => return Err("the device stopped".to_owned()),
            }
            mps.peak = mps.peak.max(mps.bond);
        }
        Ok(())
    };
    let mut executor = executor_fn(promise(), |_| Ok(Mps { bond: 1, peak: 1 }), logic)
        .state_report(|mps: &Mps| PeakBond(mps.peak))
        .shots(OnePerShot);
    run(&mut executor, &[0], &[Op::Flip, Op::Flip]);
    run(&mut executor, &[0, 1], &[Op::Read(0), Op::Flip]);
    assert_eq!(executor.close().states, PeakBond(4));
}

/// A device that stops on shot 1 of a batch on shots 1 and 2, after a
/// call on shots 0 and 1: the failed executor keeps the states made before
/// the failed call until it closes, and reports them, shot 1's as the
/// failed call left it; shot 2's state, made for the failed call, has no
/// report.
#[test]
fn a_failed_executor_still_reports_the_states_made_before_the_failed_call() {
    let mut executor = executor_fn(promise(), Flipped::new, count_flips)
        .state_report(flips)
        .shots(OnePerShot);
    run(&mut executor, &[0, 1], &[Op::Flip]);
    let (_, end) = send(&mut executor, &[1, 2], &[Op::Flip, Op::Stop]).expect("the call runs");
    end.expect_err("the device stops");
    assert_eq!(executor.close().states, Flips::of([(0, 1), (1, 2)]));
}

/// A simulator holding a resource per shot releases them when its executor
/// closes: the states are dropped then.
#[test]
fn closing_drops_the_states() {
    /// A state that counts its drops.
    struct Held(Rc<Cell<u32>>);
    impl Drop for Held {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let (dropped, counted) = counter();
    let mut executor = executor_fn(
        promise(),
        move |_| Ok(Held(Rc::clone(&counted))),
        |_: &mut Held, _: &ProgramInput<'_, Op>, _: &mut Outcomes<'_>| Ok(()),
    )
    .shots(OnePerShot);
    run(&mut executor, &[0, 1], &[Op::Flip]);
    assert_eq!(dropped.get(), 0, "the states are kept until close");
    executor.close();
    assert_eq!(dropped.get(), 2);
}

/// A simulator of a whole run that states no limit, or every limit it
/// can: a state per shot neither lowers nor raises its promise, so it
/// describes the promise as given.
#[test]
fn describes_the_promise_as_given() {
    for given in [promise(), narrow_promise()] {
        let executor = executor_fn(given.clone(), Bit::new, run_bit).shots(OnePerShot);
        assert_eq!(executor.capabilities(), given);
    }
}

/// A simulator promises each shot's log probability, or does not say: it
/// promises it exactly when it is told how to read it from a state.
#[test]
fn promises_probabilities_only_with_a_log_probability() {
    for given in [true, false] {
        let promise = promise().with_probabilities(given);
        let without = executor_fn(promise.clone(), Bit::new, run_bit).shots(OnePerShot);
        assert!(!without.capabilities().probabilities, "given {given}");
        let with = executor_fn(promise, Bit::new, run_bit)
            .log_probability(|_| 0.0)
            .shots(OnePerShot);
        assert!(with.capabilities().probabilities, "given {given}");
    }
}

/// A simulator's check refuses a batch that reads twice under one tag: no
/// state is made and no shot runs.
#[test]
fn the_check_refuses_a_call_before_any_state_is_made() {
    let (made, counted) = counter();
    let mut executor = executor_fn(
        promise(),
        move |context: ShotContext| {
            counted.set(counted.get() + 1);
            Bit::new(context)
        },
        run_bit,
    )
    .check(|input| check_unique_tags(readouts(input.instructions())))
    .shots(OnePerShot);
    let refused = send(&mut executor, &[0, 1], &[Op::Read(1), Op::Read(1)]).err();
    assert!(
        matches!(refused, Some(Rejected::Instruction { at: 1, .. })),
        "{refused:?}"
    );
    assert_eq!(made.get(), 0);
}

/// A simulator of two states runs a call, refuses one beyond its states,
/// then stops in a third: its report counts the two it accepted.
#[test]
fn the_report_counts_the_calls_it_accepted() {
    let mut executor =
        executor_fn(promise().with_states(2), shot_state, run_shot_state).shots(OnePerShot);
    run(&mut executor, &[0], &[]);
    send(&mut executor, &[2, 3, 4], &[]).expect_err("no room for a third state");
    let (_, end) = send(&mut executor, &[1], &[]).expect("the call runs");
    end.expect_err("shot 1 stops");
    assert_eq!(executor.close().calls, 2);
}

/// An executor already called on shot 4, then given a shot strategy: the
/// strategy continues it, as one executor, not a new one: shot 4 keeps
/// the state its earlier call left (its bit reads one, where the new
/// shot 5's reads zero), the earlier call is counted with the later one,
/// and shot 4's log probability is reported.
#[test]
fn a_shot_strategy_continues_the_executor_it_is_given() {
    let mut one_state = executor_fn(promise(), Bit::new, run_bit).log_probability(|_| -1.0);
    run(&mut one_state, &[4], &[Op::Flip]);
    let mut executor = one_state.shots(OnePerShot);
    let outcomes = run(&mut executor, &[4, 5], &[Op::Read(0)]);
    assert_eq!(values(&outcomes), [(0, Value::One), (0, Value::Zero)]);
    let report = executor.close();
    assert_eq!(report.calls, 2);
    assert_eq!(report.log_p.get(4), Some(-1.0));
}

/// A wrapper never forgets a refusal: an executor closed, then given a state
/// per shot, still refuses every call as closed. Chosen because a wrapper
/// continues what it wraps, failures included, as iterator adapters and
/// Tower's `Buffer` do.
#[test]
fn given_a_state_per_shot_after_close_it_still_refuses() {
    let mut one_state = executor_fn(promise(), Bit::new, run_bit);
    one_state.close();
    let mut executor = one_state.shots(OnePerShot);
    assert_eq!(
        send(&mut executor, &[0], &[Op::Flip]).err(),
        Some(Rejected::Closed)
    );
}

/// A wrapper never forgets a refusal: an executor whose call failed, then
/// given a state per shot, still refuses every call with the failure,
/// whatever shots it names. Chosen because a wrapper continues what it
/// wraps, failures included, as iterator adapters and Tower's `Buffer` do.
/// A device stops in a call on shot 0; a call on shot 1 follows.
#[test]
fn given_a_state_per_shot_after_a_failure_it_still_refuses() {
    let mut one_state = executor_fn(promise(), Bit::new, run_bit);
    let (_, end) = send(&mut one_state, &[0], &[Op::Stop]).expect("the call runs");
    end.expect_err("the device stops");
    let mut executor = one_state.shots(OnePerShot);
    assert_eq!(
        send(&mut executor, &[1], &[Op::Flip]).err(),
        Some(Rejected::EarlierFailure("the device stopped".to_owned()))
    );
}

/// A wrapper never forgets a refusal: an executor closed, given a state per
/// shot, then placed on a thread of its own, refuses the next call at
/// `call` as closed, rather than accept it and fail its reply on that
/// thread. Chosen because a wrapper continues what it wraps, failures
/// included, as iterator adapters and Tower's `Buffer` do.
#[test]
fn given_a_state_per_shot_then_placed_after_close_it_refuses_at_call() {
    let mut executor = executor_fn(promise(), Bit::new, run_bit).shots(OnePerShot);
    executor.close();
    let mut placed = executor.run_on(OwnThread);
    assert_eq!(
        send(&mut placed, &[0], &[Op::Flip]).err(),
        Some(Rejected::Closed)
    );
}

/// A wrapper never forgets a refusal: an executor given a state per shot
/// whose call failed, then placed on a thread of its own, refuses the next
/// call at `call` with the failure, rather than accept it and fail its
/// reply on that thread. Chosen because a wrapper continues what it wraps,
/// failures included, as iterator adapters and Tower's `Buffer` do. A
/// device stops in a call on shot 0; a call on shot 1 follows.
#[test]
fn given_a_state_per_shot_then_placed_after_a_failure_it_refuses_at_call() {
    let mut executor = executor_fn(promise(), Bit::new, run_bit).shots(OnePerShot);
    let (_, end) = send(&mut executor, &[0], &[Op::Stop]).expect("the call runs");
    end.expect_err("the device stops");
    let mut placed = executor.run_on(OwnThread);
    assert_eq!(
        send(&mut placed, &[1], &[Op::Flip]).err(),
        Some(Rejected::EarlierFailure("the device stopped".to_owned()))
    );
}

/// A device whose states fail to power down (a driver's bug) stops in a
/// call on three new shots: the call frees each state made for it, and
/// the first one's panic reaches the caller once every state is freed,
/// rather than abort the process.
#[test]
fn a_failed_call_frees_every_state_made_for_it_then_raises_the_first_drops_panic() {
    let freed = Freed::default();
    let mut executor =
        executor_fn(promise(), powering_down_badly(&freed, None), stops).shots(OnePerShot);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut executor, &[0, 1, 2], &[Op::Flip])
    }))
    .expect_err("the states' drops panic");
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0, 1, 2]);
}

/// A device whose states fail to power down stops in a call on two new
/// shots, whose states' drops then panic: the call failed first, so later
/// calls are refused with the logic's reason, not the panic's.
#[test]
fn after_a_failed_calls_states_drops_panic_later_calls_get_the_logics_reason() {
    let freed = Freed::default();
    let mut executor =
        executor_fn(promise(), powering_down_badly(&freed, None), stops).shots(OnePerShot);
    panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut executor, &[0, 1], &[Op::Flip])
    }))
    .expect_err("the states' drops panic");
    assert_eq!(
        send(&mut executor, &[5], &[Op::Flip]).err(),
        Some(Rejected::EarlierFailure("the device stopped".to_owned()))
    );
}

/// A device whose states fail to power down, and whose logic panics
/// (two bugs), in a call on two new shots: the logic's panic reaches the
/// caller, and the states, freed as it unwinds, leave theirs to the panic
/// hook, rather than abort the process.
#[test]
fn a_logics_panic_reaches_the_caller_though_its_new_states_drops_panic_too() {
    let freed = Freed::default();
    let mut executor =
        executor_fn(promise(), powering_down_badly(&freed, None), panics).shots(OnePerShot);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut executor, &[0, 1], &[Op::Flip])
    }))
    .expect_err("the logic panics");
    assert_eq!(panic_text(&*panicked), Some("the driver panics"));
    assert_eq!(freed_so_far(&freed), [0, 1]);
}

/// A device whose states fail to power down has no room for shot 2: a
/// call on shots 0, 1 and 2 frees the states it made for shots 0 and 1,
/// and the first one's panic reaches the caller once both are freed,
/// rather than abort the process.
#[test]
fn a_refused_call_frees_every_state_made_for_it_then_raises_the_first_drops_panic() {
    let freed = Freed::default();
    let mut executor =
        executor_fn(promise(), powering_down_badly(&freed, Some(2)), runs).shots(OnePerShot);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| {
        send(&mut executor, &[0, 1, 2], &[Op::Flip])
    }))
    .expect_err("the states' drops panic");
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(0)));
    assert_eq!(freed_so_far(&freed), [0, 1]);
}

/// A device whose states fail to power down ran a call on shot 1, then
/// one on shot 0: `close` frees both, in the order of their first calls,
/// then the first one's panic, shot 1's, reaches the closer, rather than
/// abort the process.
#[test]
fn close_frees_every_state_then_raises_the_first_drops_panic() {
    let freed = Freed::default();
    let mut executor =
        executor_fn(promise(), powering_down_badly(&freed, None), runs).shots(OnePerShot);
    for shot in [1, 0] {
        let (_, end) = send(&mut executor, &[shot], &[Op::Flip]).expect("an open executor runs");
        end.expect("the logic runs");
    }
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| executor.close()))
        .expect_err("the states' drops panic");
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(1)));
    assert_eq!(freed_so_far(&freed), [1, 0]);
}

/// A driver with two bugs: its log probability panics as `close` reads
/// the first of two states that fail to power down. The log probability's
/// panic reaches the closer, and the states, freed as it unwinds, leave
/// theirs to the panic hook, rather than abort the process.
#[test]
fn a_report_closures_panic_reaches_the_closer_though_the_states_drops_panic_too() {
    let freed = Freed::default();
    let mut executor = executor_fn(promise(), powering_down_badly(&freed, None), runs)
        .log_probability(|_: &PowersDownBadly| panic!("the log probability panics"))
        .shots(OnePerShot);
    let (_, end) = send(&mut executor, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| executor.close()))
        .expect_err("the log probability panics");
    assert_eq!(panic_text(&*panicked), Some("the log probability panics"));
    assert_eq!(freed_so_far(&freed), [0, 1]);
}

/// A device whose states fail to power down ran a call on shots 0 and 1,
/// and is dropped unclosed: its drop frees both and raises neither panic,
/// which only a close reports.
#[test]
fn dropped_it_frees_every_state_raising_no_panic_of_their_drops() {
    let freed = Freed::default();
    let mut executor =
        executor_fn(promise(), powering_down_badly(&freed, None), runs).shots(OnePerShot);
    let (_, end) = send(&mut executor, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    panic::catch_unwind(AssertUnwindSafe(|| drop(executor)))
        .expect("a drop raises no panic of the states' drops");
    assert_eq!(freed_so_far(&freed), [0, 1]);
}

/// A device whose state closure and logic each hold a handle on its
/// connection that fails to release (a driver's bug) ran a call on shots
/// 0 and 1, and is dropped unclosed: its drop releases both handles, each
/// in a catch of its own, and raises neither panic, as for its states'.
#[test]
fn dropped_it_frees_what_each_closure_captures_raising_no_panic_of_their_drops() {
    let (handle, released) = ReleasesBadly::new();
    let making = handle.another();
    let mut executor = executor_fn(
        promise(),
        move |context| {
            let _held = &making;
            Bit::new(context)
        },
        holding_a_handle(Arc::new(handle)),
    )
    .shots(OnePerShot);
    let (_, end) = send(&mut executor, &[0, 1], &[Op::Flip]).expect("an open executor runs a call");
    end.expect("the logic runs");
    panic::catch_unwind(AssertUnwindSafe(|| drop(executor)))
        .expect("a drop raises no panic of a capture's drop");
    assert_eq!(released.load(Ordering::SeqCst), 2);
}
