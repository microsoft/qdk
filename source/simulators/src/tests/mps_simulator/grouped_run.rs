// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The grouped run's rules: a shot reads One only where One has a
//! probability, and draws a uniform even where the outcome is certain;
//! shots that draw the same values share one state, prepared only when a
//! shot first needs it; a split forks it once and keeps few states at
//! once; an engine failure fails the shot with the engine's error, and the
//! state it failed on is not used again (a failed fork fails the whole
//! group, a failed settle of the fork only the shots that wait); a run
//! names its first failing shot in the order of its shots, whichever group
//! of a split goes on, and drops the groups whose shots all come after it;
//! a run after a failed one is independent of it; and a stop flag ends a
//! run before the next batch.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::bytecode::Instruction;
use crate::mps_simulator::shot_stream::shot_stream;
use crate::mps_simulator::{MpsResumer, MpsResumerError};
use crate::qir::{QirProgram, QirResumeError, QirShotId, QirTrap};
use crate::{MeasurementResult, OutputRecord};
use tensornet::reference::ExactReference;
use tensornet::{MpsEngine, MpsShape};

use super::programs::{
    branch, coin_copied_by_a_branch, coins, gate, gate1, gate2, jump, measure, mz, program,
    program_with, read_result, readout_noise, reset, ret, set,
};
use super::reference_spy::{Call, ReferenceSpy, ReferenceSpyError};
use super::{LONE_SHOT_CALLS, bell, copies, ids, results, run};
use crate::bytecode::encoding::{OPID_CX, OPID_H, OPID_MRESETZ, OPID_MZ, OPID_RY, OPID_X};

/// A shot reads One only where One has a probability: a qubit at |0⟩ reads
/// 0 even when the shot's uniform is 0, the smallest it draws. The run's
/// seed is chosen so that shot 0's first uniform is 0.
#[test]
fn a_qubit_at_zero_reads_zero_even_on_a_uniform_of_zero() {
    let seed = 0x7951_b2dd_10d3_4ef6;
    assert_eq!(
        shot_stream(seed, QirShotId(0)).next_uniform().to_bits(),
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

/// A shot draws a uniform at a measurement or a reset whose value is
/// certain, as at any other, so its later draws are those it would make
/// were the value uncertain: a qubit at |0⟩, or flipped to |1⟩, measured
/// (or reset), then turned to a fair coin and measured into a second
/// result, which reads the second uniform of the shot's stream against ½.
#[test]
fn a_certain_measurement_or_reset_still_draws_a_uniform() {
    // Each row: the scene, the certain call, and whether it records the
    // value into the first result (a reset leaves it at 0).
    let rows = [
        ("a certain measurement", measure(OPID_MZ, 0, 0), true),
        ("a certain reset", reset(0), false),
    ];
    let shots: Vec<u32> = (0..16).collect();
    for (scene, certain_call, records_it) in rows {
        for certain_one in [false, true] {
            let mut block = Vec::new();
            if certain_one {
                block.push(gate1(OPID_X, 0));
            }
            block.extend([
                certain_call,
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
            let first = u8::from(records_it && certain_one);
            for (&shot, records) in shots.iter().zip(records) {
                let mut stream = shot_stream(5, QirShotId(shot));
                let _certain = stream.next_uniform();
                let coin = u8::from(stream.next_uniform() < 0.5);
                assert_eq!(
                    records,
                    results(&[first, coin]),
                    "{scene} of {certain_one}, shot {shot}"
                );
            }
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
    assert_eq!(resumer.engine().prepared(), Vec::<MpsShape>::new());
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
/// larger group would hold all at once.
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

/// How a run failed.
#[derive(Debug, PartialEq)]
enum Failure {
    /// A shot trapped, on a branch on a register holding 2.
    Trap,
    /// The engine failed a call, as the spy's failing engines do.
    Engine,
}

/// A run that fails: its scene, for the failure message, and its engine,
/// seed, program and shots.
struct FailingRun {
    scene: String,
    engine: fn() -> ReferenceSpy,
    seed: u64,
    program: QirProgram,
    shots: Vec<QirShotId>,
}

impl FailingRun {
    /// Runs the scene on a new resumer; returns the resumer, the shot the
    /// run names, and how it failed.
    fn run(&self) -> (MpsResumer<ReferenceSpy>, QirShotId, Failure) {
        let mut resumer = MpsResumer::new((self.engine)(), self.seed);
        let error = run(&mut resumer, &self.program, &self.shots).expect_err(&self.scene);
        let (shot, failure) = match error {
            QirResumeError::Trapped {
                shot,
                trap: QirTrap::NotABit { value: 2, .. },
            } => (shot, Failure::Trap),
            QirResumeError::Resumer {
                shot,
                error: MpsResumerError::Engine(ReferenceSpyError::Device),
            } => (shot, Failure::Engine),
            other => panic!("{}: a trap or the engine fails it: {other:?}", self.scene),
        };
        (resumer, shot, failure)
    }
}

/// A failed run names its first failing shot in the order of its shots,
/// whichever group of a split goes on and whichever failure the run meets
/// first. Each row is a scene, the shot it names, and how that shot fails.
#[test]
fn a_failed_run_names_its_first_failing_shot_in_shot_order() {
    let mut rows: Vec<(FailingRun, QirShotId, Failure)> = Vec::new();

    // On a coin whose shots that read 1 trap, and on one whose shots that
    // read 0 trap, for 16 seeds, 8 shots forward and reversed: the first of
    // the shots that fail alone.
    let eight = QirShotId::first(8);
    let eight_reversed: Vec<_> = eight.iter().rev().copied().collect();
    for pass in [0, 1] {
        for seed in 0..16 {
            for (order, shots) in [("forward", &eight), ("reversed", &eight_reversed)] {
                let program = coins_then_trap_unless_all(1, pass);
                let make = || MpsResumer::new(ReferenceSpy::new(), seed);
                if let Some(&first) = failing_alone(make, &program, shots).first() {
                    let scene = format!("a coin that traps unless {pass}, seed {seed}, {order}");
                    let shots = shots.clone();
                    let engine = ReferenceSpy::new;
                    let scene_run = FailingRun {
                        scene,
                        engine,
                        seed,
                        program,
                        shots,
                    };
                    rows.push((scene_run, first, Failure::Trap));
                }
            }
        }
    }

    // Among several failing groups, on two coins whose every history but
    // 0, 0 traps, 16 shots forward and reversed: the first of the shots
    // that fail alone.
    let sixteen = QirShotId::first(16);
    let sixteen_reversed: Vec<_> = sixteen.iter().rev().copied().collect();
    for (order, shots) in [("forward", &sixteen), ("reversed", &sixteen_reversed)] {
        let program = coins_then_trap_unless_all(2, 0);
        let make = || MpsResumer::new(ReferenceSpy::new(), 7);
        let failing = failing_alone(make, &program, shots);
        assert!(failing.len() > 2, "most shots trap");
        let scene = format!("several failing groups, {order}");
        let engine = ReferenceSpy::new;
        let shots = shots.clone();
        let scene_run = FailingRun {
            scene,
            engine,
            seed: 7,
            program,
            shots,
        };
        rows.push((scene_run, failing[0], Failure::Trap));
    }

    // A failure met later, for shots after the first failing shot: four
    // shots A, B, C, D on two coins, on an engine whose projections on One
    // fail. On the first coin, A and C read 0 and B and D read 1, so the
    // state that keeps B and D's 1 fails, failing B; on the second coin, A
    // reads 0 and C reads 1, so C fails too, later. The run names B.
    let a = reading_on_two_coins(0, Some(0))[0];
    let c = reading_on_two_coins(0, Some(1))[0];
    let ones = reading_on_two_coins(1, None);
    let (b, d) = (ones[0], ones[1]);
    let scene_run = FailingRun {
        scene: "a failure met later, for later shots".to_string(),
        engine: ReferenceSpy::failing_projections_on_one,
        seed: 5,
        program: coins(2),
        shots: vec![a, b, c, d],
    };
    rows.push((scene_run, b, Failure::Engine));

    // A failed waiting copy, and a shot that goes on from its split and
    // fails later on its own: a coin whose shots that read 0 trap, on an
    // engine whose projections on One fail, so the copy for the shots that
    // read 1 fails before the shot that reads 0, going on, traps. With the
    // shot that reads 0 first, the run names it; with it last, the first
    // shot that reads 1.
    let [zero, one, other_one] = a_zero_then_two_ones();
    for (scene, shots, named, failure) in [
        (
            "the shot that reads 0 first",
            vec![zero, one, other_one],
            zero,
            Failure::Trap,
        ),
        (
            "the shot that reads 0 last",
            vec![one, other_one, zero],
            one,
            Failure::Engine,
        ),
    ] {
        let scene_run = FailingRun {
            scene: format!("a failed waiting copy, {scene}"),
            engine: ReferenceSpy::failing_projections_on_one,
            seed: 5,
            program: coins_then_trap_unless_all(1, 1),
            shots,
        };
        rows.push((scene_run, named, failure));
    }

    for (scene_run, shot, failure) in rows {
        let (_, named, failed) = scene_run.run();
        assert_eq!((named, failed), (shot, failure), "{}", scene_run.scene);
    }
}

/// No group runs on when its shots all come after the run's first failing
/// shot, since it could not change the shot the run names; so which group
/// of a tie goes on (the one that drew Zero) decides which gates a run
/// applies. Each row is a scene, the shot the run names, how it fails, and
/// the gates the run applies.
#[test]
fn no_group_runs_on_for_shots_after_the_first_failure() {
    let (trap, rest) = (1, 2);
    let mut rows: Vec<(FailingRun, QirShotId, Failure, usize)> = Vec::new();

    // Groups not yet run: a coin whose shots that read 1 trap, then three X
    // gates on a second qubit. One shot that reads 1, first, then three
    // that read 0: the one goes on and traps, and the three, which wait,
    // are dropped: only the coin's h is applied.
    let coin_then_three_x = program(
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
    let failing = failing_alone(make, &coin_then_three_x, &all);
    let passing: Vec<_> = all.iter().filter(|shot| !failing.contains(shot)).collect();
    assert!(
        failing.len() >= 2 && passing.len() >= 3,
        "at least two of 16 coins read 1, and three read 0"
    );
    let scene_run = FailingRun {
        scene: "the groups that wait".to_string(),
        engine: ReferenceSpy::new,
        seed: 7,
        program: coin_then_three_x.clone(),
        shots: vec![failing[0], *passing[0], *passing[1], *passing[2]],
    };
    rows.push((scene_run, failing[0], Failure::Trap, 1));

    // A tie, two shots that read 1 and two that read 0, on the same
    // program: the group that drew Zero goes on, as the docs promise, runs
    // its three X gates and ends, then the group that drew One traps. Had
    // the group that drew One gone on, it would trap first, and the group
    // that drew Zero, all of whose shots come after the first, would be
    // dropped, its X gates never applied.
    let scene_run = FailingRun {
        scene: "a tie".to_string(),
        engine: ReferenceSpy::new,
        seed: 7,
        program: coin_then_three_x,
        shots: vec![failing[0], *passing[0], *passing[1], failing[1]],
    };
    rows.push((scene_run, failing[0], Failure::Trap, 4));

    // The group going on from a split whose waiting copy failed: a coin,
    // then three X gates on a second qubit, on an engine whose projections
    // on One fail. The two shots that read 1 first, which wait and whose
    // copy fails, then the shot that reads 0, which goes on and applies
    // none of the gates.
    let [zero, one, other_one] = a_zero_then_two_ones();
    let scene_run = FailingRun {
        scene: "the group going on from a failed waiting copy".to_string(),
        engine: ReferenceSpy::failing_projections_on_one,
        seed: 5,
        program: program(
            2,
            &[&[
                gate1(OPID_H, 0),
                mz(0),
                gate1(OPID_X, 1),
                gate1(OPID_X, 1),
                gate1(OPID_X, 1),
                ret(),
            ]],
        ),
        shots: vec![one, other_one, zero],
    };
    rows.push((scene_run, one, Failure::Engine, 1));

    // The group going on from any split, not only from one whose waiting
    // copy failed: a first coin, whose shots that read 1 trap, then for the
    // others a second coin and three X gates on a third qubit. The shots:
    // A and C read 0 then 0, B reads 1, D reads 0 then 1. On the first coin
    // B goes on, the fewer, and traps; A, C and D run after it, as A comes
    // before it. On the second coin D goes on alone, after B, and stops:
    // the two coins' h are applied, then the three X once, for A and C.
    let zeros = reading_on_two_coins(0, Some(0));
    let (a, c) = (zeros[0], zeros[1]);
    let b = reading_on_two_coins(1, None)[0];
    let d = reading_on_two_coins(0, Some(1))[0];
    let scene_run = FailingRun {
        scene: "the group going on from any split".to_string(),
        engine: ReferenceSpy::new,
        seed: 5,
        program: program(
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
        ),
        shots: vec![a, b, c, d],
    };
    rows.push((scene_run, b, Failure::Trap, 5));

    for (scene_run, shot, failure, applies) in rows {
        let scene = &scene_run.scene;
        let (resumer, named, failed) = scene_run.run();
        assert_eq!((named, failed), (shot, failure), "{scene}");
        assert_eq!(resumer.engine().applies(), applies, "{scene}");
    }
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

/// A state an engine call failed on is not used again, as the engine
/// leaves it unspecified. Each row is a call that fails: one of the calls
/// given a state that a Bell pair's shot makes alone; or the fork of a
/// coin's state, which the group that would go on does not settle.
#[test]
fn a_state_an_engine_call_failed_on_is_not_used_again() {
    let mut rows: Vec<(String, ReferenceSpy, QirProgram, Vec<QirShotId>)> = Vec::new();
    for call in LONE_SHOT_CALLS {
        if call == Call::Prepare {
            continue; // A failed preparation gives no state.
        }
        let engine = ReferenceSpy::failing_first(call);
        rows.push((format!("a failed {call:?}"), engine, bell(), ids(&[9])));
    }
    let shots = a_zero_then_two_ones().to_vec();
    let engine = ReferenceSpy::failing_forks();
    rows.push(("a failed fork".to_string(), engine, coins(1), shots));
    for (scene, engine, program, shots) in rows {
        let mut resumer = MpsResumer::new(engine, 5);
        run(&mut resumer, &program, &shots).expect_err(&scene);
        assert_eq!(resumer.engine().uses_after_failure(), 0, "{scene}");
    }
}

/// When the fork of a group's state fails, the whole group fails, before
/// its shots split: the engine leaves the state it failed on unspecified, so
/// no group may go on with it. On a coin, the shot that reads 0 would go on,
/// the fewer, and the two that read 1 would wait; the run names the first
/// shot, which ends alone, whichever group it would be in: the shot that
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

/// When the copy for the group of a split that waits fails, only the shots
/// that wait fail, as the group's own state was not touched: on a coin,
/// the shot that reads 0 goes on, the fewer, and ends, and the run names
/// the first of the two that wait, not the shot before them. Each row is
/// how the copy fails, and the gates the run applies: as it is projected
/// on One, while the group going on applies an X on a second qubit; or, a
/// coin read and reset, as the X that returns it to |0⟩ fails, which the
/// group going on, at 0, does not need.
#[test]
fn a_failed_waiting_copy_fails_only_the_shots_that_wait() {
    let shots = a_zero_then_two_ones();
    let rows = [
        (
            FailingRun {
                scene: "a failed projection".to_string(),
                engine: ReferenceSpy::failing_projections_on_one,
                seed: 5,
                program: program(2, &[&[gate1(OPID_H, 0), mz(0), gate1(OPID_X, 1), ret()]]),
                shots: shots.to_vec(),
            },
            2,
        ),
        (
            FailingRun {
                scene: "a failed reset".to_string(),
                engine: ReferenceSpy::failing_x_gates,
                seed: 5,
                program: program(
                    1,
                    &[&[gate1(OPID_H, 0), measure(OPID_MRESETZ, 0, 0), ret()]],
                ),
                shots: shots.to_vec(),
            },
            1,
        ),
    ];
    for (scene_run, applies) in rows {
        let scene = &scene_run.scene;
        run(
            &mut MpsResumer::new((scene_run.engine)(), scene_run.seed),
            &scene_run.program,
            &shots[..1],
        )
        .expect("the shot that reads 0 ends alone");
        let (resumer, named, failure) = scene_run.run();
        assert_eq!((named, failure), (shots[1], Failure::Engine), "{scene}");
        assert_eq!(resumer.engine().applies(), applies, "{scene}");
    }
}

/// A run whose stop flag is already set fails before any gate, naming its
/// first shot.
#[test]
fn a_set_stop_flag_fails_the_run_before_any_gate() {
    let stop_flag = Arc::new(AtomicBool::new(true));
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7).with_stop_flag(stop_flag);
    let result = run(&mut resumer, &coins(4), &ids(&[3, 1, 2]));
    assert!(
        matches!(
            result,
            Err(QirResumeError::Resumer {
                shot: QirShotId(3),
                error: MpsResumerError::Stopped,
            })
        ),
        "{result:?}"
    );
    assert_eq!(resumer.engine().applies(), 0);
}

/// A flag set during a batch lets the group under way run the rest of it,
/// and stops each group split off by a draw before it runs on: set at the
/// first gate of eight coins on 16 shots, a program of one batch whose
/// draws split the shots into many groups, the run applies the eight gates
/// of the group under way, none of the groups split off, and names shot 0,
/// which was in a group that split off and had not ended.
#[test]
fn a_stop_flag_set_during_a_run_stops_it_once_the_batch_under_way_ends() {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let engine = ReferenceSpy::stopping_at_apply(1, Arc::clone(&stop_flag));
    let mut resumer = MpsResumer::new(engine, 7).with_stop_flag(stop_flag);
    let result = run(&mut resumer, &coins(8), &QirShotId::first(16));
    assert!(
        matches!(
            result,
            Err(QirResumeError::Resumer {
                shot: QirShotId(0),
                error: MpsResumerError::Stopped,
            })
        ),
        "{result:?}"
    );
    assert_eq!(
        resumer.engine().applies(),
        8,
        "the gates of the group under way, none of the groups split off"
    );
}

/// A stopped run names the first shot that had not ended, not the first of
/// the run: of three shots, the first reads 0 on its coin and the others
/// read 1. The first has no more gates, and ends before the others go on,
/// as the smaller group goes on first; the others' next batch
/// flips and reads a second qubit, and the flag, set at the flip, stops the
/// run before the batch after, naming the second shot.
#[test]
fn a_stopped_run_names_the_first_shot_that_had_not_ended() {
    let (more_batches, end) = (1, 2);
    let more_batches_after_a_one = program(
        2,
        &[
            &[
                gate1(OPID_H, 0),
                mz(0),
                read_result(0, 0),
                branch(0, more_batches, end),
            ],
            &[
                gate1(OPID_X, 1),
                mz(1),
                read_result(1, 1),
                gate1(OPID_H, 1),
                mz(1),
                ret(),
            ],
            &[ret()],
        ],
    );
    let shots = ids(&[0, 1, 8]);
    let unstopped = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &more_batches_after_a_one,
        &shots,
    )
    .expect("the program runs");
    assert_eq!(
        (unstopped[0][0], unstopped[1][0], unstopped[2][0]),
        (
            OutputRecord::Result(MeasurementResult::Zero),
            OutputRecord::Result(MeasurementResult::One),
            OutputRecord::Result(MeasurementResult::One)
        ),
        "the first shot's coin reads 0, the others' 1"
    );
    let stop_flag = Arc::new(AtomicBool::new(false));
    let engine = ReferenceSpy::stopping_at_apply(2, Arc::clone(&stop_flag));
    let mut resumer = MpsResumer::new(engine, 7).with_stop_flag(stop_flag);
    let result = run(&mut resumer, &more_batches_after_a_one, &shots);
    assert!(
        matches!(
            result,
            Err(QirResumeError::Resumer {
                shot: QirShotId(1),
                error: MpsResumerError::Stopped,
            })
        ),
        "{result:?}"
    );
}

/// The flag is read before each batch, not once per group, whatever that
/// batch would do: three shots run a first batch (a gate, then `mz`); set
/// at that gate, the flag stops the run before a second batch, naming the
/// first of the run's shots, none of which had ended, after only that gate;
/// with no second batch, the shots end and the run returns their records.
/// Each row is what follows the first batch: a second batch that runs
/// gates chosen by a coin, traps (with the shots of one value, or split by
/// a coin, where the run's first shot, alone in drawing One, goes on
/// first), or is refused (a noise call, a gate on one qubit twice); or no
/// second batch.
#[test]
fn a_stop_flag_is_read_before_each_batch() {
    let read_then = |first: u64, then: &[Instruction<u64>]| {
        let mut block = vec![gate1(first, 0), mz(0), read_result(0, 0)];
        block.extend_from_slice(then);
        block
    };
    // A branch on a register that holds neither 0 nor 1 traps.
    let a_trap = [set(0, 2), branch(0, 1, 1)];
    let shots = ids(&[1, 2, 0]);
    let coin = program(1, &[&[gate1(OPID_H, 0), mz(0), ret()]]);
    assert_eq!(
        run(
            &mut MpsResumer::new(ExactReference::new(), 7),
            &coin,
            &shots
        )
        .expect("the program runs"),
        vec![results(&[1]), results(&[0]), results(&[0])],
        "the run's first shot alone draws One on the coin"
    );
    let rows: [(&str, QirProgram, Option<u8>); 6] = [
        (
            "gates chosen by a coin",
            program(
                2,
                &[
                    &read_then(OPID_H, &[branch(0, 1, 2)]),
                    &[gate1(OPID_X, 1), gate1(OPID_H, 1), ret()],
                    &[gate1(OPID_H, 1), gate1(OPID_X, 1), ret()],
                ],
            ),
            None,
        ),
        (
            "a trap",
            program(1, &[&read_then(OPID_X, &a_trap), &[ret()]]),
            None,
        ),
        (
            "a trap, the shots split by a coin",
            program(1, &[&read_then(OPID_H, &a_trap), &[ret()]]),
            None,
        ),
        (
            "a noise call",
            program(1, &[&read_then(OPID_X, &[readout_noise(0.1, 0), ret()])]),
            None,
        ),
        (
            "a gate on one qubit twice",
            program(2, &[&read_then(OPID_X, &[gate2(OPID_CX, 1, 1), ret()])]),
            None,
        ),
        (
            "no batch: the shots end",
            program(1, &[&[gate1(OPID_X, 0), mz(0), ret()]]),
            Some(1),
        ),
    ];
    for (second_batch, program, ended) in rows {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let engine = ReferenceSpy::stopping_at_apply(1, Arc::clone(&stop_flag));
        let mut resumer = MpsResumer::new(engine, 7).with_stop_flag(stop_flag);
        let result = run(&mut resumer, &program, &shots);
        match ended {
            Some(value) => assert_eq!(
                result.expect("the shots ended"),
                vec![results(&[value]); 3],
                "{second_batch}"
            ),
            None => assert!(
                matches!(
                    result,
                    Err(QirResumeError::Resumer {
                        shot: QirShotId(1),
                        error: MpsResumerError::Stopped,
                    })
                ),
                "{second_batch}: {result:?}"
            ),
        }
        assert_eq!(
            resumer.engine().applies(),
            1,
            "{second_batch}: only the first batch's gate applied"
        );
    }
}

/// A flag that is never set changes no record.
#[test]
fn an_unset_stop_flag_changes_no_record() {
    let shots = QirShotId::first(16);
    let without = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &coins(4),
        &shots,
    )
    .expect("the program runs");
    let stop_flag = Arc::new(AtomicBool::new(false));
    let mut resumer = MpsResumer::new(ExactReference::new(), 7).with_stop_flag(stop_flag);
    let with = run(&mut resumer, &coins(4), &shots).expect("the flag is never set");
    assert_eq!(with, without);
}
