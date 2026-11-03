// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How an `executor_fn`'s logic raises a shot's conditions on the outcomes
//! it writes (`Outcome::with_conditions`, `Outcomes::raise_conditions`),
//! with one state or a state per shot, and keeps none from call to call;
//! on the device of conditions of the adapters' tests.

use crate::reply::blocking::wait_status;
use crate::tests::facilities::adapter_fn::conditions::{
    Step, device, marked, no_state, outcomes_of,
};
use crate::tests::shots;
use crate::{
    CallInput, Capabilities, Executor, InstructionBatchRef, OnePerShot, Outcome, Outcomes,
    Rejected, ReplyStatus, ShotConditions, ShotContext, ShotId, ShotOutcome, Value, conformance,
    executor_fn,
};

/// A code's runtime reads shot 1 twice, finds it rejected, then reads it
/// twice more: all four readouts carry it, the two written before as
/// well.
#[test]
fn a_condition_a_logic_raises_marks_every_outcome_it_writes_in_the_call() {
    let batch = [
        Step::Read(0),
        Step::Read(1),
        Step::Raise(1, ShotConditions::REJECTED),
        Step::Read(2),
        Step::Read(3),
    ];
    let outcomes = outcomes_of(&mut device(), &[1], &batch);
    let rejected = ShotConditions::REJECTED;
    assert_eq!(
        marked(&outcomes),
        [
            (1, 0, rejected),
            (1, 1, rejected),
            (1, 2, rejected),
            (1, 3, rejected)
        ]
    );
}

/// A code's runtime rejects shot 1, reads it, then finds its syndromes
/// inconsistent and reads it again: both readouts carry both conditions,
/// as the conditions a logic raises in a call merge by union.
#[test]
fn the_conditions_a_logic_raises_twice_in_a_call_merge_by_union() {
    let batch = [
        Step::Raise(1, ShotConditions::REJECTED),
        Step::Read(0),
        Step::Raise(1, ShotConditions::INCONSISTENT),
        Step::Read(1),
    ];
    let outcomes = outcomes_of(&mut device(), &[1], &batch);
    let both = ShotConditions::REJECTED | ShotConditions::INCONSISTENT;
    assert_eq!(marked(&outcomes), [(1, 0, both), (1, 1, both)]);
}

/// A code's runtime rejects its shot, then writes a readout it built
/// for that shot: the readout carries the rejection.
#[test]
fn a_condition_a_logic_raises_marks_the_outcomes_it_writes_for_their_shots() {
    let mut runtime = executor_fn(
        Capabilities::default(),
        |context: ShotContext| Ok::<_, Rejected>(context.shot()),
        |shot: &mut ShotId, _: &CallInput<'_, Step>, outcomes: &mut Outcomes<'_>| {
            outcomes.raise_conditions(ShotConditions::REJECTED);
            outcomes.extend([ShotOutcome::new(*shot, 0, Value::One)]);
            Ok(())
        },
    )
    .shots(OnePerShot);
    let outcomes = outcomes_of(&mut runtime, &[1], &[]);
    assert_eq!(marked(&outcomes), [(1, 0, ShotConditions::REJECTED)]);
}

/// A code's runtime rejects its shot, then writes the three readouts it
/// decoded for that shot at once: each carries the rejection.
#[test]
fn every_outcome_a_logic_writes_for_its_shot_at_once_carries_the_conditions_it_raised() {
    let mut runtime = executor_fn(
        Capabilities::default(),
        |context: ShotContext| Ok::<_, Rejected>(context.shot()),
        |shot: &mut ShotId, _: &CallInput<'_, Step>, outcomes: &mut Outcomes<'_>| {
            outcomes.raise_conditions(ShotConditions::REJECTED);
            outcomes.extend((0..3).map(|tag| ShotOutcome::new(*shot, tag, Value::One)));
            Ok(())
        },
    )
    .shots(OnePerShot);
    let outcomes = outcomes_of(&mut runtime, &[1], &[]);
    let rejected = ShotConditions::REJECTED;
    assert_eq!(
        marked(&outcomes),
        [(1, 0, rejected), (1, 1, rejected), (1, 2, rejected)]
    );
}

/// A code's runtime writes a readout it built for its shot with the
/// rejection it found on it: the readout carries it.
#[test]
fn the_conditions_a_logic_writes_on_an_outcome_for_its_shot_reach_its_caller() {
    let mut runtime = executor_fn(
        Capabilities::default(),
        |context: ShotContext| Ok::<_, Rejected>(context.shot()),
        |shot: &mut ShotId, _: &CallInput<'_, Step>, outcomes: &mut Outcomes<'_>| {
            let readout = ShotOutcome::new(*shot, 0, Value::One);
            outcomes.extend([readout.with_conditions(ShotConditions::REJECTED)]);
            Ok(())
        },
    )
    .shots(OnePerShot);
    let outcomes = outcomes_of(&mut runtime, &[1], &[]);
    assert_eq!(marked(&outcomes), [(1, 0, ShotConditions::REJECTED)]);
}

/// A code's runtime writes a readout of its shot that it found
/// inconsistent, then rejects the shot: the readout carries both, as a
/// condition raised joins those an outcome was written with rather than
/// replacing them.
#[test]
fn an_outcome_written_with_a_condition_keeps_it_when_the_logic_raises_another() {
    let mut runtime = executor_fn(
        Capabilities::default(),
        |context: ShotContext| Ok::<_, Rejected>(context.shot()),
        |_: &mut ShotId, _: &CallInput<'_, Step>, outcomes: &mut Outcomes<'_>| {
            outcomes
                .push(Outcome::new(0, Value::One).with_conditions(ShotConditions::INCONSISTENT));
            outcomes.raise_conditions(ShotConditions::REJECTED);
            Ok(())
        },
    )
    .shots(OnePerShot);
    let outcomes = outcomes_of(&mut runtime, &[1], &[]);
    let both = ShotConditions::REJECTED | ShotConditions::INCONSISTENT;
    assert_eq!(marked(&outcomes), [(1, 0, both)]);
}

/// A code's runtime, one state, writes a readout of shot 1 it found
/// inconsistent, rejects the shot, writes a second readout, then loses
/// its device: the call fails, and its reply hands over both readouts,
/// the first with both conditions and the second rejected, as a failed
/// call keeps the conditions its logic wrote and raised.
#[test]
fn a_call_its_logic_fails_hands_over_what_it_wrote_with_the_conditions_written_and_raised() {
    fn writes_rejects_then_fails(
        (): &mut (),
        _: &CallInput<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), String> {
        outcomes.push(Outcome::new(0, Value::One).with_conditions(ShotConditions::INCONSISTENT));
        outcomes.raise_conditions(ShotConditions::REJECTED);
        outcomes.push(Outcome::new(1, Value::One));
        Err("the runtime lost its device".into())
    }
    let mut runtime = executor_fn(Capabilities::default(), no_state, writes_rejects_then_fails);
    let reply = runtime.call(InstructionBatchRef {
        shots: &shots(&[1]),
        instructions: &[],
    });
    let mut outcomes = Vec::new();
    let ended = wait_status(reply.expect("the runtime accepts the batch"), &mut outcomes);
    assert!(
        matches!(ended, ReplyStatus::Failed(_)),
        "the logic fails the call"
    );
    let both = ShotConditions::REJECTED | ShotConditions::INCONSISTENT;
    assert_eq!(
        marked(&outcomes),
        [(1, 0, both), (1, 1, ShotConditions::REJECTED)]
    );
}

/// A code's runtime, a state per shot, given shots 9, 1 and 6: each shot
/// writes a readout with a condition of its own, unresolved on shot 9 and
/// inconsistent on the others, and rejects itself; shot 1's then loses
/// its state, so shot 6 never runs. The call fails, and its reply hands
/// over the readouts of shots 9 and 1, each with its own shot's
/// conditions, as a failed call keeps those its shots' logic wrote and
/// raised.
#[test]
fn with_a_state_per_shot_a_call_a_shots_logic_fails_hands_over_what_each_shot_wrote_with_its_own_conditions()
 {
    fn writes_rejects_then_fails_on_shot_1(
        shot: &mut ShotId,
        _: &CallInput<'_, Step>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), String> {
        let found = if *shot == 9 {
            ShotConditions::UNRESOLVED
        } else {
            ShotConditions::INCONSISTENT
        };
        outcomes.push(Outcome::new(0, Value::One).with_conditions(found));
        outcomes.raise_conditions(ShotConditions::REJECTED);
        if *shot == 1 {
            return Err("shot 1 lost its state".into());
        }
        Ok(())
    }
    let mut runtime = executor_fn(
        Capabilities::default(),
        |context: ShotContext| Ok::<_, Rejected>(context.shot()),
        writes_rejects_then_fails_on_shot_1,
    )
    .shots(OnePerShot);
    let reply = runtime.call(InstructionBatchRef {
        shots: &shots(&[9, 1, 6]),
        instructions: &[],
    });
    let mut outcomes = Vec::new();
    let ended = wait_status(reply.expect("the runtime accepts the batch"), &mut outcomes);
    assert!(
        matches!(ended, ReplyStatus::Failed(_)),
        "shot 1's logic fails the call"
    );
    assert_eq!(
        marked(&outcomes),
        [
            (9, 0, ShotConditions::REJECTED | ShotConditions::UNRESOLVED),
            (
                1,
                0,
                ShotConditions::REJECTED | ShotConditions::INCONSISTENT
            )
        ]
    );
}

/// A code's runtime, a state per shot, rejects shot 2 of a call on
/// shots 1 to 3, whose readouts share the call's reply: only shot 2's
/// readout carries it.
#[test]
fn a_condition_raised_on_one_shot_marks_no_other_shots_outcomes() {
    let batch = [Step::Read(0), Step::Raise(2, ShotConditions::REJECTED)];
    let outcomes = outcomes_of(&mut device(), &[1, 2, 3], &batch);
    assert_eq!(
        marked(&outcomes),
        [
            (1, 0, ShotConditions::NONE),
            (2, 0, ShotConditions::REJECTED),
            (3, 0, ShotConditions::NONE)
        ]
    );
}

/// A code's runtime made with `executor_fn` rejects shot 1 in a first
/// call, then reads it in a second: the second readout carries nothing,
/// as `executor_fn` keeps no condition from call to call (its logic's
/// state does, if it must).
#[test]
fn an_executor_fn_keeps_no_condition_from_call_to_call() {
    let mut runtime = device();
    let first = [Step::Raise(1, ShotConditions::REJECTED), Step::Read(0)];
    let rejected = outcomes_of(&mut runtime, &[1], &first);
    assert_eq!(
        marked(&rejected),
        [(1, 0, ShotConditions::REJECTED)],
        "the first call raises it"
    );
    let outcomes = outcomes_of(&mut runtime, &[1], &[Step::Read(1)]);
    assert_eq!(marked(&outcomes), [(1, 1, ShotConditions::NONE)]);
}

/// A code's runtime made with `executor_fn`, a state per shot, keeps in
/// its state the conditions it found on its shot and raises them again in
/// each call, as the logic of one that offers `resumable_shots` must: it
/// passes the conformance kit's check across calls.
#[test]
fn an_executor_fn_whose_logic_keeps_its_shots_conditions_in_its_state_keeps_them_from_one_call_to_the_next()
 {
    let mut runtime = executor_fn(
        Capabilities::default().with_resumable_shots(true),
        |context: ShotContext| Ok::<_, Rejected>((context.shot(), ShotConditions::NONE)),
        |(shot, kept): &mut (ShotId, ShotConditions),
         input: &CallInput<'_, Step>,
         outcomes: &mut Outcomes<'_>| {
            for &step in input.instructions() {
                match step {
                    Step::Raise(on, conditions) if on == *shot => *kept |= conditions,
                    Step::Raise(..) | Step::Refused | Step::AfterTheCall | Step::Ends(_) => {}
                    Step::Read(tag) => outcomes.push(Outcome::new(tag, Value::One)),
                }
            }
            outcomes.raise_conditions(*kept);
            Ok(())
        },
    )
    .shots(OnePerShot);
    conformance::keeps_each_shots_conditions_from_one_call_to_the_next(
        &mut runtime,
        &shots(&[0, 1, 2]),
        &[
            &[Step::Raise(1, ShotConditions::REJECTED), Step::Read(0)],
            &[Step::Read(1)],
        ],
    );
}
