// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An executor of many shots, a state per shot, modelled on the per-shot
//! MPS executor: its logic alone (`per_shot_executor/bond_limited.rs`),
//! made an executor by `executor_fn`, given the run's seed by `.seed(seed)`
//! and a state per shot by `.shots(OnePerShot)`, driven by a program with
//! feedback. Walked through in
//! [Making an executor](../docs/executors.md#making-an-executor-executor_fn).

#[path = "per_shot_executor/bond_limited.rs"]
mod bond_limited;
#[allow(
    dead_code,
    reason = "these tests run the toy's state, not its simulator"
)]
#[path = "toy/simulator.rs"]
mod toy;

use std::collections::BTreeMap;

use executor_protocol::{
    AdaptiveProgram, Capabilities, Close, Describe, Executor, InstructionBatch,
    InstructionBatchRef, Needs, OnePerShot, OwnThread, Rejected, ShotContext, ShotId, ShotOutcome,
    ShotSet, Value, conformance, driver, executor_fn, wait,
};

use bond_limited::{MAX_BOND, MAX_SHOTS, bond_limited_executor};
use toy::{Op, QUBITS, ToyState};

const COIN: [Op; 2] = [Op::Coin(0), Op::Measure { qubit: 0, tag: 0 }];

fn shots(ids: &[ShotId]) -> ShotSet {
    ShotSet::new(ids.to_vec()).expect("the tests' shots are distinct")
}

/// Calls `executor` with `batch` on `shots`, and waits for the outcomes.
fn run(
    executor: &mut impl Executor<InstructionBatch<Op>>,
    shots: &ShotSet,
    batch: &[Op],
) -> Vec<ShotOutcome> {
    let reply = executor
        .call(InstructionBatchRef {
            shots,
            instructions: batch,
        })
        .expect("the executor takes the batch");
    let mut outcomes = Vec::new();
    wait(reply, &mut outcomes).expect("the batch stays within the capacity");
    outcomes
}

/// A program with feedback: toss a coin on every shot, then, on the shots
/// whose coin showed 1, toss a second coin and read the first again.
struct TossAgainOnOne;

impl AdaptiveProgram for TossAgainOnOne {
    type Instruction = Op;
    type Output = Vec<ShotOutcome>;

    fn needs(&self) -> Needs {
        Needs::default()
            .with_qubits(2)
            .with_resumable_shots(true)
            .with_log_probability(true)
    }

    async fn drive<E: Executor<InstructionBatch<Op>>>(
        self,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Vec<ShotOutcome> {
        let mut outcomes = Vec::new();
        let first = executor.call(InstructionBatchRef {
            shots,
            instructions: &COIN,
        });
        wait(first.expect("the executor takes a coin"), &mut outcomes).expect("one coin fits");
        let ones = shots.subset(|j| {
            outcomes
                .iter()
                .any(|o| o.shot == j && o.value == Value::One)
        });
        if let Some(ones) = ones {
            let second = [Op::Coin(1), Op::Measure { qubit: 0, tag: 1 }];
            let reply = executor.call(InstructionBatchRef {
                shots: &ones,
                instructions: &second,
            });
            wait(reply.expect("the executor takes a coin"), &mut outcomes).expect("one coin fits");
        }
        outcomes
    }
}

/// Each shot's first coin, from a run's outcomes.
fn first_coins(outcomes: &[ShotOutcome]) -> BTreeMap<ShotId, Value> {
    outcomes
        .iter()
        .filter(|o| o.tag == 0)
        .map(|o| (o.shot, o.value))
        .collect()
}

/// A program reads the first coin again on the shots whose coin showed 1:
/// only those shots run the second batch, on the state their first left.
#[test]
fn a_program_with_feedback_runs_its_next_batch_on_the_shots_it_chose() {
    let shots = ShotSet::range(0..=7).expect("not empty");
    let executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    let (outcomes, _) = (driver() >> executor)
        .expect("nothing between to refuse")
        .run(TossAgainOnOne, shots)
        .expect("the executor has resumable shots, log probability, 8 shots")
        .wait();
    let ones: Vec<_> = (first_coins(&outcomes).into_iter())
        .filter_map(|(j, value)| (value == Value::One).then_some(j))
        .collect();
    assert!(!ones.is_empty() && ones.len() < 8, "both sides: {ones:?}");
    let again: Vec<_> = outcomes.iter().filter(|o| o.tag == 1).collect();
    assert_eq!(again.iter().map(|o| o.shot).collect::<Vec<_>>(), ones);
    assert!(again.iter().all(|o| o.value == Value::One), "{again:?}");
}

/// The report of a run gives each shot the log probability of its own
/// coins: ln ½ for a shot that tossed one, ln ¼ for one that tossed two.
#[test]
fn a_run_reports_each_shots_log_probability() {
    let shots = ShotSet::range(0..=7).expect("not empty");
    let (outcomes, report) = (driver() >> bond_limited_executor(None).seed(3).shots(OnePerShot))
        .expect("nothing between to refuse")
        .run(TossAgainOnOne, shots)
        .expect("the executor has resumable shots, log probability, 8 shots")
        .wait();
    let firsts = first_coins(&outcomes);
    assert!(
        [Value::Zero, Value::One]
            .iter()
            .all(|v| firsts.iter().any(|(_, first)| first == v)),
        "some shot tossed one coin, and some two: {firsts:?}"
    );
    for (j, first) in firsts {
        let coins = if first == Value::One { 2.0 } else { 1.0 };
        assert_eq!(report.log_p.get(j), Some(coins * 0.5_f64.ln()), "shot {j}");
    }
}

/// The run of a program with feedback gives the same outcomes and report
/// whether the shots' states run on the driver's thread, together on one
/// thread of their own, or each on a thread of its own: where an executor
/// runs is not what it computes.
#[test]
fn a_run_gives_the_same_outcomes_and_report_wherever_the_shots_states_run() {
    let shots = ShotSet::range(0..=7).expect("not empty");
    let sorted = |(mut outcomes, report): (Vec<ShotOutcome>, _)| {
        outcomes.sort_by_key(|o| (o.shot, o.tag));
        (outcomes, report)
    };
    let here = (driver() >> bond_limited_executor(None).seed(3).shots(OnePerShot))
        .expect("nothing between to refuse")
        .run(TossAgainOnOne, shots.clone())
        .expect("the executor has resumable shots, log probability, 8 shots")
        .wait();
    let one_thread = (driver()
        >> bond_limited_executor(None)
            .seed(3)
            .shots(OnePerShot)
            .run_on(OwnThread))
    .expect("nothing between to refuse")
    .run(TossAgainOnOne, shots.clone())
    .expect("placed, the executor still holds 8 shots at once")
    .wait();
    let a_thread_each = (driver()
        >> bond_limited_executor(None)
            .seed(3)
            .run_on(OwnThread)
            .shots(OnePerShot))
    .expect("nothing between to refuse")
    .run(TossAgainOnOne, shots)
    .expect("placed per shot, the executor still holds 8 shots at once")
    .wait();
    let here = sorted(here);
    assert_eq!(sorted(one_thread), here, "on one thread of their own");
    assert_eq!(sorted(a_thread_each), here, "each on a thread of its own");
}

/// A shot's log probability counts its coins only: a coin read twice is
/// still one coin, of probability ½.
#[test]
fn a_shots_log_probability_counts_its_coins_not_its_reads() {
    let mut executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    let batch = [
        Op::Coin(0),
        Op::Measure { qubit: 0, tag: 0 },
        Op::Measure { qubit: 0, tag: 1 },
    ];
    run(&mut executor, &shots(&[0]), &batch);
    let report = executor.close();
    assert_eq!(report.log_p.get(0), Some(0.5_f64.ln()));
}

/// Shot `j` draws from the last executor's seed in shot `j`, so its coin is
/// the one a toy state drawing from that seed tosses, alone.
#[test]
fn each_shot_draws_from_the_last_executors_seed() {
    let seed = 3;
    let all = ShotSet::range(0..=7).expect("0..=7 is not empty");
    let outcomes = run(
        &mut bond_limited_executor(None).seed(seed).shots(OnePerShot),
        &all,
        &COIN,
    );
    let values: Vec<_> = outcomes.iter().map(|o| o.value).collect();
    assert!(
        values.contains(&Value::Zero) && values.contains(&Value::One),
        "the shots drew different coins: {values:?}"
    );
    for (&j, outcome) in all.iter().zip(&outcomes) {
        let last_executor_seed = ShotContext::last_executor(seed, j).seed();
        let mut alone = executor_fn(
            Capabilities::default(),
            |_| Ok(ToyState::new(last_executor_seed)),
            ToyState::run,
        );
        let drawn = run(&mut alone, &ShotSet::one(j), &COIN);
        assert_eq!(outcome.value, drawn[0].value, "shot {j}");
    }
}

/// A device busy when shot 1's state is first made refuses the call on
/// shots 0 and 1, with the state closure's refusal, and takes it when made
/// again, once the device is free.
#[test]
fn a_busy_device_refuses_the_call_and_prepares_the_state_at_the_next() {
    let mut executor = bond_limited_executor(Some(1)).seed(3).shots(OnePerShot);
    let both = shots(&[0, 1]);
    let refused = executor.call(InstructionBatchRef {
        shots: &both,
        instructions: &COIN,
    });
    let reason = "the device is busy".into();
    assert_eq!(refused.err(), Some(Rejected::Unavailable(reason)));
    assert_eq!(run(&mut executor, &both, &COIN).len(), 2);
}

/// A batch on a qubit the device lacks is refused by the example's check.
#[test]
fn a_batch_on_a_qubit_the_device_lacks_is_refused() {
    let mut executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    let refused = executor.call(InstructionBatchRef {
        shots: &shots(&[0]),
        instructions: &[Op::X(QUBITS)],
    });
    assert!(
        matches!(refused, Err(Rejected::Instruction { at: 0, .. })),
        "{refused:?}"
    );
}

/// Calls `executor` with `batch` on `shots`, and gives the outcomes and how
/// the reply ended.
fn try_run(
    executor: &mut impl Executor<InstructionBatch<Op>>,
    shots: &ShotSet,
    batch: &[Op],
) -> (Vec<ShotOutcome>, Result<(), String>) {
    let reply = executor
        .call(InstructionBatchRef {
            shots,
            instructions: batch,
        })
        .expect("the capacity is checked at the batch's end, not refused");
    let mut outcomes = Vec::new();
    let ended = wait(reply, &mut outcomes);
    (outcomes, ended)
}

/// Two `Cx`s grow shot 0's bond to the capacity, which it may reach; an
/// `X` does not grow it.
#[test]
fn a_bond_equal_to_the_capacity_fits() {
    let mut executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    let batch = [Op::Cx(0, 1), Op::X(0), Op::Cx(1, 2)];
    let (_, ended) = try_run(&mut executor, &shots(&[0]), &batch);
    assert_eq!(ended, Ok(()));
}

/// A shot's bond grows across its batches: after two `Cx`s, a third, in a
/// later batch, outgrows the capacity.
#[test]
fn a_shots_bond_grows_across_its_batches() {
    let mut executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    let shot_0 = shots(&[0]);
    run(&mut executor, &shot_0, &[Op::Cx(0, 1), Op::Cx(1, 2)]);
    let (_, ended) = try_run(&mut executor, &shot_0, &[Op::Cx(2, 3)]);
    assert_eq!(
        ended,
        Err(format!("bond 3 outgrew the capacity {MAX_BOND}"))
    );
}

/// A batch whose bond outgrows the capacity fails, after handing over the
/// outcomes it read before.
#[test]
fn a_bond_beyond_the_capacity_fails_the_batch_after_its_outcomes() {
    let mut executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    let read_then_entangle = [COIN[0], COIN[1], ENTANGLE[0], ENTANGLE[1], ENTANGLE[2]];
    let (outcomes, ended) = try_run(&mut executor, &shots(&[0]), &read_then_entangle);
    assert_eq!(
        outcomes.iter().map(|o| (o.shot, o.tag)).collect::<Vec<_>>(),
        [(0, 0)]
    );
    assert_eq!(
        ended,
        Err(format!("bond 3 outgrew the capacity {MAX_BOND}"))
    );
}

/// Three `Cx`s grow a bond beyond the capacity.
const ENTANGLE: [Op; 3] = [Op::Cx(0, 1), Op::Cx(1, 2), Op::Cx(2, 3)];

/// A batch that grows shot 0's bond beyond the capacity fails, and the
/// executor refuses every later call, on any shot: the conformance kit's
/// check that a failure is final.
#[test]
fn a_batch_that_outgrows_the_capacity_ends_the_executor() {
    let mut executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    let (shot_0, shot_1) = (shots(&[0]), shots(&[1]));
    conformance::refuses_calls_after_a_failure::<InstructionBatch<Op>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &shot_0,
            instructions: &ENTANGLE,
        },
        &[
            InstructionBatchRef {
                shots: &shot_1,
                instructions: &COIN,
            },
            InstructionBatchRef {
                shots: &shot_0,
                instructions: &COIN,
            },
        ],
    );
}

/// A batch grows shot 0's bond beyond the capacity, and a call on shot 1
/// follows before the caller waits for the batch's reply: that call is
/// refused, or ends failed, naming the capacity, rather than run on an
/// executor that has ended: the conformance kit's check.
#[test]
fn a_call_in_flight_when_a_batch_outgrows_the_capacity_ends_with_that_failure() {
    let mut executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    let (shot_0, shot_1) = (shots(&[0]), shots(&[1]));
    conformance::ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<Op>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &shot_0,
            instructions: &ENTANGLE,
        },
        InstructionBatchRef {
            shots: &shot_1,
            instructions: &COIN,
        },
    );
}

/// The executor describes what it offers: the toy's qubits, the shots its
/// device holds at once, and the log probability, since it reads each state's
/// log probability.
#[test]
fn the_executor_describes_its_qubits_shots_and_log_probability() {
    let offered = bond_limited_executor(None)
        .seed(3)
        .shots(OnePerShot)
        .capabilities();
    assert_eq!(
        (offered.qubits, offered.max_shots, offered.log_probability),
        (Some(QUBITS), Some(MAX_SHOTS), true)
    );
}

/// A call on more shots than the device holds at once is refused,
/// naming the first shot it has no room for, and runs nothing; a call on as
/// many shots as it holds runs.
#[test]
fn a_call_beyond_the_shots_the_device_holds_is_refused() {
    let mut executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    let nine = ShotSet::range(0..=8).expect("0..=8 is not empty");
    let refused = executor.call(InstructionBatchRef {
        shots: &nine,
        instructions: &COIN,
    });
    assert_eq!(refused.err(), Some(Rejected::ShotNotHeld(8)));
    let eight = ShotSet::range(0..=7).expect("0..=7 is not empty");
    assert_eq!(run(&mut executor, &eight, &COIN).len(), 8);
}

/// The executor keeps the conformance kit's rule on closing: once closed,
/// it refuses calls.
#[test]
fn a_closed_executor_refuses_calls() {
    let mut executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    conformance::rejects_calls_once_closed::<InstructionBatch<Op>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &shots(&[0]),
            instructions: &COIN,
        },
    );
}

/// The executor keeps the conformance kit's rule on reports: after a call,
/// it reports once, and nothing when closed again.
#[test]
fn the_executor_reports_once() {
    let mut executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    run(&mut executor, &shots(&[0]), &COIN);
    conformance::reports_once(&mut executor);
}

/// The executor keeps the conformance kit's rules on readout tags: it
/// refuses a batch that repeats a tag, at the instruction that repeats it,
/// and takes one that reuses a tag of a batch before.
#[test]
fn a_tag_repeated_within_a_batch_is_refused() {
    let mut executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    let (a, b, a_again) = (
        Op::Measure { qubit: 0, tag: 0 },
        Op::Measure { qubit: 1, tag: 1 },
        Op::Measure { qubit: 2, tag: 0 },
    );
    let shot_0 = shots(&[0]);
    conformance::refuses_a_tag_repeated_within_a_batch(&mut executor, &shot_0, &a, &b, &a_again);
    conformance::refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut executor,
        &shot_0,
        &a,
        &Op::X(1),
        &a_again,
    );
}

/// The executor keeps the conformance kit's rule on refusals: a batch on a
/// qubit the device lacks is refused, and the corrected batch runs.
#[test]
fn a_refused_call_leaves_the_executor_usable() {
    let mut executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
    let shot_0 = shots(&[0]);
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<Op>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &shot_0,
            instructions: &[Op::X(QUBITS)],
        },
        InstructionBatchRef {
            shots: &shot_0,
            instructions: &COIN,
        },
    );
}

/// The executor keeps the conformance kit's promise of `replayable`: each
/// of shots 0 to 3 tosses alone, and with the shots split into calls in
/// another order, the two coins it tosses among the others.
#[test]
fn each_shot_alone_gives_what_it_gives_among_others() {
    let coin_1 = [Op::Coin(1), Op::Measure { qubit: 1, tag: 1 }];
    conformance::gives_each_shot_alone_what_it_gives_among_others(
        || bond_limited_executor(None).seed(3).shots(OnePerShot),
        &ShotSet::range(0..=3).expect("not empty"),
        &[&COIN, &coin_1],
    );
}

/// The executor keeps the conformance kit's promise of `resumable_shots`: a
/// second call reads the flip of the first, on each of shots 0 and 1.
#[test]
fn a_second_call_continues_each_shot() {
    conformance::continues_a_shot_from_one_call_to_the_next(
        || bond_limited_executor(None).seed(3).shots(OnePerShot),
        &shots(&[0, 1]),
        &[Op::X(0)],
        &[Op::Measure { qubit: 0, tag: 0 }],
    );
}

/// A flip on shots 1 and 3 leaves shots 0 and 2, flipped before, as they
/// were: the conformance kit's check that a call changes no shot it does
/// not name.
#[test]
fn a_call_changes_no_shot_it_does_not_name() {
    conformance::changes_no_shot_a_call_does_not_name(
        || bond_limited_executor(None).seed(3).shots(OnePerShot),
        &shots(&[0, 1, 2, 3]),
        &[Op::X(0)],
        &[Op::Measure { qubit: 0, tag: 0 }],
    );
}

/// After an executor flipped shots 0 to 3 and was closed, or dropped
/// unclosed, the next one reads them as made: the conformance kit's check
/// that a new executor starts its shots as made.
#[test]
fn a_new_executor_starts_its_shots_as_made() {
    conformance::starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || bond_limited_executor(None).seed(3).shots(OnePerShot),
        &shots(&[0, 1, 2, 3]),
        &[Op::X(0)],
        &[Op::Measure { qubit: 0, tag: 0 }],
    );
}
