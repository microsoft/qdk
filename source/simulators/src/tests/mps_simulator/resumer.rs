// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The resumer's rules: what a shot records, wherever the order places its
//! qubits; that its records do not depend on the shots beside it or on
//! earlier runs; how shots share states; and how a run fails.

mod order;
mod splitmix64;

use std::num::NonZeroUsize;

use crate::bytecode::Instruction;
use crate::qir::{
    QirInstruction, QirMachine, QirProgram, QirResumeError, QirResumer, QirShotId, QirTrap,
    expect_ready,
};
use crate::{MeasurementResult, OutputRecord};
use tensornet::MpsEngine;
use tensornet::reference::ExactReference;

use super::engines::{Observed, ObservedError};
use super::programs::{
    OPID_CX, OPID_CZ, OPID_H, OPID_MOVE, OPID_MRESETZ, OPID_MZ, OPID_RY, OPID_SWAP, OPID_X, branch,
    coin_copied_by_a_branch, coins, correlated_noise, gate, gate1, gate2, jump, measure, mz,
    peek_loss, program, program_with, read_result, readout_noise, reset, ret, set,
};
use crate::mps_simulator::resumer::order::QubitOrder;
use crate::mps_simulator::resumer::splitmix64::SplitMix64;
use crate::mps_simulator::{MpsResumer, MpsResumerError};

type RunResult<E> = Result<Vec<Vec<OutputRecord>>, QirResumeError<MpsResumerError<E>>>;

/// Runs `shots` of `program` on `resumer`.
fn run<E: MpsEngine>(
    resumer: &mut MpsResumer<E>,
    program: &QirProgram,
    shots: &[QirShotId],
) -> RunResult<E::Error> {
    expect_ready(resumer.run(&QirMachine::new(program), shots))
}

/// The records of a shot that read `bits`, as results.
fn results(bits: &[u8]) -> Vec<OutputRecord> {
    bits.iter()
        .map(|&bit| {
            OutputRecord::Result(if bit == 1 {
                MeasurementResult::One
            } else {
                MeasurementResult::Zero
            })
        })
        .collect()
}

fn ids(ids: &[u32]) -> Vec<QirShotId> {
    ids.iter().copied().map(QirShotId).collect()
}

/// A program that flips qubit 0, copies it to qubit 3 and from there to
/// qubit 1 (the control first, so a gate applied with its operands swapped
/// would leave qubit 1 at 0), then swaps qubits 1 and 2, and reads all four:
/// 1, 0, 1, 1.
fn copies() -> QirProgram {
    program(
        4,
        &[&[
            gate1(OPID_X, 0),
            gate2(OPID_CX, 0, 3),
            gate2(OPID_CX, 0, 3),
            gate2(OPID_CX, 0, 3),
            gate2(OPID_CX, 3, 1),
            gate2(OPID_SWAP, 1, 2),
            mz(0),
            mz(1),
            mz(2),
            mz(3),
            ret(),
        ]],
    )
}

/// A program of no randomness records its basis state, whichever sites
/// the method places its qubits on: here the order moves them.
#[test]
fn a_deterministic_program_records_its_basis_state() {
    let program = copies();
    assert_ne!(
        QubitOrder::for_program(&program),
        QubitOrder::identity(4),
        "the program's gates move its qubits off their own sites"
    );
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    let records = run(&mut resumer, &program, &ids(&[0, 1])).expect("the program runs");
    assert_eq!(records, vec![results(&[1, 0, 1, 1]); 2]);
}

/// The values of qubits 0 to 3 in [`moved`]'s programs.
const MOVED_VALUES: [u8; 4] = [1, 0, 0, 1];

/// A program on four qubits whose `cz`s (no change to a basis state) make
/// the order move every qubit to a site that, in the identity order, holds
/// a qubit of the other value; `x`s give the qubits [`MOVED_VALUES`]; then
/// `tail`, into 4 results.
fn moved(tail: &[Instruction<u64>]) -> QirProgram {
    let mut block = vec![gate1(OPID_X, 0), gate1(OPID_X, 3)];
    for (a, b) in [(1, 3), (3, 0), (0, 2)] {
        block.extend([
            gate2(OPID_CZ, a, b),
            gate2(OPID_CZ, a, b),
            gate2(OPID_CZ, a, b),
        ]);
    }
    block.extend_from_slice(tail);
    block.push(ret());
    let program = program_with(4, 4, &[], &[&block]);
    let order = QubitOrder::for_program(&program);
    for qubit in 0..4 {
        let site = order.site(qubit);
        assert_ne!(
            MOVED_VALUES[qubit as usize], MOVED_VALUES[site],
            "qubit {qubit} sits on site {site}, whose qubit in the identity order has the other value"
        );
    }
    program
}

/// `mz` reads each qubit where the order placed it.
#[test]
fn a_measurement_reads_its_qubit_where_the_order_placed_it() {
    let program = moved(&[mz(0), mz(1), mz(2), mz(3)]);
    let records = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &program,
        &ids(&[0]),
    )
    .expect("the program runs");
    assert_eq!(records, vec![results(&MOVED_VALUES)]);
}

/// `mresetz` reads and resets its qubit where the order placed it: qubit 3
/// reads 1, then 0.
#[test]
fn a_measure_reset_acts_on_its_qubit_where_the_order_placed_it() {
    let program = moved(&[measure(OPID_MRESETZ, 3, 0), measure(OPID_MZ, 3, 1)]);
    let records = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &program,
        &ids(&[0]),
    )
    .expect("the program runs");
    assert_eq!(records, vec![results(&[1, 0, 0, 0])]);
}

/// `reset` resets its qubit where the order placed it: qubit 0 reads 0
/// after it, and qubit 3 keeps its 1.
#[test]
fn a_reset_acts_on_its_qubit_where_the_order_placed_it() {
    let program = moved(&[reset(0), measure(OPID_MZ, 0, 0), measure(OPID_MZ, 3, 1)]);
    let records = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &program,
        &ids(&[0]),
    )
    .expect("the program runs");
    assert_eq!(records, vec![results(&[0, 1, 0, 0])]);
}

/// A Bell pair's two readouts agree in every shot, and both values occur
/// among 32 shots.
#[test]
fn a_bell_pairs_readouts_agree() {
    let bell = program(
        2,
        &[&[gate1(OPID_H, 0), gate2(OPID_CX, 0, 1), mz(0), mz(1), ret()]],
    );
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    let records = run(&mut resumer, &bell, &QirShotId::first(32)).expect("the program runs");
    assert!(records.iter().all(|shot| shot[0] == shot[1]));
    assert!(records.contains(&results(&[0, 0])) && records.contains(&results(&[1, 1])));
}

/// A mid-circuit measurement steers its own shot's later gates: a branch
/// on a coin copies it, in every shot, both values occurring.
#[test]
fn a_measurement_steers_its_shots_later_gates() {
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    let records = run(
        &mut resumer,
        &coin_copied_by_a_branch(),
        &QirShotId::first(32),
    )
    .expect("the program runs");
    assert!(records.iter().all(|shot| shot[0] == shot[1]));
    assert!(records.contains(&results(&[0, 0])) && records.contains(&results(&[1, 1])));
}

/// A reset returns a qubit in superposition to |0⟩, in every shot.
#[test]
fn a_reset_returns_a_qubit_to_zero() {
    let program = program(1, &[&[gate1(OPID_H, 0), reset(0), mz(0), ret()]]);
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    let records = run(&mut resumer, &program, &QirShotId::first(16)).expect("the program runs");
    assert_eq!(records, vec![results(&[0]); 16]);
}

/// `mresetz` reads a qubit's value, then leaves it at |0⟩.
#[test]
fn a_measure_reset_reads_the_value_then_leaves_zero() {
    let program = program(
        2,
        &[&[
            gate1(OPID_X, 0),
            measure(OPID_MRESETZ, 0, 0),
            measure(OPID_MZ, 0, 1),
            ret(),
        ]],
    );
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    let records = run(&mut resumer, &program, &ids(&[0])).expect("the program runs");
    assert_eq!(records, vec![results(&[1, 0])]);
}

/// `move` changes no state: a qubit flipped to 1 and moved still reads 1.
#[test]
fn a_move_changes_no_state() {
    let program = program(1, &[&[gate1(OPID_X, 0), gate1(OPID_MOVE, 0), mz(0), ret()]]);
    let records = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &program,
        &ids(&[0]),
    )
    .expect("the program runs");
    assert_eq!(records, vec![results(&[1])]);
}

/// A shot records alone what it records among other shots, whose draws
/// split them into groups of other histories.
#[test]
fn a_shot_records_the_same_alone_as_among_other_shots() {
    let program = coins(3);
    let shots = QirShotId::first(16);
    let together = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &program,
        &shots,
    )
    .expect("the program runs");
    for (place, &shot) in shots.iter().enumerate() {
        let alone = run(
            &mut MpsResumer::new(ExactReference::new(), 7),
            &program,
            &[shot],
        )
        .expect("the program runs");
        assert_eq!(alone, vec![together[place].clone()], "{shot}");
    }
}

/// A shot's records follow it to any place among the run's shots: the
/// shots in reverse order give the same records, reversed.
#[test]
fn a_shot_records_the_same_at_any_place_among_the_shots() {
    let program = coin_copied_by_a_branch();
    let shots = QirShotId::first(16);
    let reversed: Vec<_> = shots.iter().rev().copied().collect();
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    let forward = run(&mut resumer, &program, &shots).expect("the program runs");
    let mut backward = run(&mut resumer, &program, &reversed).expect("the program runs");
    backward.reverse();
    assert_eq!(forward, backward);
}

/// A later run on the same resumer gives a shot the records an earlier one
/// gave it.
#[test]
fn a_shot_records_the_same_in_a_later_run() {
    let program = coins(4);
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    let first = run(&mut resumer, &program, &QirShotId::first(8)).expect("the program runs");
    let later = run(&mut resumer, &program, &QirShotId::first(8)).expect("the program runs");
    assert_eq!(first, later);
}

/// The resumer's seed selects the draws: another seed gives other records.
#[test]
fn another_seed_gives_other_records() {
    let program = coins(8);
    let shots = QirShotId::first(8);
    let one = run(
        &mut MpsResumer::new(ExactReference::new(), 1),
        &program,
        &shots,
    )
    .expect("the program runs");
    let other = run(
        &mut MpsResumer::new(ExactReference::new(), 2),
        &program,
        &shots,
    )
    .expect("the program runs");
    assert_ne!(one, other);
}

/// A shot draws from the stream the executor protocol (planned) will seed
/// for that shot of a chain's only participant that draws, at place 0, as
/// this method is in a chain of a driver and itself: so such a chain, its
/// executor drawing as this method does, will reproduce the records a
/// resumer gives. Sixteen coins, read in order,
/// are the first sixteen uniforms of that stream, each against ½, for
/// several seeds and shots.
#[test]
fn a_shot_draws_from_the_executor_protocols_stream_of_the_only_drawing_participant() {
    // The protocol's derivation: with `mix(x)` the first value of
    // `SplitMix64::new(x)`, a participant's seed is
    // `mix(run ^ mix(place + 1))`, and its shot's `mix(participant ^ shot)`.
    fn mix(x: u64) -> u64 {
        SplitMix64::new(x).next_u64()
    }
    let program = coins(16);
    for seed in [0, 7, u64::MAX] {
        for shot in [0, 1, 1000] {
            let participant = mix(seed ^ mix(1));
            let mut stream = SplitMix64::new(mix(participant ^ u64::from(shot)));
            let bits: Vec<u8> = (0..16)
                .map(|_| u8::from(stream.next_uniform() < 0.5))
                .collect();
            let records = run(
                &mut MpsResumer::new(ExactReference::new(), seed),
                &program,
                &ids(&[shot]),
            )
            .expect("the program runs");
            assert_eq!(records, vec![results(&bits)], "seed {seed}, shot {shot}");
        }
    }
}

/// Shots that draw the same values share one state: a program of no
/// randomness prepares one state for all its shots and forks none.
#[test]
fn shots_with_the_same_draws_share_one_state() {
    let mut resumer = MpsResumer::new(Observed::new(), 7);
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
    let mut resumer = MpsResumer::new(Observed::new(), 7);
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
    let mut resumer = MpsResumer::new(Observed::new(), 7);
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
    let make = || MpsResumer::new(Observed::new(), 7);
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
    let make = || MpsResumer::new(Observed::failing_projections_on_one(), 5);
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
                    error: MpsResumerError::Engine(ObservedError::Device),
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
            run(&mut MpsResumer::new(Observed::new(), 5), coin, &[shot]).expect("a coin runs")
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
const FAILING_SPLITS: [fn() -> Observed; 2] = [
    Observed::failing_forks,
    Observed::failing_projections_on_one,
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
                    error: MpsResumerError::Engine(ObservedError::Device),
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
                    error: MpsResumerError::Engine(ObservedError::Device),
                } if shot == one
            ),
            "{zero_last:?}"
        );
    }
}

/// A max bond dimension that makes no chain of the program's qubits fails
/// the run, naming its first shot, before any state is prepared: 64 qubits
/// with unbounded bonds.
#[test]
fn a_max_bond_dimension_that_makes_no_chain_fails_naming_the_first_shot() {
    let program = program(64, &[&[gate1(OPID_H, 0), ret()]]);
    let mut resumer =
        MpsResumer::new(Observed::new(), 7).with_max_bond_dimension(NonZeroUsize::MAX);
    let error = run(&mut resumer, &program, &ids(&[5, 2])).expect_err("no chain");
    assert!(
        matches!(
            error,
            QirResumeError::Resumer {
                shot: QirShotId(5),
                error: MpsResumerError::Capacity { qubits: 64, .. },
            }
        ),
        "{error:?}"
    );
    assert!(resumer.engine().prepared().is_empty());
}

/// The refusal of a noise or loss call: its shot fails, naming the
/// instruction and the planned item.
fn assert_refused(program: &QirProgram, instruction: QirInstruction) {
    let error = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        program,
        &ids(&[3, 4]),
    )
    .expect_err("noise and loss are not run");
    let QirResumeError::Resumer { shot, error } = error else {
        panic!("the resumer refuses the call, no trap: {error:?}");
    };
    assert_eq!(shot, QirShotId(3));
    assert!(
        matches!(&error, MpsResumerError::Unsupported { instruction: called } if *called == instruction),
        "{error:?}"
    );
    assert!(error.to_string().contains("planned"), "{error}");
}

/// Readout noise is not run on an MPS yet: a shot that reaches it fails.
#[test]
fn a_readout_noise_call_fails_its_shot_naming_the_planned_item() {
    let noisy = program(1, &[&[readout_noise(0.1, 0), ret()]]);
    assert_refused(&noisy, QirInstruction::ReadoutNoise);
}

/// Correlated noise is not run on an MPS yet: a shot that reaches it fails.
#[test]
fn a_correlated_noise_call_fails_its_shot_naming_the_planned_item() {
    let noisy = program_with(1, 1, &[0], &[&[set(0, 0), correlated_noise(1, 0), ret()]]);
    assert_refused(&noisy, QirInstruction::CorrelatedNoise);
}

/// A loss check is not run on an MPS yet: a shot that reaches it fails.
#[test]
fn a_loss_check_fails_its_shot_naming_the_planned_item() {
    let lossy = program(1, &[&[peek_loss(0, 0), ret()]]);
    assert_refused(&lossy, QirInstruction::PeekLoss);
}

/// A noise call on a path no shot takes fails nothing: a call is refused
/// when a shot reaches it.
#[test]
fn a_noise_call_no_shot_reaches_fails_nothing() {
    let (noisy, clean) = (1, 2);
    let program = program(
        1,
        &[
            &[set(0, 0), branch(0, noisy, clean)],
            &[readout_noise(0.1, 0), jump(clean)],
            &[ret()],
        ],
    );
    let records = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &program,
        &ids(&[0]),
    )
    .expect("no shot reaches the noise");
    assert_eq!(records, vec![results(&[0])]);
}

/// A two-qubit gate on one qubit twice fails its shot, naming the qubit.
#[test]
fn a_gate_on_one_qubit_twice_fails_naming_the_qubit() {
    let program = program(2, &[&[gate2(OPID_CX, 1, 1), ret()]]);
    let error = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &program,
        &ids(&[0]),
    )
    .expect_err("cx(1, 1) has no matrix on distinct qubits");
    assert!(matches!(
        error,
        QirResumeError::Resumer {
            error: MpsResumerError::RepeatedQubit { qubit: 1, .. },
            ..
        }
    ));
}

/// No shots, no records.
#[test]
fn no_shots_give_no_records() {
    let records = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &coins(2),
        &[],
    )
    .expect("no shot fails");
    assert!(records.is_empty());
}

/// A program of no qubits runs with no state: its shots record its
/// results, all `Zero`.
#[test]
fn a_program_of_no_qubits_records_without_a_state() {
    let program = program(0, &[&[ret()]]);
    let mut resumer = MpsResumer::new(Observed::new(), 7);
    let records = run(&mut resumer, &program, &ids(&[0, 1])).expect("the program runs");
    assert_eq!(records, vec![results(&[0]); 2]);
    assert!(resumer.engine().prepared().is_empty());
}
