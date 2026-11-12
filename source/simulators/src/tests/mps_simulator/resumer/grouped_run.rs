// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The grouped run's rules: a shot reads One only where One has a
//! probability, and draws a uniform even where the outcome is certain;
//! shots that draw the same values share one state, prepared only when a
//! shot first needs it; a split forks it once and keeps few states at
//! once; an engine failure fails the shot with the engine's error, and the
//! state it failed on is not used again (a failed fork fails the whole
//! group, a failed settle of the fork only the shots that wait); a run
//! names its first failing shot in the order of its shots, whichever part
//! of a split goes on, and drops the groups whose shots all come after it;
//! and a run after a failed one is independent of it.

use crate::bytecode::Instruction;
use crate::mps_simulator::{GateFusion, MpsResumer, MpsResumerError};
use crate::qir::{QirProgram, QirResumeError, QirShotId, QirTrap};
use tensornet::MpsEngine;
use tensornet::reference::ExactReference;

use super::super::programs::{
    branch, coin_copied_by_a_branch, coins, gate, gate1, gate2, jump, measure, mz, program,
    program_with, read_result, reset, ret, set,
};
use super::super::reference_spy::{Call, ReferenceSpy, ReferenceSpyError};
use super::shot_stream::shot_stream;
use super::{RunResult, copies, ids, results, run};
use crate::bytecode::encoding::{OPID_CX, OPID_H, OPID_MRESETZ, OPID_MZ, OPID_RY, OPID_X};

/// A shot reads One only where One has a probability: a qubit at |0⟩ reads
/// 0 even when the shot's uniform is 0, the smallest it draws. The run's
/// seed is chosen so that shot 0's first uniform is 0.
#[test]
fn a_qubit_at_zero_reads_zero_even_on_a_uniform_of_zero() {
    let seed = 0xe85b_9f31_99d1_1237;
    assert_eq!(
        shot_stream(seed, 0).next_uniform().to_bits(),
        0.0f64.to_bits(),
        "the seed gives shot 0 a first uniform of 0"
    );
    let program = program(1, &[&[mz(0), ret()]]);
    let records = run(
        &mut MpsResumer::new(ExactReference::new(), seed),
        &program,
        &ids(&[0]),
    );
    assert_eq!(records.expect("the program runs"), vec![results(&[0])]);
}

/// A shot draws a uniform at a measurement whose value is certain, as at
/// any other, so its later draws are those it would make were the value
/// uncertain: a qubit at |0⟩, or flipped to |1⟩, measured, then turned to
/// a fair coin and measured again, which reads the second uniform of the
/// shot's stream against ½.
#[test]
fn a_certain_measurement_draws_a_uniform() {
    let shots: Vec<u32> = (0..16).collect();
    for certain_one in [false, true] {
        let mut block = Vec::new();
        if certain_one {
            block.push(gate1(OPID_X, 0));
        }
        block.extend([
            measure(OPID_MZ, 0, 0),
            gate1(OPID_H, 0),
            measure(OPID_MZ, 0, 1),
            ret(),
        ]);
        let program = program_with(1, 2, &[], &[&block]);
        let records = run(
            &mut MpsResumer::new(ExactReference::new(), 5),
            &program,
            &ids(&shots),
        )
        .expect("the program runs");
        for (&shot, records) in shots.iter().zip(records) {
            let mut stream = shot_stream(5, shot);
            let _certain = stream.next_uniform();
            let coin = u8::from(stream.next_uniform() < 0.5);
            assert_eq!(
                records,
                results(&[u8::from(certain_one), coin]),
                "{certain_one}, shot {shot}"
            );
        }
    }
}

/// A shot draws a uniform at a reset whose value is certain, as at a
/// measurement: a qubit at |0⟩, or flipped to |1⟩, reset, then turned to a
/// fair coin and measured, which reads the second uniform of the shot's
/// stream against ½.
#[test]
fn a_certain_reset_draws_a_uniform() {
    let shots: Vec<u32> = (0..16).collect();
    for certain_one in [false, true] {
        let mut block = Vec::new();
        if certain_one {
            block.push(gate1(OPID_X, 0));
        }
        block.extend([reset(0), gate1(OPID_H, 0), mz(0), ret()]);
        let program = program(1, &[&block]);
        let records = run(
            &mut MpsResumer::new(ExactReference::new(), 5),
            &program,
            &ids(&shots),
        )
        .expect("the program runs");
        for (&shot, records) in shots.iter().zip(records) {
            let mut stream = shot_stream(5, shot);
            let _certain = stream.next_uniform();
            let coin = u8::from(stream.next_uniform() < 0.5);
            assert_eq!(records, results(&[coin]), "{certain_one}, shot {shot}");
        }
    }
}

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

/// A state is prepared only when a shot first needs it: a program of two
/// qubits whose quantum part runs only when a classical flag is set, here
/// clear, so its shots end with no quantum call, prepares no state.
#[test]
fn a_shot_with_no_quantum_call_prepares_no_state() {
    let (quantum, done) = (1, 2);
    let program = program(
        2,
        &[
            &[set(0, 0), branch(0, quantum, done)],
            &[gate1(OPID_H, 0), mz(0), jump(done)],
            &[ret()],
        ],
    );
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
    let records = run(&mut resumer, &program, &ids(&[0, 1])).expect("the program runs");
    assert_eq!(
        records,
        vec![results(&[0, 0]); 2],
        "no shot took the quantum part"
    );
    assert!(resumer.engine().prepared().is_empty());
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

/// A Bell pair read into two results: a run of one shot makes each call
/// of [`LONE_SHOT_CALLS`].
fn bell() -> QirProgram {
    program(
        2,
        &[&[gate1(OPID_H, 0), gate2(OPID_CX, 0, 1), mz(0), mz(1), ret()]],
    )
}

/// The engine's calls a run of one shot of [`bell`] makes: it prepares a
/// state, applies the pair's gates, reads the largest bond, reads P(One)
/// and projects at each measurement, and reads the retained weight at the
/// end. Only a split forks, which a lone shot never makes: a failed fork
/// has its own tests below.
const LONE_SHOT_CALLS: [Call; 6] = [
    Call::Prepare,
    Call::Apply,
    Call::Shape,
    Call::ProbabilityOfOne,
    Call::Project,
    Call::RetainedWeight,
];

/// An engine's failure in any call a shot makes alone fails the shot with
/// that failure: a Bell pair's shot, on an engine that fails the first of
/// one of those calls.
#[test]
fn a_failure_of_any_call_a_lone_shot_makes_fails_it_with_the_engines_error() {
    for call in LONE_SHOT_CALLS {
        let error = run(
            &mut MpsResumer::new(ReferenceSpy::failing_first(call), 5),
            &bell(),
            &ids(&[9]),
        )
        .expect_err("the call fails");
        assert!(
            matches!(
                error,
                QirResumeError::Resumer {
                    shot: QirShotId(9),
                    error: MpsResumerError::Engine(ReferenceSpyError::Device),
                }
            ),
            "{call:?}: {error:?}"
        );
    }
}

/// A state a call failed on is not used again, as the engine leaves it
/// unspecified: a Bell pair's shot, on an engine that fails the first of
/// one of the calls given a state.
#[test]
fn a_state_a_call_failed_on_is_not_used_again() {
    for call in LONE_SHOT_CALLS {
        if call == Call::Prepare {
            continue; // A failed preparation gives no state.
        }
        let mut resumer = MpsResumer::new(ReferenceSpy::failing_first(call), 5);
        run(&mut resumer, &bell(), &ids(&[9])).expect_err("the call fails");
        assert_eq!(resumer.engine().uses_after_failure(), 0, "{call:?}");
    }
}

/// A run after a run an engine's failure ended gives the records a new
/// resumer gives: eight shots of a Bell pair, twice, on an engine that
/// fails only the first of one of the calls the run makes.
#[test]
fn a_run_after_a_failed_run_gives_the_records_of_a_new_resumer() {
    let shots = QirShotId::first(8);
    let new = run(
        &mut MpsResumer::new(ReferenceSpy::new(), 5),
        &bell(),
        &shots,
    )
    .expect("the program runs");
    for call in LONE_SHOT_CALLS {
        let mut resumer = MpsResumer::new(ReferenceSpy::failing_first(call), 5);
        run(&mut resumer, &bell(), &shots).expect_err("the call fails");
        let next = run(&mut resumer, &bell(), &shots).expect("the engine fails only once");
        assert_eq!(next, new, "{call:?}");
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

/// The shots among the first 64 that read `first` on the first of two coins
/// with seed 5, and `second`, when given, on the second, as each reads
/// alone, in order.
fn reading_on_two_coins(first: u8, second: Option<u8>) -> Vec<QirShotId> {
    let program = coins(2);
    QirShotId::first(64)
        .into_iter()
        .filter(|&shot| {
            let read = run(
                &mut MpsResumer::new(ReferenceSpy::new(), 5),
                &program,
                &[shot],
            )
            .expect("two coins run")
            .remove(0);
            read[0] == results(&[first])[0]
                && second.is_none_or(|second| read[1] == results(&[second])[0])
        })
        .collect()
}

/// When the fork of a group's state fails, the whole group fails, before
/// its shots part: the engine leaves the state it failed on unspecified, so
/// no part may go on with it. On a coin, the shot that reads 0 would go on,
/// the fewer, and the two that read 1 would wait; the run names the first
/// shot, which ends alone, whichever part it would be in: the shot that
/// reads 0 first, then a shot that reads 1 first.
#[test]
fn a_failed_fork_fails_the_shots_going_on_too() {
    let [zero, one, other_one] = a_zero_then_two_ones();
    let make = || MpsResumer::new(ReferenceSpy::failing_forks(), 5);
    for shots in [[zero, one, other_one], [one, zero, other_one]] {
        run(&mut make(), &coins(1), &shots[..1]).expect("the first shot ends alone");
        let error = run(&mut make(), &coins(1), &shots).expect_err("the fork fails");
        assert!(
            matches!(
                error,
                QirResumeError::Resumer {
                    shot,
                    error: MpsResumerError::Engine(ReferenceSpyError::Device),
                } if shot == shots[0]
            ),
            "{shots:?}: {error:?}"
        );
    }
}

/// The state whose fork failed is not used again, as the engine leaves it
/// unspecified: on a coin, the part that would go on does not settle it.
#[test]
fn a_state_whose_fork_failed_is_not_used_again() {
    let mut resumer = MpsResumer::new(ReferenceSpy::failing_forks(), 5);
    run(&mut resumer, &coins(1), &a_zero_then_two_ones()).expect_err("the fork fails");
    assert_eq!(resumer.engine().uses_after_failure(), 0);
}

/// When the copy for the part of a split that waits fails as it is
/// projected on that part's value, only the shots that wait fail, as the
/// group's own state was not touched: the shot that reads 0, the fewer,
/// goes on and applies the X after the coin, and the run names the first
/// shot that waits, not the shot before it.
#[test]
fn a_failed_projection_of_the_waiting_copy_fails_only_the_shots_that_wait() {
    let shots = a_zero_then_two_ones();
    let program = program(2, &[&[gate1(OPID_H, 0), mz(0), gate1(OPID_X, 1), ret()]]);
    let make = || MpsResumer::new(ReferenceSpy::failing_projections_on_one(), 5);
    run(&mut make(), &program, &shots[..1]).expect("the shot that reads 0 ends alone");
    let mut resumer = make();
    let error = run(&mut resumer, &program, &shots).expect_err("the waiting part's copy fails");
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
    assert_eq!(
        resumer.engine().applies(),
        2,
        "the coin's h, then the X of the part going on"
    );
}

/// When the copy for the part of a split that waits fails as a reset
/// returns it to |0⟩, only the shots that wait fail, as when its projection
/// fails: a coin read and reset, on an engine whose X gates
/// fail. The shot that reads 0 goes on, the fewer, and needs no X; the two
/// that read 1 wait on a copy that keeps One, whose X fails, and the run
/// names the first of them.
#[test]
fn a_failed_reset_of_the_waiting_copy_fails_only_the_shots_that_wait() {
    let shots = a_zero_then_two_ones();
    let program = program(
        1,
        &[&[gate1(OPID_H, 0), measure(OPID_MRESETZ, 0, 0), ret()]],
    );
    let make = || MpsResumer::new(ReferenceSpy::failing_x_gates(), 5);
    run(&mut make(), &program, &shots[..1]).expect("the shot that reads 0 ends alone");
    let error = run(&mut make(), &program, &shots).expect_err("the waiting copy's X fails");
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

/// A failure met later, for shots that come after the first failing shot,
/// does not replace it: four shots A, B, C, D on two coins, with an engine
/// whose projections on One fail. On the first coin, A and C read 0 and B
/// and D read 1: on the tie, A and C go on, and the copy for B and D fails,
/// failing B. On the second coin, A reads 0 and C reads 1: on the tie, A
/// goes on, and the copy for C fails too. The run names B.
#[test]
fn a_failure_met_later_for_later_shots_keeps_the_first_failing_shot() {
    let a = reading_on_two_coins(0, Some(0))[0];
    let c = reading_on_two_coins(0, Some(1))[0];
    let ones = reading_on_two_coins(1, None);
    let (b, d) = (ones[0], ones[1]);
    let error = run(
        &mut MpsResumer::new(ReferenceSpy::failing_projections_on_one(), 5),
        &coins(2),
        &[a, b, c, d],
    )
    .expect_err("the copies that keep One fail");
    assert!(
        matches!(
            error,
            QirResumeError::Resumer {
                shot,
                error: MpsResumerError::Engine(ReferenceSpyError::Device),
            } if shot == b
        ),
        "{error:?}"
    );
}

/// Between a failed waiting copy and a shot that goes on from its split
/// and fails later on its own, the run names whichever shot comes first in
/// the order of the shots, not the failure met last: one coin, whose shots
/// that read 0 trap, on an engine whose projections on One fail, so the
/// copy for the shots that read 1 fails; the shot that reads 0 first, then
/// last.
#[test]
fn a_failed_waiting_copy_and_a_later_failure_name_the_first_shot_in_order() {
    let [zero, one, other_one] = a_zero_then_two_ones();
    let program = coins_then_trap_unless_all(1, 1);
    let make = || MpsResumer::new(ReferenceSpy::failing_projections_on_one(), 5);
    let zero_first = run(&mut make(), &program, &[zero, one, other_one]);
    assert_eq!(trapped_shot(zero_first), zero);
    let zero_last = run(&mut make(), &program, &[one, other_one, zero])
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

/// The part that goes on from a split whose waiting copy failed runs no
/// later step when its shots all come after the shots that failed, since it
/// could not change the shot the run names: a coin, then three X gates on
/// a second qubit, on an engine whose projections on One fail; the two
/// shots that read 1 first, which wait and whose copy fails, then the shot
/// that reads 0, which goes on and applies none of the gates.
#[test]
fn the_part_going_on_from_a_failed_waiting_copy_stops_when_its_shots_come_after() {
    let [zero, one, other_one] = a_zero_then_two_ones();
    let program = program(
        2,
        &[&[
            gate1(OPID_H, 0),
            mz(0),
            gate1(OPID_X, 1),
            gate1(OPID_X, 1),
            gate1(OPID_X, 1),
            ret(),
        ]],
    );
    let mut resumer = MpsResumer::new(ReferenceSpy::failing_projections_on_one(), 5);
    let error = run(&mut resumer, &program, &[one, other_one, zero])
        .expect_err("the waiting part's copy fails");
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
    assert_eq!(
        resumer.engine().applies(),
        1,
        "only the coin's h is applied"
    );
}

/// The part going on from any split runs no later step when its shots all
/// come after the run's failure, not only from a split whose waiting copy
/// failed: a first coin, whose shots that read 1 trap, then for the others
/// a second coin and three X gates on a third qubit, unfused, so that each
/// gate is one apply. The shots: A and C read 0 then 0, B reads 1, D reads
/// 0 then 1. On the first coin B goes on, the fewer, and traps; A, C and D
/// run after it, as A comes before it. On the second coin D goes on alone,
/// after B, and stops: only A and C apply the three X.
#[test]
fn the_part_going_on_from_any_split_stops_when_its_shots_come_after_the_failure() {
    let (trap, rest) = (1, 2);
    let program = program(
        3,
        &[
            &[
                gate1(OPID_H, 0),
                mz(0),
                read_result(0, 0),
                branch(0, trap, rest),
            ],
            &[set(0, 2), branch(0, rest, rest)],
            &[
                gate1(OPID_H, 1),
                mz(1),
                gate1(OPID_X, 2),
                gate1(OPID_X, 2),
                gate1(OPID_X, 2),
                ret(),
            ],
        ],
    );
    let zeros = reading_on_two_coins(0, Some(0));
    let (a, c) = (zeros[0], zeros[1]);
    let b = reading_on_two_coins(1, None)[0];
    let d = reading_on_two_coins(0, Some(1))[0];
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 5).with_gate_fusion(GateFusion::Unfused);
    assert_eq!(trapped_shot(run(&mut resumer, &program, &[a, b, c, d])), b);
    assert_eq!(
        resumer.engine().applies(),
        5,
        "the two coins' h, then the three X once, for A and C"
    );
}

/// On a split whose parts are as large, the shots that read 0 go on and
/// those that read 1 wait: one shot of each on a coin, the one that reads 1
/// first, then for the shots that read 1 a trap and for the others an X on
/// a second qubit. The shot that reads 0 goes on and applies the X before
/// the other traps; had it waited, it would come after the trap and be
/// dropped unrun.
#[test]
fn on_a_tie_the_shots_that_read_zero_go_on() {
    let [zero, one, _] = a_zero_then_two_ones();
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
            &[gate1(OPID_X, 1), ret()],
        ],
    );
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 5);
    assert_eq!(trapped_shot(run(&mut resumer, &program, &[one, zero])), one);
    assert_eq!(
        resumer.engine().applies(),
        2,
        "the coin's h, then the X of the shot that reads 0"
    );
}
