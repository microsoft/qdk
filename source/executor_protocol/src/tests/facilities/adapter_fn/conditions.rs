// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How a shot's conditions travel up a chain on its outcomes: a component
//! raises them on the outcomes it writes (`Outcome::with_conditions`,
//! `Outcomes::raise_conditions`); an `adapter_fn` merges those it reads
//! below, and those its logic raises, into each shot's conditions so far,
//! and marks with them every outcome it gives a shot of the call, with no
//! code in its logic. The tests of an `executor_fn` alone are in
//! `executor_fn/conditions.rs`, on this file's device of conditions.

use std::task::Waker;

use crate::reply::blocking::wait_status;
use crate::tests::shots;
use crate::{
    AdapterError, Below, BelowHandle, CallInput, Capabilities, Close, Describe, Executor,
    ImmediateReply, InstructionBatch, InstructionBatchRef, OnePerShot, Outcome, Outcomes, Rejected,
    Reply, ReplyStatus, ShotConditions, ShotContext, ShotId, ShotOutcome, TypedInstruction, Value,
    adapter_fn, compose, conformance, executor_fn, owning_adapter_fn, wait,
};

/// An instruction of the tests' device of conditions: it raises
/// conditions on one shot, reads a One, or is refused; and, for the
/// runtimes whose replies can end late or badly ([`AnswersAsTold`]), says
/// when and how its batch's reply ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tests::facilities) enum Step {
    /// Raise these conditions on this shot, if the state serves it.
    Raise(ShotId, ShotConditions),
    /// Read a One under this tag.
    Read(u32),
    /// An instruction the device refuses, as one it cannot run.
    Refused,
    /// The batch's reply ends after the call that sent it.
    AfterTheCall,
    /// The batch's reply, once it has read, ends as this says.
    Ends(Ending),
}

impl TypedInstruction for Step {}

/// The tests' device of conditions, a state per shot, which knows its
/// shot: it raises what a [`Step::Raise`] names for its shot on the
/// outcomes of the call, keeps nothing from call to call, and refuses a
/// batch with a [`Step::Refused`].
pub(in crate::tests::facilities) fn device()
-> impl Executor<InstructionBatch<Step>> + Close + Describe {
    executor_fn(
        Capabilities::default(),
        |context: ShotContext| Ok::<_, Rejected>(context.shot()),
        |shot: &mut ShotId, input: &CallInput<'_, Step>, outcomes: &mut Outcomes<'_>| {
            for &step in input.instructions() {
                match step {
                    Step::Raise(on, conditions) if on == *shot => {
                        outcomes.raise_conditions(conditions);
                    }
                    Step::Raise(..) | Step::Refused | Step::AfterTheCall | Step::Ends(_) => {}
                    Step::Read(tag) => outcomes.push(Outcome::new(tag, Value::One)),
                }
            }
            Ok(())
        },
    )
    .check(|input: &CallInput<'_, Step>| {
        match input
            .instructions()
            .iter()
            .position(|&s| s == Step::Refused)
        {
            Some(at) => Err(Rejected::Instruction {
                at,
                reason: "the device cannot run it".into(),
            }),
            None => Ok(()),
        }
    })
    .shots(OnePerShot)
}

/// The state of an adapter that keeps none.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature a state closure has"
)]
pub(in crate::tests::facilities) fn no_state(_: ShotContext) -> Result<(), Rejected> {
    Ok(())
}

/// The logic of an observer that sends each call down as it is.
fn pass(
    (): &mut (),
    input: &CallInput<'_, Step>,
    below: &mut Below<'_, Step>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    below.send(input.instructions())
}

/// `read` written anew by a decoder: each readout under its tag plus 100,
/// with no condition, as a decoder with no code for conditions makes them.
fn decoded(read: Vec<ShotOutcome>) -> impl Iterator<Item = ShotOutcome> {
    (read.into_iter()).map(|o| ShotOutcome::new(o.shot, o.tag + 100, o.value))
}

/// The logic of a decoder that runs each call below and writes, for each
/// readout it read, one of its own under the tag plus 100, made anew; it
/// has no code for conditions.
fn decode(
    (): &mut (),
    input: &CallInput<'_, Step>,
    below: &mut Below<'_, Step>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    let read = below.run(input.instructions())?;
    outcomes.extend(decoded(read));
    Ok(())
}

/// Each outcome of `outcomes` as its shot, its tag and its conditions.
pub(in crate::tests::facilities) fn marked(
    outcomes: &[ShotOutcome],
) -> Vec<(ShotId, u32, ShotConditions)> {
    (outcomes.iter())
        .map(|o| (o.shot, o.tag, o.conditions))
        .collect()
}

/// A One under each shot and tag of `listed`, carrying its conditions, in
/// that order: what [`marked`] gives back.
fn readouts(listed: &[(ShotId, u32, ShotConditions)]) -> Vec<ShotOutcome> {
    (listed.iter())
        .map(|&(shot, tag, conditions)| {
            ShotOutcome::new(shot, tag, Value::One).with_conditions(conditions)
        })
        .collect()
}

/// The outcomes of `batch` on `ids` through `executor`, which ends well.
pub(in crate::tests::facilities) fn outcomes_of(
    executor: &mut impl Executor<InstructionBatch<Step>>,
    ids: &[ShotId],
    batch: &[Step],
) -> Vec<ShotOutcome> {
    let reply = executor.call(InstructionBatchRef {
        shots: &shots(ids),
        instructions: batch,
    });
    let mut outcomes = Vec::new();
    wait(
        reply.expect("the executor accepts the batch"),
        &mut outcomes,
    )
    .expect("the call ends well");
    outcomes
}

/// A profiler over a code's runtime that rejects shot 2: the profiler's
/// caller sees shot 2's readout rejected, and only shot 2's, as it would
/// with no profiler.
#[test]
fn the_conditions_raised_below_reach_the_caller_through_an_adapter_that_sends() {
    let mut profiled = compose(adapter_fn(no_state, pass), device()).expect("steps fit steps");
    let batch = [Step::Raise(2, ShotConditions::REJECTED), Step::Read(0)];
    let outcomes = outcomes_of(&mut profiled, &[1, 2, 3], &batch);
    assert_eq!(
        marked(&outcomes),
        [
            (1, 0, ShotConditions::NONE),
            (2, 0, ShotConditions::REJECTED),
            (3, 0, ShotConditions::NONE)
        ]
    );
}

/// A decoder over a code's runtime that finds shot 1's syndromes
/// inconsistent: the decoder's logic writes its outcomes anew, with no
/// condition, and its caller sees shot 1's inconsistent.
#[test]
fn a_decoders_outcomes_carry_the_conditions_of_its_runs_with_no_code_in_its_logic() {
    let mut decoder = compose(adapter_fn(no_state, decode), device()).expect("steps fit steps");
    let batch = [Step::Read(0), Step::Raise(1, ShotConditions::INCONSISTENT)];
    let outcomes = outcomes_of(&mut decoder, &[1, 2], &batch);
    assert_eq!(
        marked(&outcomes),
        [
            (1, 100, ShotConditions::INCONSISTENT),
            (2, 100, ShotConditions::NONE)
        ]
    );
}

/// A decoder whose states own their way below (`owning_adapter_fn`), over
/// a code's runtime that finds shot 1's syndromes inconsistent: its
/// caller sees shot 1's outcome inconsistent, as with a lent way below.
#[test]
fn an_owning_decoders_outcomes_carry_the_conditions_of_its_runs() {
    let owning = owning_adapter_fn(
        |_: ShotContext, below| Ok::<_, Rejected>(below),
        |below: &mut BelowHandle<Step>,
         input: &CallInput<'_, Step>,
         outcomes: &mut Outcomes<'_>| {
            let read = below.run(input.instructions())?;
            outcomes.extend(decoded(read));
            Ok(())
        },
    );
    let mut decoder = compose(owning, device()).expect("steps fit steps");
    let batch = [Step::Read(0), Step::Raise(1, ShotConditions::INCONSISTENT)];
    let outcomes = outcomes_of(&mut decoder, &[1, 2], &batch);
    assert_eq!(
        marked(&outcomes),
        [
            (1, 100, ShotConditions::INCONSISTENT),
            (2, 100, ShotConditions::NONE)
        ]
    );
}

/// A decoder with a state per shot, each owning its way below
/// (`owning_adapter_fn` with `OnePerShot`), over a code's runtime that
/// finds shot 1's syndromes inconsistent: its caller sees shot 1's outcome
/// inconsistent, and shot 2's clear, as with one state.
#[test]
fn with_a_state_per_shot_an_owning_decoders_outcomes_carry_the_conditions_of_its_runs() {
    let owning = owning_adapter_fn(
        |_: ShotContext, below| Ok::<_, Rejected>(below),
        |below: &mut BelowHandle<Step>,
         input: &CallInput<'_, Step>,
         outcomes: &mut Outcomes<'_>| {
            let read = below.run(input.instructions())?;
            outcomes.extend(decoded(read));
            Ok(())
        },
    )
    .shots(OnePerShot);
    let mut decoder = compose(owning, device()).expect("steps fit steps");
    let batch = [Step::Read(0), Step::Raise(1, ShotConditions::INCONSISTENT)];
    let outcomes = outcomes_of(&mut decoder, &[1, 2], &batch);
    assert_eq!(
        marked(&outcomes),
        [
            (1, 100, ShotConditions::INCONSISTENT),
            (2, 100, ShotConditions::NONE)
        ]
    );
}

/// The logic of a decoder, one state for all its shots, that finds a call
/// of syndrome extraction alone (no instructions) unresolved, and
/// otherwise decodes as [`decode`] does.
fn unresolves_extraction(
    (): &mut (),
    input: &CallInput<'_, Step>,
    below: &mut Below<'_, Step>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    if input.instructions().is_empty() {
        outcomes.raise_conditions(ShotConditions::UNRESOLVED);
    }
    decode(&mut (), input, below, outcomes)
}

/// A decoder runs a call of syndrome extraction alone, which reads
/// nothing, and finds shot 3 unresolved; its next call reads shot 3: the
/// readout carries the condition, which no outcome could carry before.
#[test]
fn a_condition_raised_in_a_call_with_no_readouts_marks_the_shots_outcomes_in_later_calls() {
    let mut decoder =
        compose(adapter_fn(no_state, unresolves_extraction), device()).expect("steps fit steps");
    let none = outcomes_of(&mut decoder, &[3], &[]);
    assert!(none.is_empty(), "the extraction reads nothing");
    let outcomes = outcomes_of(&mut decoder, &[3], &[Step::Read(0)]);
    assert_eq!(marked(&outcomes), [(3, 100, ShotConditions::UNRESOLVED)]);
}

/// A batch decoder, one state for all its shots, runs a call of syndrome
/// extraction on shots 1 to 3, which reads nothing, and finds the batch
/// unresolved; its next call reads the three: each shot's readout carries
/// the condition, as a logic with one state raises on every shot of the
/// call.
#[test]
fn a_condition_a_logic_with_one_state_raises_marks_every_shot_of_the_call() {
    let mut decoder =
        compose(adapter_fn(no_state, unresolves_extraction), device()).expect("steps fit steps");
    let none = outcomes_of(&mut decoder, &[1, 2, 3], &[]);
    assert!(none.is_empty(), "the extraction reads nothing");
    let outcomes = outcomes_of(&mut decoder, &[1, 2, 3], &[Step::Read(0)]);
    assert_eq!(
        marked(&outcomes),
        [
            (1, 100, ShotConditions::UNRESOLVED),
            (2, 100, ShotConditions::UNRESOLVED),
            (3, 100, ShotConditions::UNRESOLVED)
        ]
    );
}

/// A batch decoder, one state for all its shots, writes one result for a
/// call on shots 1 and 2 with the rejection it found on the batch: each
/// shot's result carries it, as `push` writes an outcome, with its
/// conditions, for every shot the state serves.
#[test]
fn the_conditions_a_logic_with_one_state_writes_on_an_outcome_mark_every_shot_of_the_call() {
    fn rejects_the_batch(
        (): &mut (),
        _: &CallInput<'_, Step>,
        _: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        outcomes.push(Outcome::new(0, Value::One).with_conditions(ShotConditions::REJECTED));
        Ok(())
    }
    let mut decoder =
        compose(adapter_fn(no_state, rejects_the_batch), device()).expect("steps fit steps");
    let outcomes = outcomes_of(&mut decoder, &[1, 2], &[]);
    assert_eq!(
        marked(&outcomes),
        [
            (1, 0, ShotConditions::REJECTED),
            (2, 0, ShotConditions::REJECTED)
        ]
    );
}

/// A batch decoder, one state for all its shots, writes a result of its
/// own for each of shots 1 and 2, the first with the inconsistency it
/// found on shot 1: shot 2's result carries none, as `extend` with
/// `ShotOutcome`s writes each for its own shot alone.
#[test]
fn the_conditions_a_logic_writes_on_a_shots_outcome_mark_that_shot_alone() {
    fn finds_shot_1_inconsistent(
        (): &mut (),
        _: &CallInput<'_, Step>,
        _: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        let inconsistent = ShotOutcome::new(1, 0, Value::One);
        outcomes.extend([
            inconsistent.with_conditions(ShotConditions::INCONSISTENT),
            ShotOutcome::new(2, 0, Value::One),
        ]);
        Ok(())
    }
    let mut decoder = compose(adapter_fn(no_state, finds_shot_1_inconsistent), device())
        .expect("steps fit steps");
    let outcomes = outcomes_of(&mut decoder, &[1, 2], &[]);
    assert_eq!(
        marked(&outcomes),
        [
            (1, 0, ShotConditions::INCONSISTENT),
            (2, 0, ShotConditions::NONE)
        ]
    );
}

/// A batch decoder, one state for all its shots, runs a call of syndrome
/// extraction on shots 1 and 2, which reads nothing, and finds the batch
/// rejected, then unresolved; its next call reads both: each readout
/// carries both conditions, as an `adapter_fn` takes in the union of what
/// its logic raised in the call.
#[test]
fn the_conditions_a_logic_raises_twice_in_a_call_with_no_readouts_both_mark_later_outcomes() {
    fn rejects_then_unresolves_extraction(
        (): &mut (),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if input.instructions().is_empty() {
            outcomes.raise_conditions(ShotConditions::REJECTED);
            outcomes.raise_conditions(ShotConditions::UNRESOLVED);
        }
        decode(&mut (), input, below, outcomes)
    }
    let mut decoder = compose(
        adapter_fn(no_state, rejects_then_unresolves_extraction),
        device(),
    )
    .expect("steps fit steps");
    let none = outcomes_of(&mut decoder, &[1, 2], &[]);
    assert!(none.is_empty(), "the extraction reads nothing");
    let outcomes = outcomes_of(&mut decoder, &[1, 2], &[Step::Read(0)]);
    let both = ShotConditions::REJECTED | ShotConditions::UNRESOLVED;
    assert_eq!(marked(&outcomes), [(1, 100, both), (2, 100, both)]);
}

/// A code's runtime, which keeps no conditions from call to call, finds
/// shot 3 unresolved in a call that reads nothing: no outcome carries the
/// condition, then or later, as a condition travels on outcomes alone.
#[test]
fn a_condition_raised_below_in_a_call_with_no_readouts_reaches_no_caller() {
    let mut decoder = compose(adapter_fn(no_state, decode), device()).expect("steps fit steps");
    let extraction = [Step::Raise(3, ShotConditions::UNRESOLVED)];
    let none = outcomes_of(&mut decoder, &[3], &extraction);
    assert!(none.is_empty(), "the extraction reads nothing");
    let outcomes = outcomes_of(&mut decoder, &[3], &[Step::Read(0)]);
    assert_eq!(marked(&outcomes), [(3, 100, ShotConditions::NONE)]);
}

/// A code's layer writes two results of a call, and marks the second with
/// its runtime's status; it writes a later call's with none of its own:
/// the later results carry the earlier status, as once raised a condition
/// stays raised.
#[test]
fn the_conditions_a_logic_writes_on_an_outcome_mark_the_shots_outcomes_in_later_calls() {
    fn rejects_first(
        calls: &mut u32,
        _: &CallInput<'_, Step>,
        _: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        let conditions = if *calls == 0 {
            ShotConditions::REJECTED
        } else {
            ShotConditions::NONE
        };
        *calls += 1;
        outcomes.push(Outcome::new(0, Value::One));
        outcomes.push(Outcome::new(1, Value::One).with_conditions(conditions));
        Ok(())
    }
    let mut layer =
        compose(adapter_fn(|_| Ok(0), rejects_first), device()).expect("steps fit steps");
    let first = outcomes_of(&mut layer, &[1], &[]);
    assert!(
        (first.iter()).any(|o| o.conditions == ShotConditions::REJECTED),
        "the first call raises it"
    );
    let second = outcomes_of(&mut layer, &[1], &[]);
    assert_eq!(
        marked(&second),
        [
            (1, 0, ShotConditions::REJECTED),
            (1, 1, ShotConditions::REJECTED)
        ]
    );
}

/// A code's layer, one state for all its shots, writes results of shots 1
/// and 2 in one call, and marks some with its runtime's status, rejected
/// or unresolved: three of shot 1, only the middle one marked; or each
/// shot's two in a row, shot 1's second marked and shot 2's first; or the
/// shots' interleaved, shot 1's first marked, or its second, with shot 2's
/// first or not. Each result carries its own shot's conditions in that
/// call, those on the results before it and after it, in a row or not,
/// as a condition is the shot's, not the readout's.
#[test]
fn the_conditions_a_logic_writes_on_an_outcome_mark_the_shots_other_outcomes_in_the_call() {
    #[expect(clippy::ptr_arg, reason = "the signature a logic has for its state")]
    fn writes_as_listed(
        listed: &mut Vec<ShotOutcome>,
        _: &CallInput<'_, Step>,
        _: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        outcomes.extend(listed.iter().copied());
        Ok(())
    }
    let (rejected, unresolved, none) = (
        ShotConditions::REJECTED,
        ShotConditions::UNRESOLVED,
        ShotConditions::NONE,
    );
    let cases = [
        (
            vec![(1, 0, none), (1, 1, rejected), (1, 2, none)],
            vec![(1, 0, rejected), (1, 1, rejected), (1, 2, rejected)],
        ),
        (
            vec![
                (1, 5, none),
                (1, 6, rejected),
                (2, 5, unresolved),
                (2, 6, none),
            ],
            vec![
                (1, 5, rejected),
                (1, 6, rejected),
                (2, 5, unresolved),
                (2, 6, unresolved),
            ],
        ),
        (
            vec![(1, 5, rejected), (2, 5, none), (1, 6, none), (2, 6, none)],
            vec![
                (1, 5, rejected),
                (2, 5, none),
                (1, 6, rejected),
                (2, 6, none),
            ],
        ),
        (
            vec![(1, 5, none), (2, 5, none), (1, 6, rejected), (2, 6, none)],
            vec![
                (1, 5, rejected),
                (2, 5, none),
                (1, 6, rejected),
                (2, 6, none),
            ],
        ),
        (
            vec![
                (1, 5, none),
                (2, 5, unresolved),
                (1, 6, rejected),
                (2, 6, none),
            ],
            vec![
                (1, 5, rejected),
                (2, 5, unresolved),
                (1, 6, rejected),
                (2, 6, unresolved),
            ],
        ),
    ];
    for (written, expected) in cases {
        let listed = readouts(&written);
        let mut layer = compose(
            adapter_fn(move |_| Ok(listed.clone()), writes_as_listed),
            device(),
        )
        .expect("steps fit steps");
        let outcomes = outcomes_of(&mut layer, &[1, 2], &[]);
        assert_eq!(marked(&outcomes), expected, "written {written:?}");
    }
}

/// A layer sends a call on shots 1 and 2 below, where the code's runtime
/// rejects shot 1, then writes a result of its own for each shot: in that
/// call, shot 1's result carries the rejection its send read, and shot
/// 2's carries none.
#[test]
fn a_condition_a_send_reads_marks_the_outcomes_the_logic_writes_for_the_shot_in_the_call() {
    fn sends_then_writes(
        (): &mut (),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        below.send(input.instructions())?;
        outcomes.push(Outcome::new(50, Value::One));
        Ok(())
    }
    let mut layer =
        compose(adapter_fn(no_state, sends_then_writes), device()).expect("steps fit steps");
    let batch = [Step::Raise(1, ShotConditions::REJECTED), Step::Read(0)];
    let mut outcomes = marked(&outcomes_of(&mut layer, &[1, 2], &batch));
    outcomes.sort_by_key(|&(shot, tag, _)| (shot, tag));
    assert_eq!(
        outcomes,
        [
            (1, 0, ShotConditions::REJECTED),
            (1, 50, ShotConditions::REJECTED),
            (2, 0, ShotConditions::NONE),
            (2, 50, ShotConditions::NONE)
        ]
    );
}

/// A layer rejects shot 1 and sends the call below, where the code's
/// runtime finds the shot's syndromes inconsistent: the readout passed up
/// carries both, the layer's and the one from below.
#[test]
fn a_condition_a_logic_raises_and_one_from_below_on_the_same_shot_merge_by_union() {
    let mut layer =
        compose(adapter_fn(no_state, pass_rejecting), device()).expect("steps fit steps");
    let batch = [Step::Raise(1, ShotConditions::INCONSISTENT), Step::Read(0)];
    let outcomes = outcomes_of(&mut layer, &[1], &batch);
    let both = ShotConditions::REJECTED | ShotConditions::INCONSISTENT;
    assert_eq!(marked(&outcomes), [(1, 0, both)]);
}

/// A layer rejects shot 1 and sends the call below, where the code's
/// runtime finds the shot's syndromes inconsistent; in the next call, with
/// nothing to send, it writes a result of its own for the shot: that
/// result carries both, the layer's and the one from below, as the
/// adapter keeps their union for later calls.
#[test]
fn a_condition_a_logic_raises_and_one_from_below_on_the_same_shot_both_mark_the_shots_outcomes_in_later_calls()
 {
    fn rejects_and_sends_or_writes(
        (): &mut (),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if input.instructions().is_empty() {
            outcomes.push(Outcome::new(5, Value::One));
            return Ok(());
        }
        pass_rejecting(&mut (), input, below, outcomes)
    }
    let mut layer = compose(adapter_fn(no_state, rejects_and_sends_or_writes), device())
        .expect("steps fit steps");
    let batch = [Step::Raise(1, ShotConditions::INCONSISTENT), Step::Read(0)];
    let both = ShotConditions::REJECTED | ShotConditions::INCONSISTENT;
    let first = outcomes_of(&mut layer, &[1], &batch);
    assert_eq!(
        marked(&first),
        [(1, 0, both)],
        "the send's readout carries both"
    );
    let outcomes = outcomes_of(&mut layer, &[1], &[]);
    assert_eq!(marked(&outcomes), [(1, 5, both)]);
}

/// A decoder given its shots in descending order, 9, 6 and 2, as a
/// driver given them so sends them, over a code's runtime that raises a
/// different condition on each: each shot's readout carries its own.
#[test]
fn a_decoders_outcomes_carry_their_own_shots_conditions_whatever_the_order_of_its_shots() {
    let mut decoder = compose(adapter_fn(no_state, decode), device()).expect("steps fit steps");
    let batch = [
        Step::Raise(9, ShotConditions::REJECTED),
        Step::Raise(6, ShotConditions::INCONSISTENT),
        Step::Raise(2, ShotConditions::UNRESOLVED),
        Step::Read(0),
    ];
    let mut outcomes = marked(&outcomes_of(&mut decoder, &[9, 6, 2], &batch));
    outcomes.sort_by_key(|&(shot, ..)| shot);
    assert_eq!(
        outcomes,
        [
            (2, 100, ShotConditions::UNRESOLVED),
            (6, 100, ShotConditions::INCONSISTENT),
            (9, 100, ShotConditions::REJECTED)
        ]
    );
}

/// An observer over a code's runtime that rejects shot 1 and leaves shot
/// 3 unresolved in a call on shots 1 to 3; its next call reads two
/// readouts of each shot, as a runtime with several readouts per shot
/// does: each readout carries its own shot's conditions, both of shot 1's
/// and both of shot 3's, and shot 2's none.
#[test]
fn with_several_shots_flagged_each_outcome_of_a_shot_carries_its_conditions() {
    let mut observed = compose(adapter_fn(no_state, pass), device()).expect("steps fit steps");
    let batch = [
        Step::Raise(1, ShotConditions::REJECTED),
        Step::Raise(3, ShotConditions::UNRESOLVED),
        Step::Read(0),
    ];
    let first = outcomes_of(&mut observed, &[1, 2, 3], &batch);
    assert_eq!(
        marked(&first),
        [
            (1, 0, ShotConditions::REJECTED),
            (2, 0, ShotConditions::NONE),
            (3, 0, ShotConditions::UNRESOLVED)
        ],
        "the first call flags shots 1 and 3"
    );
    let outcomes = outcomes_of(&mut observed, &[1, 2, 3], &[Step::Read(1), Step::Read(2)]);
    assert_eq!(
        marked(&outcomes),
        [
            (1, 1, ShotConditions::REJECTED),
            (1, 2, ShotConditions::REJECTED),
            (2, 1, ShotConditions::NONE),
            (2, 2, ShotConditions::NONE),
            (3, 1, ShotConditions::UNRESOLVED),
            (3, 2, ShotConditions::UNRESOLVED)
        ]
    );
}

/// A layer, one state for all its shots, sends a call on shots 1 to 3
/// below, where the code's runtime rejects shot 1, and in a second case
/// also leaves shot 3 unresolved; its next call writes two results of
/// each shot, under tags 5 and 6, by tag, as `Outcomes::push` writes them
/// (shots 1, 2, 3, then 1, 2, 3 again), or by shot: each result of a
/// flagged shot carries its shot's conditions, its first and its second,
/// whatever their order, and shot 2's none.
#[test]
fn each_outcome_a_logic_writes_carries_its_shots_conditions_from_earlier_calls_whatever_their_order()
 {
    fn sends_or_writes_two(
        by_shot: &mut bool,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if !input.instructions().is_empty() {
            return below.send(input.instructions());
        }
        if *by_shot {
            let by_shot = [1, 2, 3].map(|shot| [5, 6].map(|tag| (shot, tag, ShotConditions::NONE)));
            outcomes.extend(readouts(by_shot.as_flattened()));
        } else {
            outcomes.push(Outcome::new(5, Value::One));
            outcomes.push(Outcome::new(6, Value::One));
        }
        Ok(())
    }
    let (rejected, unresolved, none) = (
        ShotConditions::REJECTED,
        ShotConditions::UNRESOLVED,
        ShotConditions::NONE,
    );
    for flagged in [vec![(1, rejected)], vec![(1, rejected), (3, unresolved)]] {
        for by_shot in [false, true] {
            let mut layer = compose(
                adapter_fn(move |_| Ok(by_shot), sends_or_writes_two),
                device(),
            )
            .expect("steps fit steps");
            let mut batch: Vec<_> = (flagged.iter())
                .map(|&(shot, conditions)| Step::Raise(shot, conditions))
                .collect();
            batch.push(Step::Read(0));
            outcomes_of(&mut layer, &[1, 2, 3], &batch);
            let mut outcomes = marked(&outcomes_of(&mut layer, &[1, 2, 3], &[]));
            outcomes.sort_by_key(|&(shot, tag, _)| (shot, tag));
            let of = |shot| {
                (flagged.iter())
                    .find(|&&(on, _)| on == shot)
                    .map_or(none, |&(_, conditions)| conditions)
            };
            assert_eq!(
                outcomes,
                [1, 2, 3]
                    .map(|shot| [5, 6].map(|tag| (shot, tag, of(shot))))
                    .as_flattened(),
                "{flagged:?}, by shot: {by_shot}"
            );
        }
    }
}

/// An observer sends a call on shots 1 and 2 below, to a code's runtime
/// whose reply gives, in the call, readouts of the shots in an order of
/// its own and marks some rejected or unresolved, then ends well, failed
/// or cancelled: a shot's two readouts, one marked, the first or the
/// second; or the shots' interleaved, shot 1's first marked, or its
/// second, with shot 2's first or not; or each shot's two in a row, shot
/// 1's second marked and shot 2's first. However it ends, each readout
/// passed up carries its own shot's conditions, those on the readouts
/// before it and after it, in a row or not, as well as its own.
#[test]
fn the_conditions_a_send_reads_in_the_call_on_an_outcome_mark_the_shots_other_outcomes_whatever_their_order()
 {
    let (rejected, unresolved, none) = (
        ShotConditions::REJECTED,
        ShotConditions::UNRESOLVED,
        ShotConditions::NONE,
    );
    let cases = [
        (
            vec![(1, 0, none), (1, 1, rejected)],
            vec![(1, 0, rejected), (1, 1, rejected)],
        ),
        (
            vec![(1, 0, rejected), (1, 1, none)],
            vec![(1, 0, rejected), (1, 1, rejected)],
        ),
        (
            vec![(1, 0, rejected), (2, 0, none), (1, 1, none)],
            vec![(1, 0, rejected), (2, 0, none), (1, 1, rejected)],
        ),
        (
            vec![(1, 0, none), (2, 0, none), (1, 1, rejected)],
            vec![(1, 0, rejected), (2, 0, none), (1, 1, rejected)],
        ),
        (
            vec![
                (1, 0, rejected),
                (2, 0, unresolved),
                (1, 1, none),
                (2, 1, none),
            ],
            vec![
                (1, 0, rejected),
                (2, 0, unresolved),
                (1, 1, rejected),
                (2, 1, unresolved),
            ],
        ),
        (
            vec![
                (1, 0, none),
                (1, 1, rejected),
                (2, 0, unresolved),
                (2, 1, none),
            ],
            vec![
                (1, 0, rejected),
                (1, 1, rejected),
                (2, 0, unresolved),
                (2, 1, unresolved),
            ],
        ),
    ];
    for (read, expected) in cases {
        for ending in [None, Some(Ending::Failed), Some(Ending::Cancelled)] {
            let below = ReadsAsListed {
                read: readouts(&read),
                after_the_call: false,
                ending,
            };
            let mut observed = compose(adapter_fn(no_state, pass), below).expect("steps fit steps");
            let batch = [Step::Read(0), Step::Read(1)];
            assert_eq!(
                handed_over_as_the_send_ends(&mut observed, &[1, 2], &batch, ending),
                expected,
                "read {read:?}, {ending:?}"
            );
        }
    }
}

/// A decoder runs below twice in one call, on shots 1 to 4, and writes
/// anew the readouts of its second run alone: the first run rejects shots
/// 1 and 3, the second leaves shots 2 and 3 unresolved. Each shot's
/// readout carries the union of its own (shot 3 both), shots split across
/// the runs not contiguously, and shot 4's none.
#[test]
fn the_conditions_of_several_runs_merge_by_union_shot_by_shot() {
    fn twice(
        (): &mut (),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        let (first, second) = input.instructions().split_at(3);
        below.run(first)?;
        let read = below.run(second)?;
        outcomes.extend((read.into_iter()).map(|o| ShotOutcome::new(o.shot, o.tag, o.value)));
        Ok(())
    }
    let mut decoder = compose(adapter_fn(no_state, twice), device()).expect("steps fit steps");
    let batch = [
        Step::Raise(1, ShotConditions::REJECTED),
        Step::Raise(3, ShotConditions::REJECTED),
        Step::Read(0),
        Step::Raise(2, ShotConditions::UNRESOLVED),
        Step::Raise(3, ShotConditions::UNRESOLVED),
        Step::Read(1),
    ];
    let outcomes = outcomes_of(&mut decoder, &[1, 2, 3, 4], &batch);
    let both = ShotConditions::REJECTED | ShotConditions::UNRESOLVED;
    assert_eq!(
        marked(&outcomes),
        [
            (1, 1, ShotConditions::REJECTED),
            (2, 1, ShotConditions::UNRESOLVED),
            (3, 1, both),
            (4, 1, ShotConditions::NONE)
        ]
    );
}

/// The logic of a layer that sends each call down as it is, and finds
/// every shot rejected.
fn pass_rejecting(
    (): &mut (),
    input: &CallInput<'_, Step>,
    below: &mut Below<'_, Step>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    outcomes.raise_conditions(ShotConditions::REJECTED);
    below.send(input.instructions())
}

/// A layer that rejects shot 1 sends a call below, whose reply ends in
/// the call: the readout it passes up carries the rejection.
#[test]
fn a_condition_a_logic_raises_marks_the_outcomes_of_its_sends() {
    let mut layer =
        compose(adapter_fn(no_state, pass_rejecting), device()).expect("steps fit steps");
    let outcomes = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(marked(&outcomes), [(1, 0, ShotConditions::REJECTED)]);
}

/// A layer rejects shot 1 in a call whose send the device refuses: the
/// call is refused, as if it never came, and raises nothing, so that the
/// shot's readout in the next call carries no rejection.
#[test]
fn a_refused_call_raises_nothing() {
    fn rejects_the_refused(
        (): &mut (),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if input.instructions().contains(&Step::Refused) {
            outcomes.raise_conditions(ShotConditions::REJECTED);
        }
        below.send(input.instructions())
    }
    let mut layer =
        compose(adapter_fn(no_state, rejects_the_refused), device()).expect("steps fit steps");
    let refused = layer.call(InstructionBatchRef {
        shots: &shots(&[1]),
        instructions: &[Step::Read(0), Step::Refused],
    });
    assert!(
        matches!(refused, Err(Rejected::Instruction { at: 1, .. })),
        "the device refuses the call"
    );
    let outcomes = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(marked(&outcomes), [(1, 0, ShotConditions::NONE)]);
}

/// A layer with a state per shot counts the calls it sent below for its
/// shot, after the send, as a logic changes its state only after
/// `send(..)?`, writes the count as a result of its own, and rejects the
/// shot once, as the count reaches two. The second call is refused below,
/// where the shots' sends go merged after every logic ran: the count it
/// reached is kept, as the later call's result shows, but the rejection
/// it raised is dropped, so that result carries none.
#[test]
fn with_a_state_per_shot_a_call_refused_below_raises_none_of_the_conditions_its_logic_raised() {
    fn counts_and_rejects_at_two(
        count: &mut u32,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        below.send(input.instructions())?;
        *count += 1;
        if *count == 2 {
            outcomes.raise_conditions(ShotConditions::REJECTED);
        }
        outcomes.push(Outcome::new(100 + *count, Value::One));
        Ok(())
    }
    let per_shot = adapter_fn(|_: ShotContext| Ok(0), counts_and_rejects_at_two).shots(OnePerShot);
    let mut layer = compose(per_shot, device()).expect("steps fit steps");
    let first = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&first),
        [(1, 101, ShotConditions::NONE), (1, 0, ShotConditions::NONE)],
        "the first call counts one"
    );
    let refused = layer.call(InstructionBatchRef {
        shots: &shots(&[1]),
        instructions: &[Step::Read(0), Step::Refused],
    });
    assert!(
        matches!(refused, Err(Rejected::Instruction { at: 1, .. })),
        "the device refuses the merged send"
    );
    let outcomes = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&outcomes),
        [(1, 103, ShotConditions::NONE), (1, 0, ShotConditions::NONE)]
    );
}

/// A layer with a state per shot counts, for shot 1, the calls it sent
/// below, after the send, writes the count as a result of its own, and
/// rejects the shot as the count reaches two; for shot 2 it runs the call
/// below. In the second call, shot 2's run is refused below, after shot
/// 1's logic counted and rejected: the call is refused, shot 1's count is
/// kept, as the later call's result shows, but its rejection is dropped,
/// so that result carries none.
#[test]
fn with_a_state_per_shot_a_call_refused_at_a_later_shots_run_raises_none_of_the_conditions_its_logic_raised()
 {
    fn counts_shot_1_and_runs_shot_2(
        (shot, count): &mut (ShotId, u32),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if *shot == 2 {
            return below.run(input.instructions()).map(drop);
        }
        below.send(&[Step::Read(0)])?;
        *count += 1;
        if *count == 2 {
            outcomes.raise_conditions(ShotConditions::REJECTED);
        }
        outcomes.push(Outcome::new(100 + *count, Value::One));
        Ok(())
    }
    let per_shot = adapter_fn(
        |context: ShotContext| Ok((context.shot(), 0)),
        counts_shot_1_and_runs_shot_2,
    )
    .shots(OnePerShot);
    let mut layer = compose(per_shot, device()).expect("steps fit steps");
    let first = outcomes_of(&mut layer, &[1, 2], &[]);
    assert_eq!(
        marked(&first),
        [(1, 101, ShotConditions::NONE), (1, 0, ShotConditions::NONE)],
        "the first call counts one"
    );
    let refused = layer.call(InstructionBatchRef {
        shots: &shots(&[1, 2]),
        instructions: &[Step::Refused],
    });
    assert!(
        matches!(refused, Err(Rejected::Instruction { at: 0, .. })),
        "the device refuses shot 2's run"
    );
    let outcomes = outcomes_of(&mut layer, &[1], &[]);
    assert_eq!(
        marked(&outcomes),
        [(1, 103, ShotConditions::NONE), (1, 0, ShotConditions::NONE)]
    );
}

/// A layer with a state per shot sends each call below for its shot,
/// which it keeps until the shot's run, counts the calls it sent, after
/// the send, writes the count as a result of its own, rejects the shot as
/// the count reaches two, then runs a round below. In the second call,
/// the send, going down before the run, is refused below: the call is
/// refused at its shot's own run, the count is kept, as the later call's
/// result shows, but the rejection is dropped, so that result carries
/// none.
#[test]
fn with_a_state_per_shot_a_call_refused_at_its_shots_own_run_raises_none_of_the_conditions_its_logic_raised()
 {
    fn counts_rejects_at_two_then_runs(
        count: &mut u32,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        below.send(input.instructions())?;
        *count += 1;
        if *count == 2 {
            outcomes.raise_conditions(ShotConditions::REJECTED);
        }
        outcomes.push(Outcome::new(100 + *count, Value::One));
        below.run(&[]).map(drop)
    }
    let per_shot =
        adapter_fn(|_: ShotContext| Ok(0), counts_rejects_at_two_then_runs).shots(OnePerShot);
    let mut layer = compose(per_shot, device()).expect("steps fit steps");
    let first = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&first),
        [(1, 101, ShotConditions::NONE), (1, 0, ShotConditions::NONE)],
        "the first call counts one"
    );
    let refused = layer.call(InstructionBatchRef {
        shots: &shots(&[1]),
        instructions: &[Step::Read(0), Step::Refused],
    });
    assert!(
        matches!(refused, Err(Rejected::Instruction { at: 1, .. })),
        "the device refuses the send going down before the shot's run"
    );
    let outcomes = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&outcomes),
        [(1, 103, ShotConditions::NONE), (1, 0, ShotConditions::NONE)]
    );
}

/// An executor below whose reply waits through some polls, then reads a
/// One under tag 0 on each shot of the call when its batch holds a
/// [`Step::Read`], and nothing otherwise, as a batch of gates alone would,
/// raising on what it reads the conditions it holds: in its first call
/// alone, as an executor that keeps no condition from one call to the
/// next, though it offers `resumable_shots`; or, once
/// [`keeping`](Self::keeping) them, in every call, as one that keeps them.
struct ReadsLater {
    /// How many polls its reply waits through.
    waits: u8,
    /// The conditions its first call raises, and its later calls too when
    /// it keeps them.
    conditions: ShotConditions,
    /// Whether it raises them again in each later call.
    keeps: bool,
}

impl ReadsLater {
    /// One whose reply waits through the two polls an adapter makes during
    /// a send's call (at the send, and as the call ends), so that it ends
    /// after the call, as one on another thread may.
    const fn after_the_call(conditions: ShotConditions) -> Self {
        Self {
            waits: 2,
            conditions,
            keeps: false,
        }
    }

    /// One whose reply waits through the poll an adapter makes at a send,
    /// and ends at the one it makes as the call ends, as one on another
    /// thread that answers before the logic returns.
    const fn as_the_call_ends(conditions: ShotConditions) -> Self {
        Self {
            waits: 1,
            conditions,
            keeps: false,
        }
    }

    /// One whose reply ends at the poll an adapter makes at a send, as one
    /// that answers at once.
    const fn at_the_send(conditions: ShotConditions) -> Self {
        Self {
            waits: 0,
            conditions,
            keeps: false,
        }
    }

    /// This one, raising its conditions again in each call, as an
    /// executor that offers `resumable_shots` must keep them.
    const fn keeping(self) -> Self {
        Self {
            keeps: true,
            ..self
        }
    }
}

/// [`ReadsLater`]'s reply: how many more polls it waits through, the
/// shots it reads, and the conditions its readouts carry.
struct ReadsLaterReply(u8, Vec<ShotId>, ShotConditions);

impl Reply for ReadsLaterReply {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        if self.0 > 0 {
            self.0 -= 1;
            waker.wake_by_ref();
            return ReplyStatus::Pending;
        }
        let conditions = self.2;
        out.extend(
            (self.1.iter())
                .map(|&shot| ShotOutcome::new(shot, 0, Value::One).with_conditions(conditions)),
        );
        ReplyStatus::Done
    }
}

impl Executor<InstructionBatch<Step>> for ReadsLater {
    type Reply = ReadsLaterReply;

    fn call(&mut self, batch: InstructionBatchRef<'_, Step>) -> Result<Self::Reply, Rejected> {
        let conditions = if self.keeps {
            self.conditions
        } else {
            std::mem::take(&mut self.conditions)
        };
        let reads = (batch.instructions.iter()).any(|step| matches!(step, Step::Read(_)));
        let read = if reads {
            batch.shots.to_vec()
        } else {
            Vec::new()
        };
        Ok(ReadsLaterReply(self.waits, read, conditions))
    }
}

impl Describe for ReadsLater {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default().with_resumable_shots(true)
    }
}

impl Close for ReadsLater {
    type Report = ();

    fn close(&mut self) {}
}

/// A layer with a state per shot, which knows its shot, rejects shot 2
/// and sends a call on shots 1 and 2 below, to a runtime whose reply ends
/// after the call and finds both shots inconsistent: only shot 2's
/// readout, passed up then, carries the rejection, beside the condition
/// from below.
#[test]
fn a_condition_a_logic_raises_marks_its_shots_outcomes_of_its_sends_ending_later() {
    fn pass_rejecting_shot_2(
        shot: &mut ShotId,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if *shot == 2 {
            outcomes.raise_conditions(ShotConditions::REJECTED);
        }
        below.send(input.instructions())
    }
    let per_shot = adapter_fn(
        |context: ShotContext| Ok(context.shot()),
        pass_rejecting_shot_2,
    )
    .shots(OnePerShot);
    let below = ReadsLater::after_the_call(ShotConditions::INCONSISTENT);
    let mut layer = compose(per_shot, below).expect("steps fit steps");
    let outcomes = outcomes_of(&mut layer, &[1, 2], &[Step::Read(0)]);
    let both = ShotConditions::REJECTED | ShotConditions::INCONSISTENT;
    assert_eq!(
        marked(&outcomes),
        [(1, 0, ShotConditions::INCONSISTENT), (2, 0, both)]
    );
}

/// A layer, one state for all its shots, rejects shots 1 and 2 and sends
/// the call below, to a runtime whose reply ends after the call and finds
/// both shots inconsistent: each readout, passed up then, carries both,
/// the layer's and the one from below.
#[test]
fn a_condition_a_logic_raises_and_one_from_below_merge_by_union_on_its_sends_ending_later() {
    let below = ReadsLater::after_the_call(ShotConditions::INCONSISTENT);
    let mut layer = compose(adapter_fn(no_state, pass_rejecting), below).expect("steps fit steps");
    let outcomes = outcomes_of(&mut layer, &[1, 2], &[Step::Read(0)]);
    let both = ShotConditions::REJECTED | ShotConditions::INCONSISTENT;
    assert_eq!(marked(&outcomes), [(1, 0, both), (2, 0, both)]);
}

/// A layer, one state for all its shots, rejects shots 1 and 2 and sends
/// the call below, to a runtime with a thread per shot whose threads start
/// only after the call returned. Its caller polls the reply once shot 1's
/// thread has run, and gets shot 1's readout while shot 2's is pending,
/// then waits for the rest: both readouts carry the rejection, the one
/// handed over early as well.
#[test]
fn a_sends_outcomes_handed_over_while_its_reply_is_pending_carry_the_shots_conditions() {
    fn reads_a_one(
        (): &mut (),
        _: &CallInput<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), String> {
        outcomes.push(Outcome::new(0, Value::One));
        Ok(())
    }
    let (placement, jobs) = crate::tests::facilities::placed_executor::holding();
    let below = executor_fn(Capabilities::default(), no_state, reads_a_one)
        .run_on(placement)
        .shots(OnePerShot);
    let mut layer = compose(adapter_fn(no_state, pass_rejecting), below).expect("steps fit steps");
    let mut reply = (layer.call(InstructionBatchRef {
        shots: &shots(&[1, 2]),
        instructions: &[Step::Read(0)],
    }))
    .expect("the layer accepts the batch");
    let mut shots_jobs = jobs.try_iter();
    (shots_jobs.next().expect("shot 1's first call hands a job"))();
    let mut outcomes = Vec::new();
    assert_eq!(
        reply.poll_events(&mut outcomes, Waker::noop()),
        ReplyStatus::Pending,
        "shot 2's thread has not run"
    );
    assert_eq!(
        marked(&outcomes),
        [(1, 0, ShotConditions::REJECTED)],
        "shot 1's readout is handed over early"
    );
    (shots_jobs.next().expect("shot 2's first call hands a job"))();
    wait(reply, &mut outcomes).expect("the call ends well");
    assert_eq!(
        marked(&outcomes),
        [
            (1, 0, ShotConditions::REJECTED),
            (2, 0, ShotConditions::REJECTED)
        ]
    );
}

/// A profiler sends a call on shot 1 below, to a runtime on another
/// thread that rejects the shot and answers as the profiler's logic
/// returns: the shot's readout in the next call carries the rejection, as
/// an adapter takes in what its sends read up to the poll it makes as the
/// call ends.
#[test]
fn a_condition_a_send_reads_as_its_call_ends_marks_the_shots_outcomes_in_later_calls() {
    let below = ReadsLater::as_the_call_ends(ShotConditions::REJECTED);
    let mut profiled = compose(adapter_fn(no_state, pass), below).expect("steps fit steps");
    let first = outcomes_of(&mut profiled, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&first),
        [(1, 0, ShotConditions::REJECTED)],
        "the readout passes up with the rejection from below"
    );
    let outcomes = outcomes_of(&mut profiled, &[1], &[Step::Read(0)]);
    assert_eq!(marked(&outcomes), [(1, 0, ShotConditions::REJECTED)]);
}

/// A runtime that streams its readouts, keeping nothing across calls: its
/// reply to a batch with a [`Step::Read`] hands over, at its poll `at`, a
/// readout of each shot of the call under tag 0, found inconsistent,
/// while still pending; it waits through the polls an adapter makes
/// during a call (at the send, before a run, and as the call ends), then
/// hands over a second readout of each shot under tag 1, found
/// inconsistent too, and ends as its `ending` says. Its reply to a batch
/// without one ends at once with nothing.
struct ReadsWhilePending {
    at: u8,
    ending: Option<Ending>,
}

/// [`ReadsWhilePending`]'s reply: how many polls it has had, and those of
/// the runtime.
struct ReadsWhilePendingReply {
    polls: u8,
    at: u8,
    shots: Vec<ShotId>,
    ending: Option<Ending>,
}

impl Reply for ReadsWhilePendingReply {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        if self.shots.is_empty() {
            return ReplyStatus::Done;
        }
        let inconsistent = |&shot, tag| {
            ShotOutcome::new(shot, tag, Value::One).with_conditions(ShotConditions::INCONSISTENT)
        };
        let poll = self.polls;
        self.polls += 1;
        if poll == self.at {
            out.extend(self.shots.iter().map(|shot| inconsistent(shot, 0)));
        }
        if poll < 3 {
            waker.wake_by_ref();
            return ReplyStatus::Pending;
        }
        out.extend(self.shots.iter().map(|shot| inconsistent(shot, 1)));
        match self.ending {
            None => ReplyStatus::Done,
            Some(Ending::Failed) => ReplyStatus::Failed("the runtime lost its shots".into()),
            Some(Ending::Cancelled) => ReplyStatus::Cancelled,
        }
    }
}

impl Executor<InstructionBatch<Step>> for ReadsWhilePending {
    type Reply = ReadsWhilePendingReply;

    fn call(&mut self, batch: InstructionBatchRef<'_, Step>) -> Result<Self::Reply, Rejected> {
        let reads = (batch.instructions.iter()).any(|step| matches!(step, Step::Read(_)));
        Ok(ReadsWhilePendingReply {
            polls: 0,
            at: self.at,
            shots: if reads {
                batch.shots.to_vec()
            } else {
                Vec::new()
            },
            ending: self.ending,
        })
    }
}

impl Describe for ReadsWhilePending {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Close for ReadsWhilePending {
    type Report = ();

    fn close(&mut self) {}
}

/// A profiler sends a call on shot 1 below, to a runtime that streams:
/// its reply hands over the shot's first readout, found inconsistent,
/// while still pending, at the send, at the poll before a run the
/// profiler then makes, or as the call ends, after a run or not; and the
/// second once the call returned, ending well, failed or cancelled.
/// Before that reply is polled, the profiler's next call on the shot,
/// which goes nowhere below, writes a result: whichever way, the result
/// carries the inconsistency, as the adapter takes in what a send reads
/// during the call, its reply pending or not.
#[test]
fn a_condition_a_send_reads_while_its_reply_is_pending_in_the_call_marks_the_shots_outcomes_in_later_calls()
 {
    fn sends_or_writes(
        runs: &mut bool,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if input.instructions().is_empty() {
            outcomes.push(Outcome::new(7, Value::One));
            return Ok(());
        }
        below.send(input.instructions())?;
        if *runs {
            below.run(&[])?;
        }
        Ok(())
    }
    let inconsistent = ShotConditions::INCONSISTENT;
    for ending in [None, Some(Ending::Failed), Some(Ending::Cancelled)] {
        for (at, runs) in [(0, false), (1, true), (1, false), (2, true)] {
            let profiler = adapter_fn(move |_: ShotContext| Ok(runs), sends_or_writes);
            let below = ReadsWhilePending { at, ending };
            let mut chain = compose(profiler, below).expect("steps fit steps");
            let first = chain.call(InstructionBatchRef {
                shots: &shots(&[1]),
                instructions: &[Step::Read(0)],
            });
            let first = first.expect("the profiler accepts the first batch");
            let outcomes = outcomes_of(&mut chain, &[1], &[]);
            let case = format!("read at poll {at}, runs: {runs}, ends {ending:?}");
            assert_eq!(marked(&outcomes), [(1, 7, inconsistent)], "{case}");
            let mut read = Vec::new();
            let ended = wait_status(first, &mut read);
            assert_eq!(ended == ReplyStatus::Done, ending.is_none(), "{case}");
            assert_eq!(
                marked(&read),
                [(1, 0, inconsistent), (1, 1, inconsistent)],
                "the send's readouts pass up as they came, {case}"
            );
        }
    }
}

/// A layer with a state per shot sends, for each of shots 1 and 2, a
/// call of its own below, to a runtime that streams: each reply hands
/// over its shot's first readout, found inconsistent, while still
/// pending, as the call ends. Before those replies are polled, the
/// layer's next call on both shots, which goes nowhere below, writes a
/// result for each: each result carries the inconsistency, as the adapter
/// takes in what every send still pending reads as the call ends, the
/// earlier one's as well as the last's.
#[test]
fn with_a_state_per_shot_the_conditions_each_pending_send_reads_as_the_call_ends_mark_its_shots_outcomes_in_later_calls()
 {
    fn sends_its_own_or_writes(
        shot: &mut ShotId,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if input.instructions().is_empty() {
            outcomes.push(Outcome::new(7, Value::One));
            return Ok(());
        }
        below.send(&[Step::Read(*shot)])
    }
    let per_shot = adapter_fn(
        |context: ShotContext| Ok(context.shot()),
        sends_its_own_or_writes,
    )
    .shots(OnePerShot);
    let below = ReadsWhilePending {
        at: 1,
        ending: None,
    };
    let mut layer = compose(per_shot, below).expect("steps fit steps");
    let first = layer.call(InstructionBatchRef {
        shots: &shots(&[1, 2]),
        instructions: &[Step::Read(0)],
    });
    let first = first.expect("the layer accepts the first batch");
    let inconsistent = ShotConditions::INCONSISTENT;
    let outcomes = outcomes_of(&mut layer, &[1, 2], &[]);
    assert_eq!(
        marked(&outcomes),
        [(1, 7, inconsistent), (2, 7, inconsistent)]
    );
    let mut read = Vec::new();
    wait(first, &mut read).expect("the sends' replies end well");
    let mut read = marked(&read);
    read.sort_unstable_by_key(|&(shot, tag, _)| (shot, tag));
    assert_eq!(
        read,
        [
            (1, 0, inconsistent),
            (1, 1, inconsistent),
            (2, 0, inconsistent),
            (2, 1, inconsistent)
        ],
        "each send's readouts pass up"
    );
}

/// A layer, one state for all its shots, finds shot 1 rejected and sends
/// a call on it below, whose reply ends after the call, as one to a
/// runtime on another thread may: well, or failed, or cancelled. Before
/// that reply is polled, the layer's next call on the shot, which goes
/// nowhere below, writes a result: whichever way the send ends, the
/// result carries the rejection, as a call keeps what its logic raised
/// for later calls even while a send's reply is pending.
#[test]
fn a_condition_raised_in_a_call_whose_send_ends_after_it_marks_the_shots_outcomes_in_later_calls() {
    fn rejects_and_sends_or_writes(
        (): &mut (),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if input.instructions().is_empty() {
            outcomes.push(Outcome::new(7, Value::One));
            return Ok(());
        }
        outcomes.raise_conditions(ShotConditions::REJECTED);
        below.send(input.instructions())
    }
    for ending in [None, Some(Ending::Failed), Some(Ending::Cancelled)] {
        let mut layer = compose(
            adapter_fn(no_state, rejects_and_sends_or_writes),
            AnswersAsTold,
        )
        .expect("steps fit steps");
        let mut sent = vec![Step::Read(0), Step::AfterTheCall];
        sent.extend(ending.map(Step::Ends));
        let first = layer.call(InstructionBatchRef {
            shots: &shots(&[1]),
            instructions: &sent,
        });
        let first = first.expect("the layer accepts the first batch");
        let next = layer.call(InstructionBatchRef {
            shots: &shots(&[1]),
            instructions: &[],
        });
        let mut outcomes = Vec::new();
        let ended = wait(
            next.expect("the send's ending is not yet seen"),
            &mut outcomes,
        );
        ended.expect("the next call ends well");
        assert_eq!(
            marked(&outcomes),
            [(1, 7, ShotConditions::REJECTED)],
            "{ending:?}"
        );
        let ended = wait(first, &mut Vec::new());
        assert_eq!(ended.is_ok(), ending.is_none(), "the send ends {ending:?}");
    }
}

/// An observer sends each call on shot 1 below, to a runtime that offers
/// `resumable_shots` but keeps no condition from one call to the next, and
/// whose reply ends after the call with a readout it rejected: the readout
/// passes up with its rejection, but the adapter never sees it, and the
/// runtime gives the shot's next readout without it, so that readout
/// carries none. The runtime breaks the protocol's requirement on an
/// executor that offers `resumable_shots` (`docs/replies.md`, §Shot
/// conditions).
#[test]
fn an_adapter_whose_sends_end_after_their_call_loses_a_condition_over_an_executor_that_forgets_it()
{
    let below = ReadsLater::after_the_call(ShotConditions::REJECTED);
    let mut layer = compose(adapter_fn(no_state, pass), below).expect("steps fit steps");
    let first = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&first),
        [(1, 0, ShotConditions::REJECTED)],
        "the readout passes up with the rejection from below"
    );
    let outcomes = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(marked(&outcomes), [(1, 0, ShotConditions::NONE)]);
}

/// A profiler, one state for all its shots, sends each call below, to a
/// runtime that keeps its shots' conditions from one call to the next, as
/// an executor that offers `resumable_shots` must, and whose reply ends
/// after the call: the shot's readouts it passes up in later calls carry
/// the rejection again from below, by the conformance kit's check.
#[test]
fn an_adapter_whose_sends_end_after_their_call_keeps_each_shots_conditions_over_an_executor_that_keeps_them()
 {
    let below = ReadsLater::after_the_call(ShotConditions::REJECTED).keeping();
    let mut profiled = compose(adapter_fn(no_state, pass), below).expect("steps fit steps");
    conformance::keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut profiled,
        &shots(&[1]),
        &[&[Step::Read(0)], &[Step::Read(0)]],
    );
}

/// A profiler with a state per shot sends each call below, to a runtime
/// that keeps its shots' conditions from one call to the next, and whose
/// reply ends after the call: the shot's readouts it passes up in later
/// calls carry the rejection again from below, by the conformance kit's
/// check.
#[test]
fn with_a_state_per_shot_an_adapter_whose_sends_end_after_their_call_keeps_each_shots_conditions_over_an_executor_that_keeps_them()
 {
    let per_shot = adapter_fn(no_state, pass).shots(OnePerShot);
    let below = ReadsLater::after_the_call(ShotConditions::REJECTED).keeping();
    let mut profiled = compose(per_shot, below).expect("steps fit steps");
    conformance::keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut profiled,
        &shots(&[1, 2]),
        &[&[Step::Read(0)], &[Step::Read(0)]],
    );
}

/// A layer sends its first call on shot 1 below, to a runtime that keeps
/// its shots' conditions and whose reply ends after the call with a
/// readout it rejected, then writes the shot's readouts of later calls
/// itself, with no call below: the send's readout passes up with its
/// rejection, but the adapter never sees it, so the readout it writes in
/// the next call carries none (`docs/replies.md`, §Shot conditions, known
/// issue).
#[test]
fn a_logic_that_writes_a_shots_outcome_with_no_call_below_loses_a_condition_a_send_gave_after_its_call()
 {
    fn sends_first_then_writes(
        sent: &mut bool,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if *sent {
            outcomes.push(Outcome::new(0, Value::One));
            return Ok(());
        }
        *sent = true;
        below.send(input.instructions())
    }
    let below = ReadsLater::after_the_call(ShotConditions::REJECTED).keeping();
    let mut layer = compose(adapter_fn(|_| Ok(false), sends_first_then_writes), below)
        .expect("steps fit steps");
    let first = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&first),
        [(1, 0, ShotConditions::REJECTED)],
        "the send's readout passes up with the rejection from below"
    );
    let outcomes = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(marked(&outcomes), [(1, 0, ShotConditions::NONE)]);
}

/// A layer sends its first call on shot 1 below, to a runtime that keeps
/// its shots' conditions and whose reply ends after the call with a
/// readout it rejected; in the next call it writes a readout of the shot
/// itself and sends the call below again, whose reply ends after that
/// call too, so that the call reads no outcome of the shot below before
/// it ends: the readout the layer writes carries no rejection, while the
/// send's, passed up later, carries it (`docs/replies.md`, §Shot
/// conditions, known issue).
#[test]
fn a_logic_that_writes_a_shots_outcome_in_a_call_whose_send_ends_after_it_loses_a_condition_a_send_gave_after_its_call()
 {
    fn sends_then_writes_and_sends(
        sent: &mut bool,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if *sent {
            outcomes.push(Outcome::new(5, Value::One));
        }
        *sent = true;
        below.send(input.instructions())
    }
    let below = ReadsLater::after_the_call(ShotConditions::REJECTED).keeping();
    let mut layer = compose(
        adapter_fn(|_| Ok(false), sends_then_writes_and_sends),
        below,
    )
    .expect("steps fit steps");
    let first = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&first),
        [(1, 0, ShotConditions::REJECTED)],
        "the send's readout passes up with the rejection from below"
    );
    let outcomes = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&outcomes),
        [
            (1, 5, ShotConditions::NONE),
            (1, 0, ShotConditions::REJECTED)
        ]
    );
}

/// A layer sends its first call on shot 1 below, to a runtime that keeps
/// its shots' conditions and whose reply ends after the call with a
/// readout it rejected; in the next call it runs below a batch that reads
/// nothing, as a correction of gates alone would, then writes a readout
/// of the shot itself: the run reads no outcome of the shot, so the
/// readout the layer writes carries no rejection (`docs/replies.md`,
/// §Shot conditions, known issue).
#[test]
fn a_logic_that_writes_a_shots_outcome_after_a_run_below_that_reads_none_of_it_loses_a_condition_a_send_gave_after_its_call()
 {
    fn sends_then_runs_and_writes(
        sent: &mut bool,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if *sent {
            let read = below.run(&[])?;
            assert!(read.is_empty(), "a batch with no readout reads nothing");
            outcomes.push(Outcome::new(0, Value::One));
            return Ok(());
        }
        *sent = true;
        below.send(input.instructions())
    }
    let below = ReadsLater::after_the_call(ShotConditions::REJECTED).keeping();
    let mut layer = compose(adapter_fn(|_| Ok(false), sends_then_runs_and_writes), below)
        .expect("steps fit steps");
    let first = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&first),
        [(1, 0, ShotConditions::REJECTED)],
        "the send's readout passes up with the rejection from below"
    );
    let outcomes = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(marked(&outcomes), [(1, 0, ShotConditions::NONE)]);
}

/// A decoder sends its first call on shot 1 below, to a runtime that
/// keeps its shots' conditions and whose reply ends after the call with a
/// readout it rejected, then, in the next call, runs the shot below and
/// writes the shot's readout anew from what it read: the run reads an
/// outcome of the shot, which carries the rejection again, so the readout
/// the decoder writes carries it, though the adapter never saw the send's.
#[test]
fn a_logic_that_writes_a_shots_outcome_after_a_run_below_reads_it_keeps_a_condition_a_send_gave_after_its_call()
 {
    fn sends_first_then_decodes(
        sent: &mut bool,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if *sent {
            let read = below.run(input.instructions())?;
            outcomes.extend(decoded(read));
            return Ok(());
        }
        *sent = true;
        below.send(input.instructions())
    }
    let below = ReadsLater::after_the_call(ShotConditions::REJECTED).keeping();
    let mut decoder = compose(adapter_fn(|_| Ok(false), sends_first_then_decodes), below)
        .expect("steps fit steps");
    let first = outcomes_of(&mut decoder, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&first),
        [(1, 0, ShotConditions::REJECTED)],
        "the send's readout passes up with the rejection from below"
    );
    let outcomes = outcomes_of(&mut decoder, &[1], &[Step::Read(0)]);
    assert_eq!(marked(&outcomes), [(1, 100, ShotConditions::REJECTED)]);
}

/// A decoder sends its first call on shot 1 below, to a runtime that
/// keeps its shots' conditions and whose reply ends after the call with a
/// readout it rejected; in the next call it writes the shot's readout of
/// an earlier round, under tag 7, then runs the shot below: the run reads
/// an outcome of the shot that carries the rejection again before the call
/// ends, so the readout written before it carries the rejection too.
#[test]
fn a_logic_that_writes_a_shots_outcome_before_a_run_below_reads_it_in_the_call_keeps_a_condition_a_send_gave_after_its_call()
 {
    fn sends_first_then_writes_and_runs(
        sent: &mut bool,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if *sent {
            outcomes.push(Outcome::new(7, Value::One));
            below.run(input.instructions())?;
            return Ok(());
        }
        *sent = true;
        below.send(input.instructions())
    }
    let below = ReadsLater::after_the_call(ShotConditions::REJECTED).keeping();
    let mut decoder = compose(
        adapter_fn(|_| Ok(false), sends_first_then_writes_and_runs),
        below,
    )
    .expect("steps fit steps");
    let first = outcomes_of(&mut decoder, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&first),
        [(1, 0, ShotConditions::REJECTED)],
        "the send's readout passes up with the rejection from below"
    );
    let outcomes = outcomes_of(&mut decoder, &[1], &[Step::Read(0)]);
    assert_eq!(marked(&outcomes), [(1, 7, ShotConditions::REJECTED)]);
}

/// A layer sends its first call on shot 1 below, to a runtime that keeps
/// its shots' conditions and whose reply ends after the call with a
/// readout it rejected; in the second call it runs the shot below, whose
/// readout carries the rejection again; in the third it writes a readout
/// of the shot itself, with no call below: the adapter took the rejection
/// in from the second call's run, so the readout it writes carries it.
#[test]
fn a_condition_an_earlier_calls_run_read_marks_what_the_logic_writes_in_a_later_call_with_no_call_below()
 {
    fn sends_runs_then_writes(
        calls: &mut u32,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        *calls += 1;
        match *calls {
            1 => below.send(input.instructions()),
            2 => {
                let read = below.run(input.instructions())?;
                outcomes.extend(decoded(read));
                Ok(())
            }
            _ => {
                outcomes.push(Outcome::new(0, Value::One));
                Ok(())
            }
        }
    }
    let below = ReadsLater::after_the_call(ShotConditions::REJECTED).keeping();
    let mut layer =
        compose(adapter_fn(|_| Ok(0), sends_runs_then_writes), below).expect("steps fit steps");
    let first = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&first),
        [(1, 0, ShotConditions::REJECTED)],
        "the send's readout passes up with the rejection from below"
    );
    let second = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&second),
        [(1, 100, ShotConditions::REJECTED)],
        "the run reads the rejection again"
    );
    let outcomes = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(marked(&outcomes), [(1, 0, ShotConditions::REJECTED)]);
}

/// A caller gathers a shot's readouts of several calls in one buffer: it
/// holds shot 1's readout of an earlier call, then waits into the buffer
/// for a layer's reply that rejects shot 1 and whose send ends after the
/// call. The reply marks its own readout, and leaves the one the caller
/// held as it was, as a reply appends its events.
#[test]
fn an_adapters_reply_leaves_the_outcomes_its_caller_held_unmarked() {
    let mut layer = compose(
        adapter_fn(no_state, pass_rejecting),
        ReadsLater::after_the_call(ShotConditions::NONE),
    )
    .expect("steps fit steps");
    let reply = layer.call(InstructionBatchRef {
        shots: &shots(&[1]),
        instructions: &[Step::Read(0)],
    });
    let held = ShotOutcome::new(1, 9, Value::Zero);
    let mut outcomes = vec![held];
    wait(reply.expect("the layer accepts the batch"), &mut outcomes).expect("the call ends well");
    assert_eq!(
        marked(&outcomes[1..]),
        [(1, 0, ShotConditions::REJECTED)],
        "the reply marks its own readout"
    );
    assert_eq!(outcomes[0], held);
}

/// A decoder runs a call below in which the device rejects shot 1, writes
/// its readout, then fails: the reply hands over the readout marked with
/// the rejection, then ends with the failure.
#[test]
fn a_failed_calls_outcomes_carry_the_conditions_of_its_runs() {
    fn decode_then_fail(
        (): &mut (),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        decode(&mut (), input, below, outcomes)?;
        Err("the decoder found no correction".into())
    }
    let mut decoder =
        compose(adapter_fn(no_state, decode_then_fail), device()).expect("steps fit steps");
    let reply = decoder.call(InstructionBatchRef {
        shots: &shots(&[1]),
        instructions: &[Step::Raise(1, ShotConditions::REJECTED), Step::Read(0)],
    });
    let mut outcomes = Vec::new();
    let ended = wait(reply.expect("the decoder accepts the batch"), &mut outcomes);
    assert!(ended.is_err(), "the call fails");
    assert_eq!(marked(&outcomes), [(1, 100, ShotConditions::REJECTED)]);
}

/// A layer, one state for all its shots, sends shot 1's readout below,
/// finds the shot rejected, then fails: the reply hands over the send's
/// readout marked with the rejection, then ends with the failure.
#[test]
fn with_one_state_a_failed_calls_outcomes_carry_the_conditions_its_logic_raised() {
    fn sends_rejects_then_fails(
        (): &mut (),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        below.send(input.instructions())?;
        outcomes.raise_conditions(ShotConditions::REJECTED);
        Err("the layer found no correction".into())
    }
    let mut layer =
        compose(adapter_fn(no_state, sends_rejects_then_fails), device()).expect("steps fit steps");
    let reply = layer.call(InstructionBatchRef {
        shots: &shots(&[1]),
        instructions: &[Step::Read(0)],
    });
    let mut outcomes = Vec::new();
    let ended = wait(reply.expect("the layer accepts the batch"), &mut outcomes);
    assert!(ended.is_err(), "the call fails");
    assert_eq!(marked(&outcomes), [(1, 0, ShotConditions::REJECTED)]);
}

/// A layer with a state per shot sends shot 1's readout below, runs a
/// syndrome extraction below, before which the send goes down alone,
/// finds the shot rejected, then fails: the reply hands over the send's
/// readout marked with the rejection, then ends with the failure.
#[test]
fn with_a_state_per_shot_a_failed_calls_outcomes_carry_the_conditions_its_logic_raised() {
    fn sends_extracts_rejects_then_fails(
        (): &mut (),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        below.send(input.instructions())?;
        below.run(&[Step::Read(9)])?;
        outcomes.raise_conditions(ShotConditions::REJECTED);
        Err("the layer found no correction".into())
    }
    let per_shot = adapter_fn(no_state, sends_extracts_rejects_then_fails).shots(OnePerShot);
    let mut layer = compose(per_shot, device()).expect("steps fit steps");
    let reply = layer.call(InstructionBatchRef {
        shots: &shots(&[1]),
        instructions: &[Step::Read(0)],
    });
    let mut outcomes = Vec::new();
    let ended = wait(reply.expect("the layer accepts the batch"), &mut outcomes);
    assert!(ended.is_err(), "the call fails");
    assert_eq!(marked(&outcomes), [(1, 0, ShotConditions::REJECTED)]);
}

/// How a [`ReadsLaterThenEnds`]'s, an [`AnswersAsTold`]'s or a
/// [`ReadsAsListed`]'s reply ends once it has read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tests::facilities) enum Ending {
    /// Failed, as a runtime that loses its shots after reading them.
    Failed,
    /// Cancelled, as a runtime closed with the call in flight.
    Cancelled,
}

/// A [`ReadsLater`] whose reply, once it has read, ends as its [`Ending`]
/// says.
struct ReadsLaterThenEnds(ReadsLater, Ending);

/// [`ReadsLaterThenEnds`]'s reply.
struct ReadsLaterThenEndsReply(ReadsLaterReply, Ending);

impl Reply for ReadsLaterThenEndsReply {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        match (self.0.poll_events(out, waker), self.1) {
            (ReplyStatus::Done, Ending::Failed) => {
                ReplyStatus::Failed("the runtime lost its shots".into())
            }
            (ReplyStatus::Done, Ending::Cancelled) => ReplyStatus::Cancelled,
            (status, _) => status,
        }
    }
}

impl Executor<InstructionBatch<Step>> for ReadsLaterThenEnds {
    type Reply = ReadsLaterThenEndsReply;

    fn call(&mut self, batch: InstructionBatchRef<'_, Step>) -> Result<Self::Reply, Rejected> {
        let ending = self.1;
        (self.0.call(batch)).map(|reply| ReadsLaterThenEndsReply(reply, ending))
    }
}

impl Describe for ReadsLaterThenEnds {
    fn capabilities(&self) -> Capabilities {
        self.0.capabilities()
    }
}

impl Close for ReadsLaterThenEnds {
    type Report = ();

    fn close(&mut self) {}
}

/// A layer rejects shot 1 and sends a call below, to a runtime whose
/// reply ends after the call: it reads the shot, then fails. The layer's
/// reply, failed by the send's, hands over the send's readout marked with
/// the rejection, then ends with the failure.
#[test]
fn a_reply_failed_by_a_send_after_its_call_marks_what_it_hands_over() {
    let below = ReadsLaterThenEnds(
        ReadsLater::after_the_call(ShotConditions::NONE),
        Ending::Failed,
    );
    let mut layer = compose(adapter_fn(no_state, pass_rejecting), below).expect("steps fit steps");
    let reply = layer.call(InstructionBatchRef {
        shots: &shots(&[1]),
        instructions: &[Step::Read(0)],
    });
    let mut outcomes = Vec::new();
    let ended = wait(reply.expect("the layer accepts the batch"), &mut outcomes);
    assert!(ended.is_err(), "the send's reply fails");
    assert_eq!(marked(&outcomes), [(1, 0, ShotConditions::REJECTED)]);
}

/// The batch a layer sends below to [`AnswersAsTold`] in the tests of a
/// send that ends after its call: two readouts of each shot, under tags
/// 0 and 1, shot 9 found inconsistent, given in one poll after the call,
/// the reply ending as `ending` says.
fn two_readouts_each_after_the_call(ending: Option<Ending>) -> Vec<Step> {
    let mut batch = vec![
        Step::Raise(9, ShotConditions::INCONSISTENT),
        Step::Read(0),
        Step::Read(1),
        Step::AfterTheCall,
    ];
    batch.extend(ending.map(Step::Ends));
    batch
}

/// What the reply of a call of `batch` on `ids` through `layer` hands
/// over, as each outcome's shot, tag and conditions, once it ends as
/// `ending` says.
fn handed_over_as_the_send_ends(
    layer: &mut impl Executor<InstructionBatch<Step>>,
    ids: &[ShotId],
    batch: &[Step],
    ending: Option<Ending>,
) -> Vec<(ShotId, u32, ShotConditions)> {
    let reply = layer.call(InstructionBatchRef {
        shots: &shots(ids),
        instructions: batch,
    });
    let mut outcomes = Vec::new();
    let ended = wait_status(reply.expect("the layer accepts the batch"), &mut outcomes);
    assert_eq!(
        ended == ReplyStatus::Done,
        ending.is_none(),
        "the call ends as its send does, {ending:?}: {ended:?}"
    );
    marked(&outcomes)
}

/// A layer, one state for all its shots, rejects shots 9, 1 and 6 and
/// sends the call below, to a runtime whose reply gives two readouts of
/// each shot, shot 9's found inconsistent, at one poll after the call,
/// then ends well, failed or cancelled: whichever, every readout carries
/// the rejection, both of each shot, shot 9's beside the inconsistency.
#[test]
fn every_outcome_a_send_hands_over_after_its_call_carries_the_shots_conditions_however_it_ends() {
    let both = ShotConditions::REJECTED | ShotConditions::INCONSISTENT;
    let rejected = ShotConditions::REJECTED;
    for ending in [None, Some(Ending::Failed), Some(Ending::Cancelled)] {
        let mut layer =
            compose(adapter_fn(no_state, pass_rejecting), AnswersAsTold).expect("steps fit steps");
        let batch = two_readouts_each_after_the_call(ending);
        assert_eq!(
            handed_over_as_the_send_ends(&mut layer, &[9, 1, 6], &batch, ending),
            [
                (9, 0, both),
                (9, 1, both),
                (1, 0, rejected),
                (1, 1, rejected),
                (6, 0, rejected),
                (6, 1, rejected)
            ],
            "{ending:?}"
        );
    }
}

/// A layer with a state per shot, which knows its shot, given shots 9, 1,
/// 6 and 2, rejects shot 1, finds shot 6 unresolved and sends each shot's
/// call below, to a runtime whose reply gives two readouts of each shot,
/// shot 9's found inconsistent, at one poll after the call, then ends
/// well, failed or cancelled: whichever, both readouts of each shot carry
/// that shot's own conditions alone, and shot 2's none.
#[test]
fn with_a_state_per_shot_every_outcome_a_send_hands_over_after_its_call_carries_its_own_shots_conditions_however_it_ends()
 {
    fn flags_its_shot_then_sends(
        shot: &mut ShotId,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        match *shot {
            1 => outcomes.raise_conditions(ShotConditions::REJECTED),
            6 => outcomes.raise_conditions(ShotConditions::UNRESOLVED),
            _ => {}
        }
        below.send(input.instructions())
    }
    let (inconsistent, rejected, unresolved, none) = (
        ShotConditions::INCONSISTENT,
        ShotConditions::REJECTED,
        ShotConditions::UNRESOLVED,
        ShotConditions::NONE,
    );
    for ending in [None, Some(Ending::Failed), Some(Ending::Cancelled)] {
        let per_shot = adapter_fn(
            |context: ShotContext| Ok(context.shot()),
            flags_its_shot_then_sends,
        )
        .shots(OnePerShot);
        let mut layer = compose(per_shot, AnswersAsTold).expect("steps fit steps");
        let batch = two_readouts_each_after_the_call(ending);
        assert_eq!(
            handed_over_as_the_send_ends(&mut layer, &[9, 1, 6, 2], &batch, ending),
            [
                (9, 0, inconsistent),
                (9, 1, inconsistent),
                (1, 0, rejected),
                (1, 1, rejected),
                (6, 0, unresolved),
                (6, 1, unresolved),
                (2, 0, none),
                (2, 1, none)
            ],
            "{ending:?}"
        );
    }
}

/// A layer with a state per shot, which knows its shot, given shots 1 and
/// 2, rejects shot 1, and in a second case finds shot 2 unresolved, and
/// sends each shot's call below, to a runtime whose reply gives, at one
/// poll after the call, two readouts of each shot, by shot or
/// interleaved, then ends well, failed or cancelled: whichever, each
/// readout carries its own shot's conditions, the first of a shot and the
/// second, whatever their order, and an unflagged shot's none.
#[test]
fn with_a_state_per_shot_every_outcome_a_send_hands_over_after_its_call_carries_its_own_shots_conditions_whatever_their_order()
 {
    fn flags_then_sends(
        (shot, unresolved_too): &mut (ShotId, bool),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        match (*shot, *unresolved_too) {
            (1, _) => outcomes.raise_conditions(ShotConditions::REJECTED),
            (2, true) => outcomes.raise_conditions(ShotConditions::UNRESOLVED),
            _ => {}
        }
        below.send(input.instructions())
    }
    let none = ShotConditions::NONE;
    let by_shot = [(1, 0, none), (1, 1, none), (2, 0, none), (2, 1, none)];
    let interleaved = [(1, 0, none), (2, 0, none), (1, 1, none), (2, 1, none)];
    for unresolved_too in [false, true] {
        let of = |shot| match (shot, unresolved_too) {
            (1, _) => ShotConditions::REJECTED,
            (2, true) => ShotConditions::UNRESOLVED,
            _ => none,
        };
        for read in [by_shot, interleaved] {
            for ending in [None, Some(Ending::Failed), Some(Ending::Cancelled)] {
                let per_shot = adapter_fn(
                    move |context: ShotContext| Ok((context.shot(), unresolved_too)),
                    flags_then_sends,
                )
                .shots(OnePerShot);
                let below = ReadsAsListed {
                    read: readouts(&read),
                    after_the_call: true,
                    ending,
                };
                let mut layer = compose(per_shot, below).expect("steps fit steps");
                let batch = [Step::Read(0), Step::Read(1)];
                assert_eq!(
                    handed_over_as_the_send_ends(&mut layer, &[1, 2], &batch, ending),
                    read.map(|(shot, tag, _)| (shot, tag, of(shot))),
                    "shot 2 unresolved: {unresolved_too}, read {read:?}, {ending:?}"
                );
            }
        }
    }
}

/// A layer writes shot 1's result of an earlier round, then sends the
/// call below, and runs a round below or not, to a runtime that finds the
/// shot inconsistent and reads it at the poll the layer makes at the send,
/// before the run, or as the call ends, its reply ending there, and the
/// run's too: well, or failed, or cancelled. Whichever, the reply hands
/// over the result the layer wrote and the send's readout, both marked
/// inconsistent, as the send read the shot during the call. The call ends
/// as the send did, well, failed or cancelled, but fails where the run is
/// cancelled too, as a cancelled run fails the call.
#[test]
fn a_condition_a_send_reads_at_the_poll_where_its_reply_ends_marks_the_outcomes_the_logic_wrote_however_it_ends()
 {
    fn writes_sends_then_runs(
        runs: &mut bool,
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        outcomes.push(Outcome::new(7, Value::One));
        below.send(input.instructions())?;
        if *runs {
            below.run(&[])?;
        }
        Ok(())
    }
    fn handed_over(
        below: impl Executor<InstructionBatch<Step>> + Close + Describe,
        runs: bool,
    ) -> (ReplyStatus, Vec<(ShotId, u32, ShotConditions)>) {
        let layer = adapter_fn(move |_: ShotContext| Ok(runs), writes_sends_then_runs);
        let mut layer = compose(layer, below).expect("steps fit steps");
        let reply = layer.call(InstructionBatchRef {
            shots: &shots(&[1]),
            instructions: &[Step::Read(0)],
        });
        let mut outcomes = Vec::new();
        let end = wait_status(reply.expect("the layer accepts the batch"), &mut outcomes);
        (end, marked(&outcomes))
    }
    let inconsistent = ShotConditions::INCONSISTENT;
    for ending in [None, Some(Ending::Failed), Some(Ending::Cancelled)] {
        for (reads, runs, at) in [
            (ReadsLater::at_the_send(inconsistent), false, "at the send"),
            (
                ReadsLater::as_the_call_ends(inconsistent),
                true,
                "before the run",
            ),
            (
                ReadsLater::as_the_call_ends(inconsistent),
                false,
                "as the call ends",
            ),
        ] {
            let (end, outcomes) = match ending {
                None => handed_over(reads, runs),
                Some(how) => handed_over(ReadsLaterThenEnds(reads, how), runs),
            };
            assert!(
                matches!(
                    (ending, runs, &end),
                    (None, _, ReplyStatus::Done)
                        | (Some(Ending::Failed), _, ReplyStatus::Failed(_))
                        | (Some(Ending::Cancelled), false, ReplyStatus::Cancelled)
                        | (Some(Ending::Cancelled), true, ReplyStatus::Failed(_))
                ),
                "{ending:?} {at}: {end:?}"
            );
            assert_eq!(
                outcomes,
                [(1, 7, inconsistent), (1, 0, inconsistent)],
                "{ending:?} {at}"
            );
        }
    }
}

/// A layer writes shot 1's result of an earlier round, then sends a call
/// on the shot below, whose reply is still pending after the call, or
/// reads the shot, then is cancelled at its first poll; then runs a round
/// below, which finds the shot inconsistent and ends well, or fails, or
/// is cancelled. Whichever, the reply hands over the result the layer
/// wrote and the send's readout, both marked inconsistent, as the run
/// read the shot during the call.
#[test]
fn a_condition_a_run_reads_after_a_send_pending_or_cancelled_marks_the_outcomes_the_logic_wrote_however_the_run_ends()
 {
    fn writes_sends_then_runs(
        &mut (send_cancelled, run_ends): &mut (bool, Option<Ending>),
        _: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        outcomes.push(Outcome::new(7, Value::One));
        if send_cancelled {
            below.send(&[Step::Read(0), Step::Ends(Ending::Cancelled)])?;
        } else {
            below.send(&[Step::Read(0), Step::AfterTheCall])?;
        }
        let mut ran = vec![Step::Raise(1, ShotConditions::INCONSISTENT), Step::Read(5)];
        ran.extend(run_ends.map(Step::Ends));
        below.run(&ran).map(drop)
    }
    let inconsistent = ShotConditions::INCONSISTENT;
    for send_cancelled in [false, true] {
        for run_ends in [None, Some(Ending::Failed), Some(Ending::Cancelled)] {
            let ways = (send_cancelled, run_ends);
            let layer = adapter_fn(move |_: ShotContext| Ok(ways), writes_sends_then_runs);
            let mut layer = compose(layer, AnswersAsTold).expect("steps fit steps");
            let reply = layer.call(InstructionBatchRef {
                shots: &shots(&[1]),
                instructions: &[],
            });
            let mut outcomes = Vec::new();
            let ended = wait(reply.expect("the layer accepts the batch"), &mut outcomes);
            let well = !send_cancelled && run_ends.is_none();
            assert_eq!(ended.is_ok(), well, "{ways:?}");
            assert_eq!(
                marked(&outcomes),
                [(1, 7, inconsistent), (1, 0, inconsistent)],
                "{ways:?}"
            );
        }
    }
}

/// The logic of a decoder that writes, under tag 100, the readout it
/// decoded from an earlier round, then runs the call below for the next.
fn writes_then_runs(
    (): &mut (),
    input: &CallInput<'_, Step>,
    below: &mut Below<'_, Step>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    outcomes.push(Outcome::new(100, Value::One));
    below.run(input.instructions())?;
    Ok(())
}

/// The reply of a call on `ids` through a decoder that writes, then runs,
/// over `below`: what it hands over, as each outcome's shot, tag and
/// conditions, once it ends badly.
fn handed_over_when_the_run_ends_badly(
    below: impl Executor<InstructionBatch<Step>> + Close + Describe,
    ids: &[ShotId],
) -> Vec<(ShotId, u32, ShotConditions)> {
    let mut decoder =
        compose(adapter_fn(no_state, writes_then_runs), below).expect("steps fit steps");
    let reply = decoder.call(InstructionBatchRef {
        shots: &shots(ids),
        instructions: &[Step::Read(0)],
    });
    let mut outcomes = Vec::new();
    let ended = wait(reply.expect("the decoder accepts the batch"), &mut outcomes);
    ended.expect_err("the run below ends badly, so the call fails");
    marked(&outcomes)
}

/// A decoder writes shot 1's readout of an earlier round, then runs the
/// next round below, on a runtime that finds the shot inconsistent, reads
/// it, then loses its shots: the call fails, and its reply hands over the
/// readout the decoder wrote, marked inconsistent, as the run read it
/// before failing.
#[test]
fn a_condition_a_failed_run_read_marks_the_outcomes_the_logic_wrote_in_the_call() {
    let below = ReadsLaterThenEnds(
        ReadsLater::after_the_call(ShotConditions::INCONSISTENT),
        Ending::Failed,
    );
    assert_eq!(
        handed_over_when_the_run_ends_badly(below, &[1]),
        [(1, 100, ShotConditions::INCONSISTENT)]
    );
}

/// A decoder writes shot 1's readout of an earlier round, then runs the
/// next round below, on a runtime that finds the shot inconsistent, reads
/// it, then is closed with the call in flight: the call is cancelled, and
/// its reply hands over the readout the decoder wrote, marked
/// inconsistent, as the run read it before it was cancelled.
#[test]
fn a_condition_a_cancelled_run_read_marks_the_outcomes_the_logic_wrote_in_the_call() {
    let below = ReadsLaterThenEnds(
        ReadsLater::after_the_call(ShotConditions::INCONSISTENT),
        Ending::Cancelled,
    );
    assert_eq!(
        handed_over_when_the_run_ends_badly(below, &[1]),
        [(1, 100, ShotConditions::INCONSISTENT)]
    );
}

/// A decoder, one state for all its shots, given shots 2 and 1 in that
/// order, as a driver given them so sends them, writes each shot's readout
/// of an earlier round, then runs the next round below on both, on a
/// runtime that finds them inconsistent, reads them, then loses its shots:
/// the call fails, and its reply hands over both readouts the decoder
/// wrote, each marked inconsistent, as the run read each of its shots
/// before failing.
#[test]
fn a_failed_run_takes_in_the_conditions_it_read_on_each_of_its_shots_whatever_their_order() {
    let below = ReadsLaterThenEnds(
        ReadsLater::after_the_call(ShotConditions::INCONSISTENT),
        Ending::Failed,
    );
    assert_eq!(
        handed_over_when_the_run_ends_badly(below, &[2, 1]),
        [
            (2, 100, ShotConditions::INCONSISTENT),
            (1, 100, ShotConditions::INCONSISTENT)
        ]
    );
}

/// A decoder, one state for all its shots, given shots 9, 1, 6 and 2,
/// writes each shot's readout of an earlier round, then runs the next
/// round below, on a faulty runtime that rejects shot 9, finds shot 1
/// inconsistent and shot 6 unresolved, and reads each shot under tag 0,
/// or under tags 0 to 17, then under tag 0 again: the run is not whole,
/// as it reads a tag of a shot twice, so the call fails, and its reply
/// hands over the readouts the decoder wrote, each marked with its own
/// shot's conditions, as the run read them before it was found not whole,
/// whether a shot has few readouts or many.
#[test]
fn a_run_that_reads_a_tag_of_a_shot_twice_takes_in_the_conditions_it_read_on_each_of_its_shots() {
    for tags in [1, 18] {
        let mut batch = vec![
            Step::Raise(9, ShotConditions::REJECTED),
            Step::Raise(1, ShotConditions::INCONSISTENT),
            Step::Raise(6, ShotConditions::UNRESOLVED),
        ];
        batch.extend((0..tags).map(Step::Read));
        batch.push(Step::Read(0));
        let mut decoder = compose(adapter_fn(no_state, writes_then_runs), AnswersAsTold)
            .expect("steps fit steps");
        let reply = decoder.call(InstructionBatchRef {
            shots: &shots(&[9, 1, 6, 2]),
            instructions: &batch,
        });
        let mut outcomes = Vec::new();
        let ended = wait_status(reply.expect("the decoder accepts the batch"), &mut outcomes);
        let ReplyStatus::Failed(reason) = ended else {
            panic!("a run that reads a tag twice fails the call, not {ended:?}");
        };
        assert!(reason.contains("twice"), "the run is not whole: {reason}");
        assert_eq!(
            marked(&outcomes),
            [
                (9, 100, ShotConditions::REJECTED),
                (1, 100, ShotConditions::INCONSISTENT),
                (6, 100, ShotConditions::UNRESOLVED),
                (2, 100, ShotConditions::NONE)
            ],
            "{tags} tags, then tag 0 again"
        );
    }
}

/// A decoder, one state for all its shots, writes each shot's readout of
/// an earlier round, then runs the next round below, on a runtime that
/// reads each shot twice, in an order of its own, marking one of a shot's
/// two readouts rejected or unresolved, then ends well, failed or
/// cancelled: shot 1's first readout clear and its second rejected, as a
/// runtime that rejects a shot on its last readout then loses its shots,
/// or the other way round; shots 1 and 2, each shot's two in a row, shot
/// 1's second marked and shot 2's first, or the same interleaved; or a
/// tag of shot 1 read twice, so that the run is not whole and the call
/// fails. However the run ends, each readout the decoder wrote carries
/// its own shot's conditions, whichever of its readouts the run read them
/// on.
#[test]
fn a_runs_conditions_mark_the_outcomes_the_logic_wrote_whatever_the_order_of_its_readouts_however_it_ends()
 {
    let (rejected, unresolved, none) = (
        ShotConditions::REJECTED,
        ShotConditions::UNRESOLVED,
        ShotConditions::NONE,
    );
    let one: &[ShotId] = &[1];
    let both: &[ShotId] = &[1, 2];
    let flagged = [(1, 100, rejected), (2, 100, unresolved)];
    let cases = [
        (
            one,
            vec![(1, 0, none), (1, 1, rejected)],
            &flagged[..1],
            true,
        ),
        (
            one,
            vec![(1, 0, rejected), (1, 1, none)],
            &flagged[..1],
            true,
        ),
        (
            both,
            vec![
                (1, 0, none),
                (1, 1, rejected),
                (2, 0, unresolved),
                (2, 1, none),
            ],
            &flagged[..],
            true,
        ),
        (
            both,
            vec![
                (1, 0, none),
                (2, 0, unresolved),
                (1, 1, rejected),
                (2, 1, none),
            ],
            &flagged[..],
            true,
        ),
        (
            both,
            vec![
                (1, 0, none),
                (2, 0, unresolved),
                (1, 0, rejected),
                (2, 1, none),
            ],
            &flagged[..],
            false,
        ),
    ];
    for (ids, read, expected, whole) in cases {
        for ending in [Some(Ending::Failed), None, Some(Ending::Cancelled)] {
            let below = ReadsAsListed {
                read: readouts(&read),
                after_the_call: false,
                ending,
            };
            let mut decoder =
                compose(adapter_fn(no_state, writes_then_runs), below).expect("steps fit steps");
            let reply = decoder.call(InstructionBatchRef {
                shots: &shots(ids),
                instructions: &[Step::Read(0), Step::Read(1)],
            });
            let mut outcomes = Vec::new();
            let ended = wait(reply.expect("the decoder accepts the batch"), &mut outcomes);
            assert_eq!(
                ended.is_ok(),
                whole && ending.is_none(),
                "the call ends as its run does: read {read:?}, {ending:?}"
            );
            assert_eq!(marked(&outcomes), expected, "read {read:?}, {ending:?}");
        }
    }
}

/// A runtime that answers each call as its batch says, keeping nothing
/// across calls: its reply reads a One on each shot of the call under each
/// [`Step::Read`]'s tag, carrying the conditions that the batch's
/// [`Step::Raise`]s name for the shot; it ends in the call, or after it
/// with a [`Step::AfterTheCall`], well, or as a [`Step::Ends`] says. It
/// refuses a batch with a [`Step::Refused`].
struct AnswersAsTold;

/// [`AnswersAsTold`]'s and [`ReadsAsListed`]'s reply: how many more polls
/// it waits through, what it reads, and how it ends, if not well.
struct AnswersAsToldReply(u8, Vec<ShotOutcome>, Option<Ending>);

impl Reply for AnswersAsToldReply {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        if self.0 > 0 {
            self.0 -= 1;
            waker.wake_by_ref();
            return ReplyStatus::Pending;
        }
        out.append(&mut self.1);
        match self.2 {
            None => ReplyStatus::Done,
            Some(Ending::Failed) => ReplyStatus::Failed("the runtime lost its shots".into()),
            Some(Ending::Cancelled) => ReplyStatus::Cancelled,
        }
    }
}

impl Executor<InstructionBatch<Step>> for AnswersAsTold {
    type Reply = AnswersAsToldReply;

    fn call(&mut self, batch: InstructionBatchRef<'_, Step>) -> Result<Self::Reply, Rejected> {
        let steps = batch.instructions;
        if let Some(at) = steps.iter().position(|&step| step == Step::Refused) {
            return Err(Rejected::Instruction {
                at,
                reason: "the runtime cannot run it".into(),
            });
        }
        let mut read = Vec::new();
        for &shot in batch.shots {
            let conditions =
                (steps.iter()).fold(ShotConditions::NONE, |raised, &step| match step {
                    Step::Raise(on, conditions) if on == shot => raised | conditions,
                    _ => raised,
                });
            for &step in steps {
                if let Step::Read(tag) = step {
                    read.push(ShotOutcome::new(shot, tag, Value::One).with_conditions(conditions));
                }
            }
        }
        // Through every poll an adapter makes during a call: at the send,
        // before a run, and as the call ends.
        let waits = if steps.contains(&Step::AfterTheCall) {
            3
        } else {
            0
        };
        let ending = (steps.iter()).find_map(|&step| match step {
            Step::Ends(ending) => Some(ending),
            _ => None,
        });
        Ok(AnswersAsToldReply(waits, read, ending))
    }
}

impl Describe for AnswersAsTold {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Close for AnswersAsTold {
    type Report = ();

    fn close(&mut self) {}
}

/// A runtime that reads, whatever the batch it is given, the readouts it
/// lists, in that order, as one that gives a shot's readouts in an order
/// of its own: its reply gives them at one poll, in the call, or after it
/// as [`Step::AfterTheCall`] makes [`AnswersAsTold`]'s, then ends well, or
/// as its [`Ending`] says.
struct ReadsAsListed {
    read: Vec<ShotOutcome>,
    after_the_call: bool,
    ending: Option<Ending>,
}

impl Executor<InstructionBatch<Step>> for ReadsAsListed {
    type Reply = AnswersAsToldReply;

    fn call(&mut self, _: InstructionBatchRef<'_, Step>) -> Result<Self::Reply, Rejected> {
        let waits = if self.after_the_call { 3 } else { 0 };
        Ok(AnswersAsToldReply(waits, self.read.clone(), self.ending))
    }
}

impl Describe for ReadsAsListed {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Close for ReadsAsListed {
    type Report = ();

    fn close(&mut self) {}
}

/// A way a call through an adapter ends, but refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallEnding {
    /// Well.
    Well,
    /// Failed by its logic's `Err`.
    ByItsLogic,
    /// Failed by a run whose reply fails or is cancelled.
    ByARun(Ending),
    /// Failed, or cancelled, by a send whose reply ends so in the call.
    ByASendInTheCall(Ending),
    /// Failed, or cancelled, by a send whose reply ends so after the call.
    ByASendAfterTheCall(Ending),
    /// Failed by a call below refused after an earlier one went down.
    ByARefusalBelow,
}

/// Every [`CallEnding`].
const EVERY_CALL_ENDING: [CallEnding; 9] = [
    CallEnding::Well,
    CallEnding::ByItsLogic,
    CallEnding::ByARun(Ending::Failed),
    CallEnding::ByARun(Ending::Cancelled),
    CallEnding::ByASendInTheCall(Ending::Failed),
    CallEnding::ByASendInTheCall(Ending::Cancelled),
    CallEnding::ByASendAfterTheCall(Ending::Failed),
    CallEnding::ByASendAfterTheCall(Ending::Cancelled),
    CallEnding::ByARefusalBelow,
];

impl CallEnding {
    /// The tags of shot 1's readouts that the reply of a call on the shot
    /// hands over, after what its logic wrote, when the logic sent one
    /// under tag 0, whose reply ends in the call, then ended the call so
    /// ([`ends_as`]): tag 2 when a send read it before ending badly in the
    /// call, and tag 1 unless that send failed, which drops the replies
    /// still pending.
    const fn readouts(self) -> &'static [u32] {
        match self {
            Self::ByASendInTheCall(Ending::Failed) => &[0, 2],
            Self::ByASendInTheCall(Ending::Cancelled) => &[0, 2, 1],
            _ => &[0, 1],
        }
    }
}

/// Ends, as `ending` says, a call through a layer, one state for all its
/// shots: it sends the call's shots' readouts under tag 1, whose reply
/// ends after the call, as `ending` says if it names such a send; then
/// returns, by `Err` if `ending` says so, or ends the call by a run or a
/// send of readouts under tag 2, whose reply fails or is cancelled, or by
/// a call below that is refused.
fn ends_as(ending: CallEnding, below: &mut Below<'_, Step>) -> Result<(), AdapterError> {
    match ending {
        CallEnding::ByASendAfterTheCall(how) => {
            below.send(&[Step::Read(1), Step::AfterTheCall, Step::Ends(how)])?;
        }
        _ => below.send(&[Step::Read(1), Step::AfterTheCall])?,
    }
    match ending {
        CallEnding::Well | CallEnding::ByASendAfterTheCall(_) => Ok(()),
        CallEnding::ByItsLogic => Err("the layer found no correction".into()),
        CallEnding::ByARun(how) => below.run(&[Step::Read(2), Step::Ends(how)]).map(drop),
        CallEnding::ByASendInTheCall(how) => below.send(&[Step::Read(2), Step::Ends(how)]),
        CallEnding::ByARefusalBelow => below.send(&[Step::Refused]),
    }
}

/// What the reply of a call on shot 1 through `layer` hands over, as each
/// outcome's shot, tag and conditions, once the call ends as `ending`
/// says.
fn handed_over_as_it_ends(
    layer: &mut impl Executor<InstructionBatch<Step>>,
    ending: CallEnding,
) -> Vec<(ShotId, u32, ShotConditions)> {
    let reply = layer.call(InstructionBatchRef {
        shots: &shots(&[1]),
        instructions: &[],
    });
    let mut outcomes = Vec::new();
    let ended = wait(reply.expect("the layer accepts the batch"), &mut outcomes);
    assert_eq!(
        ended.is_ok(),
        ending == CallEnding::Well,
        "the call ends {ending:?}"
    );
    marked(&outcomes)
}

/// A layer, one state for all its shots, finds shot 1 rejected, writes no
/// result, then sends readouts of the shot below, to a runtime that finds
/// the shot inconsistent in the first, whose reply ends in the call;
/// then the call ends, in turn, each way a call ends but refused: well,
/// by the logic's `Err`, by a run, a send in the call or a send after it
/// that fails or is cancelled, or by a call below refused after the
/// others went down. Whichever, every readout its reply hands over
/// carries both conditions.
#[test]
fn however_a_call_ends_unless_refused_it_marks_what_it_hands_over_with_the_conditions_its_logic_raised_and_its_sends_read()
 {
    fn rejects_sends_then_ends(
        ending: &mut CallEnding,
        _: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        outcomes.raise_conditions(ShotConditions::REJECTED);
        below.send(&[Step::Raise(1, ShotConditions::INCONSISTENT), Step::Read(0)])?;
        ends_as(*ending, below)
    }
    let both = ShotConditions::REJECTED | ShotConditions::INCONSISTENT;
    for ending in EVERY_CALL_ENDING {
        let layer = adapter_fn(move |_: ShotContext| Ok(ending), rejects_sends_then_ends);
        let mut layer = compose(layer, AnswersAsTold).expect("steps fit steps");
        let expected: Vec<_> = (ending.readouts().iter())
            .map(|&tag| (1, tag, both))
            .collect();
        assert_eq!(
            handed_over_as_it_ends(&mut layer, ending),
            expected,
            "{ending:?}"
        );
    }
}

/// A decoder, one state for all its shots, runs a round below on shot 1,
/// which the runtime finds inconsistent, writes the readout it decoded,
/// which its own check finds unresolved, then sends readouts of the shot
/// of later rounds below; then the call ends, in turn, each way a call
/// ends but refused, as in the test above. Whichever, every outcome its
/// reply hands over carries the inconsistency the run read and the
/// unresolved result the decoder wrote.
#[test]
fn however_a_call_ends_unless_refused_it_marks_what_it_hands_over_with_the_conditions_its_runs_read_and_its_logic_wrote()
 {
    fn decodes_sends_then_ends(
        ending: &mut CallEnding,
        _: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        let read = below.run(&[Step::Raise(1, ShotConditions::INCONSISTENT), Step::Read(5)])?;
        let unresolved = |o: ShotOutcome| o.with_conditions(ShotConditions::UNRESOLVED);
        outcomes.extend(decoded(read).map(unresolved));
        below.send(&[Step::Read(0)])?;
        ends_as(*ending, below)
    }
    let both = ShotConditions::INCONSISTENT | ShotConditions::UNRESOLVED;
    for ending in EVERY_CALL_ENDING {
        let decoder = adapter_fn(move |_: ShotContext| Ok(ending), decodes_sends_then_ends);
        let mut decoder = compose(decoder, AnswersAsTold).expect("steps fit steps");
        let tags = [105].iter().chain(ending.readouts());
        let expected: Vec<_> = (tags.map(|&tag| (1, tag, both))).collect();
        assert_eq!(
            handed_over_as_it_ends(&mut decoder, ending),
            expected,
            "{ending:?}"
        );
    }
}

/// A layer with a state per shot sends shot 1's readout below, which it
/// keeps until the shot's run, finds the shot rejected, then runs a round
/// below, before which the send goes down; then the call fails or is
/// cancelled below, in turn, each way it can: by the run's reply, by the
/// send's in the call or after it, failed or cancelled, or by the run
/// refused after the send went down. Whichever, the send's readout that
/// the reply hands over carries the rejection.
#[test]
fn with_a_state_per_shot_however_a_call_fails_or_is_cancelled_below_it_marks_what_it_hands_over_with_the_conditions_its_logic_raised()
 {
    fn sends_rejects_then_runs(
        ending: &mut CallEnding,
        _: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        match *ending {
            CallEnding::ByASendInTheCall(how) => below.send(&[Step::Read(0), Step::Ends(how)])?,
            CallEnding::ByASendAfterTheCall(how) => {
                below.send(&[Step::Read(0), Step::AfterTheCall, Step::Ends(how)])?;
            }
            _ => below.send(&[Step::Read(0)])?,
        }
        outcomes.raise_conditions(ShotConditions::REJECTED);
        match *ending {
            CallEnding::ByARun(how) => below.run(&[Step::Read(1), Step::Ends(how)]),
            CallEnding::ByARefusalBelow => below.run(&[Step::Refused]),
            _ => below.run(&[Step::Read(1)]),
        }
        .map(drop)
    }
    for ending in [
        CallEnding::ByARun(Ending::Failed),
        CallEnding::ByARun(Ending::Cancelled),
        CallEnding::ByASendInTheCall(Ending::Failed),
        CallEnding::ByASendInTheCall(Ending::Cancelled),
        CallEnding::ByASendAfterTheCall(Ending::Failed),
        CallEnding::ByASendAfterTheCall(Ending::Cancelled),
        CallEnding::ByARefusalBelow,
    ] {
        let per_shot =
            adapter_fn(move |_: ShotContext| Ok(ending), sends_rejects_then_runs).shots(OnePerShot);
        let mut layer = compose(per_shot, AnswersAsTold).expect("steps fit steps");
        assert_eq!(
            handed_over_as_it_ends(&mut layer, ending),
            [(1, 0, ShotConditions::REJECTED)],
            "{ending:?}"
        );
    }
}

/// A faulty runtime: its reply reads a One under tag 0 on each shot of the
/// call, rejected, and, in a call without shot `other`, a readout of
/// `other` too, found inconsistent, which belongs to another call.
struct ReadsAnotherShot {
    other: ShotId,
}

impl Executor<InstructionBatch<Step>> for ReadsAnotherShot {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, Step>) -> Result<Self::Reply, Rejected> {
        let mut read: Vec<_> = (batch.shots.iter())
            .map(|&shot| {
                ShotOutcome::new(shot, 0, Value::One).with_conditions(ShotConditions::REJECTED)
            })
            .collect();
        if !batch.shots.contains(&self.other) {
            let stray = ShotOutcome::new(self.other, 0, Value::One);
            read.push(stray.with_conditions(ShotConditions::INCONSISTENT));
        }
        Ok(ImmediateReply::done(read))
    }
}

impl Describe for ReadsAnotherShot {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Close for ReadsAnotherShot {
    type Report = ();

    fn close(&mut self) {}
}

/// A decoder with a state per shot, on shots 2 and 1, writes each shot's
/// readout of an earlier round, then runs it below alone, on a faulty
/// runtime that rejects each shot it runs and whose run of shot 1 reads
/// shot 2 too, inconsistent: the call fails, as the run read a shot it was
/// not given, and its reply hands over both readouts the decoder wrote,
/// rejected, as each run read its own shot, but neither inconsistent, as
/// that readout of shot 2 may be another call's.
#[test]
fn a_failed_run_takes_in_no_condition_it_read_on_a_shot_it_was_not_given() {
    let per_shot = adapter_fn(no_state, writes_then_runs).shots(OnePerShot);
    let mut decoder = compose(per_shot, ReadsAnotherShot { other: 2 }).expect("steps fit steps");
    let reply = decoder.call(InstructionBatchRef {
        shots: &shots(&[2, 1]),
        instructions: &[Step::Read(0)],
    });
    let mut outcomes = Vec::new();
    let ended = wait(reply.expect("the decoder accepts the batch"), &mut outcomes);
    ended.expect_err("shot 1's run reads shot 2, which it was not given");
    assert_eq!(
        marked(&outcomes),
        [
            (2, 100, ShotConditions::REJECTED),
            (1, 100, ShotConditions::REJECTED)
        ]
    );
}

/// A decoder with one state, on shots 0 and 2, writes each shot's readout
/// of an earlier round, then runs both below, on a faulty runtime that
/// rejects each shot it runs and reads shot 1 too, inconsistent: the call
/// fails, as the run read a shot it was not given, and its reply hands over
/// both readouts the decoder wrote, rejected, shot 2's too, though in shot
/// order the run read shot 1 between shots 0 and 2, and nothing on shot 1.
#[test]
fn a_failed_run_takes_in_its_own_shots_conditions_read_after_a_shot_it_was_not_given() {
    let mut decoder = compose(
        adapter_fn(no_state, writes_then_runs),
        ReadsAnotherShot { other: 1 },
    )
    .expect("steps fit steps");
    let reply = decoder.call(InstructionBatchRef {
        shots: &shots(&[0, 2]),
        instructions: &[Step::Read(0)],
    });
    let mut outcomes = Vec::new();
    let ended = wait(reply.expect("the decoder accepts the batch"), &mut outcomes);
    ended.expect_err("the run reads shot 1, which it was not given");
    assert_eq!(
        marked(&outcomes),
        [
            (0, 100, ShotConditions::REJECTED),
            (2, 100, ShotConditions::REJECTED)
        ]
    );
}

/// A layer sends a call on shot 1 below, to a faulty runtime whose reply
/// reads shot 2 too, inconsistent; in a later call on shot 2, with nothing
/// to send, it writes a result of its own: that result carries the
/// condition, as a send's reply passes up as it comes, unchecked, and the
/// adapter takes in what a send reads during the call on whatever shot
/// each readout names (unlike a failed run's).
#[test]
fn a_condition_a_send_reads_on_a_shot_it_was_not_given_marks_that_shots_outcomes_in_later_calls() {
    fn sends_or_writes(
        (): &mut (),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if input.instructions().is_empty() {
            outcomes.push(Outcome::new(5, Value::One));
            return Ok(());
        }
        below.send(input.instructions())
    }
    let mut layer = compose(
        adapter_fn(no_state, sends_or_writes),
        ReadsAnotherShot { other: 2 },
    )
    .expect("steps fit steps");
    let first = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&first),
        [
            (1, 0, ShotConditions::REJECTED),
            (2, 0, ShotConditions::INCONSISTENT)
        ],
        "the send's reply passes up shot 2's readout too"
    );
    let outcomes = outcomes_of(&mut layer, &[2], &[]);
    assert_eq!(marked(&outcomes), [(2, 5, ShotConditions::INCONSISTENT)]);
}

/// A layer passes a call on shot 2 below, where the runtime rejects each
/// shot of a call; it then passes a call on shot 1, to which the faulty
/// runtime's reply adds a readout of shot 2, inconsistent: that readout
/// passes up with the condition it carries from below alone, without shot
/// 2's rejection, as the adapter marks the outcomes of the call's shots
/// alone.
#[test]
fn a_sends_readout_of_a_shot_the_call_was_not_given_passes_up_with_its_own_conditions_alone() {
    let mut layer = compose(adapter_fn(no_state, pass), ReadsAnotherShot { other: 2 })
        .expect("steps fit steps");
    let first = outcomes_of(&mut layer, &[2], &[Step::Read(0)]);
    assert_eq!(
        marked(&first),
        [(2, 0, ShotConditions::REJECTED)],
        "the runtime rejects shot 2"
    );
    let outcomes = outcomes_of(&mut layer, &[1], &[Step::Read(0)]);
    assert_eq!(
        marked(&outcomes),
        [
            (1, 0, ShotConditions::REJECTED),
            (2, 0, ShotConditions::INCONSISTENT)
        ]
    );
}

/// A layer with a state per shot, which knows its shot, rejects shot 2
/// alone, in a first call on shots 1 to 3 that reads nothing; it decodes
/// a later call: only shot 2's readout carries the rejection.
#[test]
fn with_a_state_per_shot_a_condition_a_logic_raises_marks_its_own_shots_outcomes_alone() {
    fn rejects_shot_2_first(
        (shot, calls): &mut (ShotId, u32),
        input: &CallInput<'_, Step>,
        below: &mut Below<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), AdapterError> {
        if *shot == 2 && *calls == 0 {
            outcomes.raise_conditions(ShotConditions::REJECTED);
        }
        *calls += 1;
        decode(&mut (), input, below, outcomes)
    }
    let per_shot = adapter_fn(
        |context: ShotContext| Ok((context.shot(), 0)),
        rejects_shot_2_first,
    )
    .shots(OnePerShot);
    let mut layer = compose(per_shot, device()).expect("steps fit steps");
    let first = outcomes_of(&mut layer, &[1, 2, 3], &[]);
    assert!(first.is_empty(), "the first call reads nothing");
    let outcomes = outcomes_of(&mut layer, &[1, 2, 3], &[Step::Read(0)]);
    assert_eq!(
        marked(&outcomes),
        [
            (1, 100, ShotConditions::NONE),
            (2, 100, ShotConditions::REJECTED),
            (3, 100, ShotConditions::NONE)
        ]
    );
}

/// A decoder over a code's runtime that rejects shot 1 in a call, and
/// keeps nothing from call to call: the decoder's readouts of shot 1 in
/// the next call carry the rejection, by the conformance kit's check.
#[test]
fn a_decoder_keeps_each_shots_conditions_from_one_call_to_the_next() {
    let mut decoder = compose(adapter_fn(no_state, decode), device()).expect("steps fit steps");
    conformance::keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut decoder,
        &shots(&[0, 1, 2]),
        &[
            &[Step::Raise(1, ShotConditions::REJECTED), Step::Read(0)],
            &[Step::Read(1)],
        ],
    );
}

/// A profiler over a code's runtime that rejects shot 1 in a call: the
/// readouts of shot 1 it passes up in the next call carry the rejection,
/// by the conformance kit's check.
#[test]
fn an_adapter_that_sends_keeps_each_shots_conditions_from_one_call_to_the_next() {
    let mut profiled = compose(adapter_fn(no_state, pass), device()).expect("steps fit steps");
    conformance::keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut profiled,
        &shots(&[0, 1, 2]),
        &[
            &[Step::Raise(1, ShotConditions::REJECTED), Step::Read(0)],
            &[Step::Read(1)],
        ],
    );
}

/// A profiler with a state per shot over a code's runtime that rejects
/// shot 1 in a call: the readouts of shot 1 it passes up in the next call
/// carry the rejection, by the conformance kit's check.
#[test]
fn with_a_state_per_shot_an_adapter_keeps_each_shots_conditions_from_one_call_to_the_next() {
    let per_shot = adapter_fn(no_state, pass).shots(OnePerShot);
    let mut profiled = compose(per_shot, device()).expect("steps fit steps");
    conformance::keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut profiled,
        &shots(&[0, 1, 2]),
        &[
            &[Step::Raise(1, ShotConditions::REJECTED), Step::Read(0)],
            &[Step::Read(1)],
        ],
    );
}
