// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The resumer's rules: what a shot records, wherever the order places its
//! qubits; that its records do not depend on the shots beside it or on
//! earlier runs; the capacity each state is prepared within; and a run's
//! edges (no shots, no qubits, a max bond dimension that makes no chain).
//! How shots draw, share states and fail is in `resumer/grouped_run.rs`,
//! the stream each draws from in `resumer/shot_stream.rs`, the calls
//! refused in `resumer/lowering.rs`.

mod fusion;
mod grouped_run;
mod lowering;
mod qubit_order;
mod report;
mod shot_stream;

use std::num::NonZeroUsize;

use crate::bytecode::Instruction;
use crate::qir::{QirMachine, QirProgram, QirResumeError, QirResumer, QirShotId, expect_ready};
use crate::{MeasurementResult, OutputRecord};
use tensornet::reference::ExactReference;
use tensornet::{MpsEngine, MpsShape};

use super::programs::{
    coin_copied_by_a_branch, coins, gate1, gate2, measure, mz, program, program_with, reset, ret,
};
use super::reference_spy::ReferenceSpy;
use crate::bytecode::encoding::{
    OPID_CX, OPID_CZ, OPID_H, OPID_MOVE, OPID_MRESETZ, OPID_MZ, OPID_SWAP, OPID_X,
};
use crate::mps_simulator::resumer::qubit_order::QubitOrder;
use crate::mps_simulator::{DEFAULT_MAX_BOND_DIMENSION, MpsResumer, MpsResumerError};

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

/// The ids `ids`, as shots.
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
/// the MPS simulation method places its qubits on: here the order moves
/// them.
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
    // The program: a Bell pair read into two results, in the bytecode the
    // tests write.
    let bell = program(
        2,
        &[&[gate1(OPID_H, 0), gate2(OPID_CX, 0, 1), mz(0), mz(1), ret()]],
    );
    // The resumer, on the exact reference engine, its draws seeded with 7.
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    // One machine runs the program's classical part; the resumer runs the
    // quantum calls of its 32 shots, and never waits.
    let shots = QirShotId::first(32);
    let records =
        expect_ready(resumer.run(&QirMachine::new(&bell), &shots)).expect("the program runs");
    // In every shot, the two readouts agree.
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

/// A max bond dimension that makes no chain of the program's qubits fails
/// the run, naming its first shot, before any state is prepared: 64 qubits
/// with unbounded bonds.
#[test]
fn a_max_bond_dimension_that_makes_no_chain_fails_naming_the_first_shot() {
    let program = program(64, &[&[gate1(OPID_H, 0), ret()]]);
    let mut resumer =
        MpsResumer::new(ReferenceSpy::new(), 7).with_max_bond_dimension(NonZeroUsize::MAX);
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

/// Each state is prepared within `MpsShape::qubit_chain(n, χ)`, n the
/// program's qubits and χ the max bond dimension: 128 by default, else the
/// one set.
#[test]
fn each_state_is_prepared_within_a_qubit_chain_of_the_max_bond_dimension() {
    let program = program(3, &[&[gate1(OPID_H, 0), gate2(OPID_CX, 0, 1), ret()]]);
    for (max_bond_dimension, expected) in [(None, DEFAULT_MAX_BOND_DIMENSION.get()), (Some(1), 1)] {
        let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
        if let Some(max_bond_dimension) = max_bond_dimension {
            resumer = resumer
                .with_max_bond_dimension(NonZeroUsize::new(max_bond_dimension).expect("not zero"));
        }
        run(&mut resumer, &program, &ids(&[0])).expect("the program runs");
        let capacity = MpsShape::qubit_chain(3, expected).expect("three sites form a chain");
        assert_eq!(resumer.engine().prepared(), vec![capacity]);
    }
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
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
    let records = run(&mut resumer, &program, &ids(&[0, 1])).expect("the program runs");
    assert_eq!(records, vec![results(&[0]); 2]);
    assert!(resumer.engine().prepared().is_empty());
}
