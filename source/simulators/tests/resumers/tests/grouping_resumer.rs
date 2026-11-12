// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The [`QirGroupingResumer`]: the kit's rules, which shots share a
//! dispatch,
//! and the records it gives, which a run shot by shot gives too.

use qdk_simulators::qir::{
    QirMachine, QirProgram, QirResumer, QirShotId, conformance, expect_ready,
};
use qdk_simulators::{MeasurementResult, OutputRecord, QubitID};

use super::{Log, Operations, full_state};
use crate::grouping_resumer::QirGroupingResumer;
use crate::programs::{
    coin_then_a_batch_or_its_prefix, coin_then_branch, coin_then_end_or_toss,
    coin_then_read_one_of_two_qubits, coin_then_toss_into_one_of_two_results,
};
use crate::simulator_resumer::QirSimulatorResumer;

use MeasurementResult::{One, Zero};

/// The run's shots, in an order that is not the order of their ids.
const SHOTS: [u32; 4] = [5, 2, 9, 0];

/// The ids of `shots`.
fn ids(shots: &[u32]) -> Vec<QirShotId> {
    shots.iter().copied().map(QirShotId).collect()
}

/// The records of a grouped run of `program` on [`SHOTS`], each shot's
/// simulator a [`Log`] whose measurement of a qubit reads 1 where `reads`
/// says so for the shot and the qubit, and the shots of each dispatch it
/// made.
fn run(
    program: &QirProgram,
    reads: fn(u32, QubitID) -> bool,
) -> (Vec<Vec<OutputRecord>>, Vec<Vec<QirShotId>>) {
    let results = program.results() as usize;
    let mut resumer = QirGroupingResumer::new(|_: &QirProgram, shot: QirShotId| {
        Log::new_reading(results, Operations::default(), move |qubit| {
            if reads(shot.0, qubit) { One } else { Zero }
        })
    });
    let records = expect_ready(resumer.run(&QirMachine::new(program), &ids(&SHOTS)))
        .expect("the shots do not trap");
    (records, resumer.dispatches().to_vec())
}

/// The odd shots read 1, the even ones 0, at every qubit.
fn odd_shots_read_one(shot: u32, _: QubitID) -> bool {
    shot % 2 == 1
}

/// Shots on a full-state simulator each, seeded by their ids.
#[test]
fn keeps_every_rule_of_the_kit() {
    conformance::check(|| QirGroupingResumer::new(full_state));
}

/// Every shot reads 0, so all run the same batches: each round is one
/// dispatch, on every shot, in the run's order.
#[test]
fn shots_whose_next_batches_are_equal_share_one_dispatch() {
    let (_, dispatches) = run(&coin_then_branch(), |_, _| false);
    assert_eq!(dispatches, vec![ids(&SHOTS); 3]);
}

/// The odd shots read 1 at the coin, so they flip qubit 1 before reading
/// it, and the even ones do not: the second round dispatches the odd shots,
/// then the even ones, each group in the run's order.
#[test]
fn a_group_splits_where_outcomes_lead_its_shots_to_different_batches() {
    let (_, dispatches) = run(&coin_then_branch(), odd_shots_read_one);
    assert_eq!(dispatches[1..3], [ids(&[5, 9]), ids(&[2, 0])]);
}

/// The odd shots flipped qubit 1 and the even ones did not, then all toss
/// the second coin: the third round is one dispatch again, on every shot.
#[test]
fn shots_on_different_paths_share_a_dispatch_where_their_batches_are_equal() {
    let (_, dispatches) = run(&coin_then_branch(), odd_shots_read_one);
    assert_eq!(
        dispatches.len(),
        4,
        "the second round has two groups: {dispatches:?}"
    );
    assert_eq!(dispatches[3], ids(&SHOTS));
}

/// The odd shots read 1 at the coin and end; the even ones read 0 and
/// toss a second coin: the second round dispatches the even shots only,
/// and no dispatch names a shot that ended.
#[test]
fn a_shot_that_ended_is_in_no_later_dispatch() {
    let (_, dispatches) = run(&coin_then_end_or_toss(), odd_shots_read_one);
    assert_eq!(dispatches, [ids(&SHOTS), ids(&[2, 0])]);
}

/// The odd shots read 1 at the coin and read qubit 1, the even ones read
/// qubit 2, both into result 1: batches of one quantum call each, alike but for
/// the qubit, are not equal, so the second round dispatches each path's shots
/// apart.
#[test]
fn shots_whose_batches_differ_only_in_a_qubit_do_not_share_a_dispatch() {
    let (_, dispatches) = run(&coin_then_read_one_of_two_qubits(), odd_shots_read_one);
    assert_eq!(dispatches, [ids(&SHOTS), ids(&[5, 9]), ids(&[2, 0])]);
}

/// The odd shots read 1 at the coin and toss a second coin into result 1, the
/// even ones toss the same coin into result 2: batches whose first quantum
/// calls are equal, the `h`, and whose `mz` differ only in the result they
/// write are not equal, so the second round dispatches each path's shots apart.
#[test]
fn shots_whose_batches_differ_only_in_a_later_result_do_not_share_a_dispatch() {
    let (_, dispatches) = run(
        &coin_then_toss_into_one_of_two_results(),
        odd_shots_read_one,
    );
    assert_eq!(dispatches, [ids(&SHOTS), ids(&[5, 9]), ids(&[2, 0])]);
}

/// The odd shots read 1 at the coin and toss a second coin, then apply an
/// `h`; the even ones toss the same coin alone: a batch and a strict
/// prefix of it are not equal, so the second round dispatches each path's
/// shots apart.
#[test]
fn shots_whose_batches_differ_only_in_a_quantum_call_at_the_end_do_not_share_a_dispatch() {
    let (_, dispatches) = run(&coin_then_a_batch_or_its_prefix(), odd_shots_read_one);
    assert_eq!(dispatches, [ids(&SHOTS), ids(&[5, 9]), ids(&[2, 0])]);
}

/// Shot 2 reads 1 at the coin (qubit 0), so it runs other batches than
/// the rest; shot 0 reads 1 at the second coin (qubit 2), in a dispatch
/// it shares with every shot: each shot's records sit at its place in the
/// run's shots, though shot 2 runs its second batch in a dispatch apart.
#[test]
fn each_shot_s_records_sit_at_its_place_in_the_run_s_shots() {
    let (records, _) = run(&coin_then_branch(), |shot, qubit| {
        (shot, qubit) == (2, 0) || (shot, qubit) == (0, 2)
    });
    let shot = |coin, copy, second| [coin, copy, second].map(OutputRecord::Result).to_vec();
    assert_eq!(
        records,
        [
            shot(Zero, Zero, Zero),
            shot(One, Zero, Zero),
            shot(Zero, Zero, Zero),
            shot(Zero, Zero, One),
        ]
    );
}

/// Shots 39 down to 0 on full-state simulators seeded by their ids, some
/// of which read 1 at the coin and some 0, so that they split: grouped,
/// each shot gives the records it gives run shot by shot.
#[test]
fn a_grouped_run_gives_each_shot_the_records_of_a_run_shot_by_shot() {
    let program = coin_then_branch();
    let machine = QirMachine::new(&program);
    let shots: Vec<QirShotId> = QirShotId::first(40).into_iter().rev().collect();
    let per_shot = expect_ready(QirSimulatorResumer::new(full_state).run(&machine, &shots))
        .expect("the shots do not trap");
    let coins = |coin| {
        per_shot
            .iter()
            .any(|shot| shot[0] == OutputRecord::Result(coin))
    };
    assert!(
        coins(One) && coins(Zero),
        "precondition: the coin reads 1 on some shots and 0 on others"
    );
    let grouped = expect_ready(QirGroupingResumer::new(full_state).run(&machine, &shots))
        .expect("the shots do not trap");
    assert_eq!(grouped, per_shot);
}

/// 100 shots of [`coin_then_branch`], of three batches each, on full-state
/// simulators seeded by their ids: one dispatch per group and round, four in
/// all (the second round has two groups), which run its 300 batches.
#[test]
fn a_run_makes_one_dispatch_per_group_and_round() {
    let program = coin_then_branch();
    let mut resumer = QirGroupingResumer::new(full_state);
    let records = expect_ready(resumer.run(&QirMachine::new(&program), &QirShotId::first(100)))
        .expect("the shots do not trap");
    let coins = |coin| {
        records
            .iter()
            .any(|shot| shot[0] == OutputRecord::Result(coin))
    };
    assert!(
        coins(One) && coins(Zero),
        "precondition: the coin reads 1 on some shots and 0 on others"
    );
    let batches: usize = resumer.dispatches().iter().map(Vec::len).sum();
    assert_eq!((resumer.dispatches().len(), batches), (4, 300));
}
