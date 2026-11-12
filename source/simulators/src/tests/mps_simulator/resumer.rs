// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The resumer's rules: what a shot records; that its records do not
//! depend on the shots beside it or on earlier runs; the layout and the
//! capacity each run starts with, and the gate fusion its gates reach the
//! engine with; and a run's edges (no shots, no qubits, a max bond
//! dimension that makes no chain). How shots draw, share states and fail
//! is in `resumer/grouped_run.rs`, the stream each draws from in
//! `resumer/shot_stream.rs`, the calls' sites and refusals in
//! `resumer/lowering.rs`.

mod grouped_run;
mod layout;
mod lowering;
mod report;
mod rewrites;
mod shot_stream;

use std::num::NonZeroUsize;

use crate::qir::{QirMachine, QirProgram, QirResumeError, QirResumer, QirShotId, expect_ready};
use crate::{MeasurementResult, OutputRecord};
use tensornet::reference::ExactReference;
use tensornet::{MpsEngine, MpsShape};

use super::programs::{
    bytecode, coin_copied_by_a_branch, coins, gate1, gate2, measure, mz, program, program_with,
    reset, ret,
};
use super::reference_spy::ReferenceSpy;
use crate::bytecode::encoding::{
    OPID_CX, OPID_H, OPID_MOVE, OPID_MRESETZ, OPID_MZ, OPID_SWAP, OPID_X,
};
use crate::mps_simulator::resumer::layout::QubitOrder;
use crate::mps_simulator::{DEFAULT_MAX_BOND_DIMENSION, GateFusion, MpsResumer, MpsResumerError};

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

/// The run places the qubits where the layout it chose from the program
/// puts them: two Bell pairs, one on qubits 0 and 3 and one on qubits 1
/// and 2, as a program that entangles qubits far apart in its numbering.
/// The layout puts each pair on neighbouring sites, so no bond holds more
/// than 2; on the identity order, both pairs would cross the middle bond,
/// of 4.
#[test]
fn the_run_places_the_qubits_by_the_layout_it_chose() {
    let program = program(
        4,
        &[&[
            gate1(OPID_H, 0),
            gate2(OPID_CX, 0, 3),
            gate1(OPID_H, 1),
            gate2(OPID_CX, 1, 2),
            mz(0),
            mz(1),
            mz(2),
            mz(3),
            ret(),
        ]],
    );
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    run(&mut resumer, &program, &ids(&[0])).expect("the program runs");
    assert_eq!(resumer.last_run().largest_bond, 2);
}

/// The resumer's gate fusion reaches the engine, `Pairs` by default: a Bell
/// pair turned to the X basis (H on qubit 0, CX, H on qubit 1), then read.
/// Fused in pairs, its three gates reach the engine as one operator; set
/// `Unfused`, as three.
#[test]
fn gates_reach_the_engine_grouped_as_the_gate_fusion_says() {
    let program = program(
        2,
        &[&[
            gate1(OPID_H, 0),
            gate2(OPID_CX, 0, 1),
            gate1(OPID_H, 1),
            mz(0),
            mz(1),
            ret(),
        ]],
    );
    for (fusion, applies) in [(None, 1), (Some(GateFusion::Unfused), 3)] {
        let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
        if let Some(fusion) = fusion {
            resumer = resumer.with_gate_fusion(fusion);
        }
        run(&mut resumer, &program, &ids(&[0])).expect("the program runs");
        assert_eq!(resumer.engine().applies(), applies, "{fusion:?}");
    }
}

/// A Bell pair's two readouts agree in every shot.
#[test]
fn a_bell_pairs_readouts_agree() {
    // The program: a Bell pair read into two results, written in the
    // Adaptive Profile bytecode, then checked.
    let bytecode = bytecode(
        2,
        &[&[gate1(OPID_H, 0), gate2(OPID_CX, 0, 1), mz(0), mz(1), ret()]],
    );
    let bell = QirProgram::new(bytecode).expect("the Bell pair's bytecode has no fault");
    // The resumer, on the exact reference engine, its draws seeded with 7.
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    // One machine runs the program's classical part; the resumer runs the
    // quantum calls of its 32 shots, and never waits.
    let shots = QirShotId::first(32);
    let records =
        expect_ready(resumer.run(&QirMachine::new(&bell), &shots)).expect("the program runs");
    assert!(
        records.contains(&results(&[0, 0])) && records.contains(&results(&[1, 1])),
        "32 shots draw both values"
    );
    // In every shot, the two readouts agree.
    assert!(records.iter().all(|shot| shot[0] == shot[1]));
}

/// A mid-circuit measurement steers its own shot's later gates: a branch
/// on a coin copies it, in every shot.
#[test]
fn a_measurement_steers_its_shots_later_gates() {
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    let records = run(
        &mut resumer,
        &coin_copied_by_a_branch(),
        &QirShotId::first(32),
    )
    .expect("the program runs");
    assert!(
        records.contains(&results(&[0, 0])) && records.contains(&results(&[1, 1])),
        "32 shots draw both values"
    );
    assert!(records.iter().all(|shot| shot[0] == shot[1]));
}

/// A reset returns a qubit in superposition to |0⟩, in every shot.
#[test]
fn a_reset_returns_a_qubit_to_zero() {
    let program = program(1, &[&[gate1(OPID_H, 0), reset(0), mz(0), ret()]]);
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    let records = run(&mut resumer, &program, &QirShotId::first(16)).expect("the program runs");
    assert_eq!(records, vec![results(&[0]); 16]);
}

/// `mresetz` reads a qubit's value, then leaves it at |0⟩, whichever value
/// it read: qubit 0, flipped to 1, reads 1 then 0; qubit 1, left at 0,
/// reads 0 then 0.
#[test]
fn a_measure_reset_reads_the_value_then_leaves_zero() {
    let program = program_with(
        2,
        4,
        &[],
        &[&[
            gate1(OPID_X, 0),
            measure(OPID_MRESETZ, 0, 0),
            measure(OPID_MZ, 0, 1),
            measure(OPID_MRESETZ, 1, 2),
            measure(OPID_MZ, 1, 3),
            ret(),
        ]],
    );
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    let records = run(&mut resumer, &program, &ids(&[0])).expect("the program runs");
    assert_eq!(records, vec![results(&[1, 0, 0, 0])]);
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
