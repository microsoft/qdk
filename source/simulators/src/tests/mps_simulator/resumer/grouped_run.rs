// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The grouped run's rules: shots that draw the same values share one
//! state, a split forks it once and keeps few states at once, and a run
//! names its first failing shot in the order of its shots, whichever part
//! of a split goes on.

use crate::bytecode::Instruction;
use crate::mps_simulator::{MpsResumer, MpsResumerError};
use crate::qir::{QirProgram, QirResumeError, QirShotId, QirTrap};
use tensornet::MpsEngine;
use tensornet::reference::ExactReference;

use super::super::programs::{
    branch, coin_copied_by_a_branch, coins, gate, gate1, jump, mz, program, program_with,
    read_result, ret, set,
};
use super::super::reference_spy::{ReferenceSpy, ReferenceSpyError};
use super::{RunResult, copies, results, run};
use crate::bytecode::encoding::{OPID_H, OPID_RY, OPID_X};

/// Shots that draw the same values share one state: a program of no
/// randomness prepares one state for all its shots and forks none.
#[test]
fn shots_with_the_same_draws_share_one_state() {
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
    run(&mut resumer, &copies(), &QirShotId::first(10)).expect("the program runs");
    assert_eq!(
        (resumer.engine().prepared().len(), resumer.engine().forks()),
        (1, 0)
    );
}

/// Shots whose draws differ fork the state where they differ: one coin
/// splits the shots once, and the copy's branch runs on the fork.
#[test]
fn shots_whose_draws_differ_fork_the_state_once_where_they_differ() {
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
    let records = run(
        &mut resumer,
        &coin_copied_by_a_branch(),
        &QirShotId::first(16),
    )
    .expect("the program runs");
    assert!(
        records.contains(&results(&[0, 0])) && records.contains(&results(&[1, 1])),
        "both values occur among 16 shots"
    );
    assert_eq!(
        (resumer.engine().prepared().len(), resumer.engine().forks()),
        (1, 1)
    );
}

/// A run of N shots holds at most ⌊log₂ N⌋ + 1 states at once, however its
/// draws split them: here twelve biased coins (P(One) 0.15) on 16 shots,
/// each coin splitting off a few shots, which a run that went on with the
/// larger part would hold all at once.
#[test]
fn a_run_holds_at_most_log2_of_its_shots_plus_one_states() {
    let angle = 2.0 * 0.15_f64.sqrt().asin();
    let mut block: Vec<_> = (0..12)
        .flat_map(|qubit| [gate(OPID_RY, qubit, 0, angle), mz(qubit)])
        .collect();
    block.push(ret());
    let program = program(12, &[&block]);
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
    run(&mut resumer, &program, &QirShotId::first(16)).expect("the program runs");
    assert!(
        resumer.engine().forks() > 5,
        "the coins split the shots often"
    );
    assert!(
        resumer.engine().most_live() <= 5,
        "{}",
        resumer.engine().most_live()
    );
}

/// Coins on qubits `0..coins`, read into their results; then a trap (a
/// branch on a register holding 2) unless every coin reads `pass`.
fn coins_then_trap_unless_all(coins: u64, pass: u64) -> QirProgram {
    let (trap, end, first_check) = (1, 2, 3);
    let mut entry: Vec<_> = (0..coins)
        .flat_map(|qubit| [gate1(OPID_H, qubit), mz(qubit)])
        .collect();
    entry.push(jump(first_check));
    // Register `coins` is the trap's.
    let mut blocks = vec![
        entry,
        vec![set(coins, 2), branch(coins, end, end)],
        vec![ret()],
    ];
    // One check per coin, in a chain that reaches `end` only when every
    // coin reads `pass`.
    for qubit in 0..coins {
        let next = if qubit + 1 == coins {
            end
        } else {
            first_check + qubit + 1
        };
        let (on_one, on_zero) = if pass == 1 {
            (next, trap)
        } else {
            (trap, next)
        };
        blocks.push(vec![
            read_result(qubit, qubit),
            branch(qubit, on_one, on_zero),
        ]);
    }
    let blocks: Vec<&[Instruction<u64>]> = blocks.iter().map(Vec::as_slice).collect();
    let qubits = u32::try_from(coins).expect("a few coins");
    program_with(qubits, qubits + 1, &[], &blocks)
}

/// The shots of `shots` that fail when each runs alone, in their order.
fn failing_alone<E: MpsEngine>(
    make: impl Fn() -> MpsResumer<E>,
    program: &QirProgram,
    shots: &[QirShotId],
) -> Vec<QirShotId> {
    shots
        .iter()
        .copied()
        .filter(|&shot| run(&mut make(), program, &[shot]).is_err())
        .collect()
}

/// The shot a run's error names, for a run that traps.
fn trapped_shot<E: std::fmt::Debug>(run: RunResult<E>) -> QirShotId {
    match run.expect_err("some shots trap") {
        QirResumeError::Trapped {
            shot,
            trap: QirTrap::NotABit { value: 2, .. },
        } => shot,
        other => panic!("the shots trap on the branch: {other:?}"),
    }
}

/// A run names the first failing shot in the order of its shots, whichever
/// part of a split goes on: on a coin whose shots that read 1 trap, and one
/// whose shots that read 0 trap, for 16 seeds, the shots forward and
/// reversed.
#[test]
fn a_run_names_the_first_failing_shot_in_the_order_of_its_shots() {
    let shots = QirShotId::first(8);
    let reversed: Vec<_> = shots.iter().rev().copied().collect();
    for pass in [0, 1] {
        let program = coins_then_trap_unless_all(1, pass);
        for seed in 0..16 {
            let make = || MpsResumer::new(ExactReference::new(), seed);
            for order in [&shots, &reversed] {
                let failing = failing_alone(make, &program, order);
                if failing.is_empty() {
                    continue;
                }
                let named = trapped_shot(run(&mut make(), &program, order));
                assert_eq!(named, failing[0], "pass {pass}, seed {seed}");
            }
        }
    }
}

/// Among several failing groups (two coins, every history but 0, 0
/// trapping), the run names the first failing shot, forward and reversed.
#[test]
fn a_run_names_the_first_failing_shot_among_several_failing_groups() {
    let program = coins_then_trap_unless_all(2, 0);
    let shots = QirShotId::first(16);
    let reversed: Vec<_> = shots.iter().rev().copied().collect();
    let make = || MpsResumer::new(ExactReference::new(), 7);
    for order in [&shots, &reversed] {
        let failing = failing_alone(make, &program, order);
        assert!(failing.len() > 2, "most shots trap");
        assert_eq!(trapped_shot(run(&mut make(), &program, order)), failing[0]);
    }
}

/// The groups whose shots all come after the first failing shot are dropped
/// unrun: the shot that reads 1 first in the order traps, and the gates of
/// the shots that read 0, after it, are not applied.
#[test]
fn groups_after_the_first_failing_shot_are_dropped_unrun() {
    let (trap, rest) = (1, 2);
    let program = program(
        2,
        &[
            &[
                gate1(OPID_H, 0),
                mz(0),
                read_result(0, 0),
                branch(0, trap, rest),
            ],
            &[set(0, 2), branch(0, rest, rest)],
            &[gate1(OPID_X, 1), gate1(OPID_X, 1), gate1(OPID_X, 1), ret()],
        ],
    );
    let make = || MpsResumer::new(ReferenceSpy::new(), 7);
    let all = QirShotId::first(16);
    let failing = failing_alone(make, &program, &all);
    let passing: Vec<_> = all.iter().filter(|shot| !failing.contains(shot)).collect();
    assert!(
        !failing.is_empty() && passing.len() >= 3,
        "some of 16 coins read 1, and three read 0"
    );
    // One shot that reads 1, first, then three that read 0: the one goes on
    // and traps, and the three wait.
    let shots = [failing[0], *passing[0], *passing[1], *passing[2]];
    let mut resumer = make();
    assert_eq!(
        trapped_shot(run(&mut resumer, &program, &shots)),
        failing[0]
    );
    assert_eq!(
        resumer.engine().applies(),
        1,
        "only the coin's h is applied"
    );
}

/// An engine's failure fails the shots whose group met it, and the run
/// names the first of them: here projections on One fail.
#[test]
fn an_engine_failure_names_the_first_shot_whose_group_met_it() {
    let program = coins(1);
    let shots = QirShotId::first(16);
    let reversed: Vec<_> = shots.iter().rev().copied().collect();
    let make = || MpsResumer::new(ReferenceSpy::failing_projections_on_one(), 5);
    for order in [&shots, &reversed] {
        let failing = failing_alone(make, &program, order);
        // Fewer shots read 1 than 0, so the shots that read 1 go on apart
        // and meet the failure after the split.
        assert!(
            !failing.is_empty() && 2 * failing.len() < order.len(),
            "some of 16 coins read 1, fewer than read 0"
        );
        let error = run(&mut make(), &program, order).expect_err("a projection on One fails");
        assert!(
            matches!(
                error,
                QirResumeError::Resumer {
                    shot,
                    error: MpsResumerError::Engine(ReferenceSpyError::Device),
                } if shot == failing[0]
            ),
            "{error:?}"
        );
    }
}

/// One shot that reads 0 on a coin with seed 5, then two that read 1, as
/// each reads alone: on the split, the shot that reads 0 goes on, the
/// fewer, and the two wait.
fn a_zero_then_two_ones() -> [QirShotId; 3] {
    let coin = coins(1);
    let reading = |value: u8| {
        let coin = &coin;
        QirShotId::first(16).into_iter().filter(move |&shot| {
            run(&mut MpsResumer::new(ReferenceSpy::new(), 5), coin, &[shot]).expect("a coin runs")
                == [results(&[value])]
        })
    };
    let zero = reading(0).next().expect("some of 16 coins read 0");
    let mut ones = reading(1);
    let one = ones.next().expect("some of 16 coins read 1");
    let other_one = ones.next().expect("two of 16 coins read 1");
    [zero, one, other_one]
}

/// The engines whose failure meets only the waiting part of a split: one
/// fails the fork, the other the fork's projection on One, the waiting
/// part's value here.
const FAILING_SPLITS: [fn() -> ReferenceSpy; 2] = [
    ReferenceSpy::failing_forks,
    ReferenceSpy::failing_projections_on_one,
];

/// When the copy for the part of a split that waits fails, forked or
/// projected on its value, only the shots that wait fail: the run names
/// the first of them, not the shot before it that went on and ends.
#[test]
fn a_failed_split_fails_only_the_shots_that_wait() {
    let shots = a_zero_then_two_ones();
    let program = coins(1);
    for engine in FAILING_SPLITS {
        run(&mut MpsResumer::new(engine(), 5), &program, &shots[..1])
            .expect("the shot that reads 0 ends alone");
        let error = run(&mut MpsResumer::new(engine(), 5), &program, &shots)
            .expect_err("the waiting part's copy fails");
        assert!(
            matches!(
                error,
                QirResumeError::Resumer {
                    shot,
                    error: MpsResumerError::Engine(ReferenceSpyError::Device),
                } if shot == shots[1]
            ),
            "{error:?}"
        );
    }
}

/// Between a failed split and a shot that goes on from it and fails later
/// on its own, the run names whichever shot comes first in the order of
/// the shots, not the failure met last: one coin, whose shots that read 0
/// trap; the shot that reads 0 first, then last.
#[test]
fn a_failed_split_and_a_later_failure_name_the_first_shot_in_order() {
    let [zero, one, other_one] = a_zero_then_two_ones();
    let program = coins_then_trap_unless_all(1, 1);
    for engine in FAILING_SPLITS {
        let zero_first = run(
            &mut MpsResumer::new(engine(), 5),
            &program,
            &[zero, one, other_one],
        );
        assert_eq!(trapped_shot(zero_first), zero);
        let zero_last = run(
            &mut MpsResumer::new(engine(), 5),
            &program,
            &[one, other_one, zero],
        )
        .expect_err("the waiting part's copy fails");
        assert!(
            matches!(
                zero_last,
                QirResumeError::Resumer {
                    shot,
                    error: MpsResumerError::Engine(ReferenceSpyError::Device),
                } if shot == one
            ),
            "{zero_last:?}"
        );
    }
}

/// On a split whose parts are as large, the shots that read 0 go on and
/// those that read 1 wait: one shot of each on a coin, with an engine whose
/// forks fail, fails the shot that reads 1, first in the order or not.
#[test]
fn on_a_tie_the_shots_that_read_zero_go_on() {
    let [zero, one, _] = a_zero_then_two_ones();
    for shots in [[one, zero], [zero, one]] {
        let error = run(
            &mut MpsResumer::new(ReferenceSpy::failing_forks(), 5),
            &coins(1),
            &shots,
        )
        .expect_err("the fork for the part that waits fails");
        assert!(
            matches!(
                error,
                QirResumeError::Resumer {
                    shot,
                    error: MpsResumerError::Engine(ReferenceSpyError::Device),
                } if shot == one
            ),
            "{error:?}"
        );
    }
}
