// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `MpsResumer`'s rules, checked through `QirResumer::run`: what a shot
//! records; that its records do not depend on the shots beside it or on
//! earlier runs; the capacity each run starts with, and its qubit order's
//! effect on the bonds; that a run does its work when called; and a run's
//! edges (no shots, a max bond dimension that makes no chain). How shots
//! draw, share states and fail is in `mps_simulator/grouped_run/tests.rs`
//! and the tests beside its parts, the RNG each draws from in
//! `mps_simulator/shot_rng/tests.rs`, the calls' sites and refusals in
//! `mps_simulator/lowering/tests.rs`. The tests run on an exact engine,
//! or on the spy over one in `mps_simulator/test_utils/reference_spy.rs`.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::OutputRecord;
use crate::qir::{QirMachine, QirProgram, QirResumeError, QirResumer, QirShotId, expect_ready};
use tensornet::MpsShape;
use tensornet::reference::ExactReference;

use crate::bytecode::encoding::{OPID_CX, OPID_H};
use crate::mps_simulator::test_utils::programs::{
    bytecode, coin_copied_by_a_branch, coins, gate1, gate2, mz, program, ret,
};
use crate::mps_simulator::test_utils::reference_spy::ReferenceSpy;
use crate::mps_simulator::test_utils::{LONE_SHOT_CALLS, Records, bell, ids, results, run};
use crate::mps_simulator::{DEFAULT_MAX_BOND_DIMENSION, MpsResumer, MpsResumerError};

/// A run lays its qubits out in the program's qubit order, which keeps
/// its bonds small. The order itself is checked in
/// `mps_simulator/qubit_order/tests.rs`. Here, two Bell pairs, on qubits
/// 0 and 3 and on qubits 1 and 2, keep every bond at 2; in numbering
/// order, both pairs would cross the middle bond, of 4.
#[test]
fn a_run_lays_its_qubits_out_in_the_programs_qubit_order() {
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
    assert_eq!(resumer.last_run().largest_bond_dimension, 2);
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

/// A shot's records depend only on the resumer's seed and the shot's id:
/// not on the shots beside it, whose draws split them into other groups,
/// nor on its place among them, nor on earlier runs, even one that an
/// engine's failure or the stop flag ended, nor on a stop flag that is not
/// set. Each row compares a shot's records in one scene with its records
/// in another.
#[test]
fn a_shots_records_depend_only_on_the_seed_and_the_shot() {
    let rows = [
        alone_as_among_16_shots(),
        reversed_as_in_order(),
        a_later_run_as_the_first(),
        after_a_failed_run_as_on_a_new_resumer(),
        after_stopped_runs_as_on_a_new_resumer(),
    ];
    for (scene, records, expected) in rows.into_iter().flatten() {
        assert_eq!(records, expected, "{scene}");
    }
}

/// A scene, the records of its shots in it, and their records in the scene
/// it is compared with.
type SameRecords = (String, Records, Records);

/// A resumer on the exact reference, with seed 7.
fn exact_resumer() -> MpsResumer<ExactReference> {
    MpsResumer::new(ExactReference::new(), 7)
}

/// Each of 16 shots of three coins, alone, as among the 16.
fn alone_as_among_16_shots() -> Vec<SameRecords> {
    let shots = QirShotId::first(16);
    let together = run(&mut exact_resumer(), &coins(3), &shots).expect("the program runs");
    shots
        .iter()
        .zip(together)
        .map(|(&shot, among)| {
            let alone = run(&mut exact_resumer(), &coins(3), &[shot]).expect("the program runs");
            (
                format!("shot {shot} alone, as among 16"),
                alone,
                vec![among],
            )
        })
        .collect()
}

/// 16 shots of a coin copied by a branch, in reverse order, as in order.
fn reversed_as_in_order() -> Vec<SameRecords> {
    let shots = QirShotId::first(16);
    let reversed: Vec<_> = shots.iter().rev().copied().collect();
    let mut resumer = exact_resumer();
    let forward = run(&mut resumer, &coin_copied_by_a_branch(), &shots).expect("the program runs");
    let mut backward =
        run(&mut resumer, &coin_copied_by_a_branch(), &reversed).expect("the program runs");
    backward.reverse();
    let scene = "16 shots reversed, as in order".to_string();
    vec![(scene, backward, forward)]
}

/// 8 shots of four coins in a later run on the same resumer, as in the
/// first.
fn a_later_run_as_the_first() -> Vec<SameRecords> {
    let mut resumer = exact_resumer();
    let first = run(&mut resumer, &coins(4), &QirShotId::first(8)).expect("the program runs");
    let later = run(&mut resumer, &coins(4), &QirShotId::first(8)).expect("the program runs");
    vec![("a later run, as the first".to_string(), later, first)]
}

/// 8 shots of a Bell pair after a run an engine's failure ended, as on a
/// new resumer, on an engine that fails only the first of one of the calls
/// the run makes.
fn after_a_failed_run_as_on_a_new_resumer() -> Vec<SameRecords> {
    let shots = QirShotId::first(8);
    let new = run(
        &mut MpsResumer::new(ReferenceSpy::new(), 5),
        &bell(),
        &shots,
    )
    .expect("the program runs");
    LONE_SHOT_CALLS
        .into_iter()
        .map(|call| {
            let mut resumer = MpsResumer::new(ReferenceSpy::failing_first(call), 5);
            run(&mut resumer, &bell(), &shots).expect_err("the call fails");
            let next = run(&mut resumer, &bell(), &shots).expect("the engine fails only once");
            let scene = format!("after a run whose {call:?} failed, as on a new resumer");
            (scene, next, new.clone())
        })
        .collect()
}

/// Shots 3, 1 and 2 of four coins after three runs the stop flag ended,
/// once it is cleared, as on a new resumer.
fn after_stopped_runs_as_on_a_new_resumer() -> Vec<SameRecords> {
    let shots = ids(&[3, 1, 2]);
    let stop_flag = Arc::new(AtomicBool::new(true));
    let mut resumer =
        MpsResumer::new(ReferenceSpy::new(), 7).with_stop_flag(Arc::clone(&stop_flag));
    for stopped in [coins(4), program(1, &[&[ret()]]), coins(4)] {
        run(&mut resumer, &stopped, &shots).expect_err("the flag stops the run");
    }
    stop_flag.store(false, Ordering::Relaxed);
    let cleared = run(&mut resumer, &coins(4), &shots).expect("a cleared flag stops nothing");
    let new = run(
        &mut MpsResumer::new(ReferenceSpy::new(), 7),
        &coins(4),
        &shots,
    )
    .expect("the program runs");
    let scene = "after stopped runs, the flag cleared, as on a new resumer".to_string();
    vec![(scene, cleared, new)]
}

/// A max bond dimension that makes no chain of the program's qubits fails
/// the run, naming its first shot, before any state is prepared and before
/// the stop flag is read: 64 qubits with unbounded bonds. Each row is a
/// resumer: one with no stop flag, and one whose flag is already set, which
/// would otherwise fail the run with `Stopped`.
#[test]
fn a_max_bond_dimension_that_makes_no_chain_fails_naming_the_first_shot() {
    let program = program(64, &[&[gate1(OPID_H, 0), ret()]]);
    let unbounded =
        || MpsResumer::new(ReferenceSpy::new(), 7).with_max_bond_dimension(NonZeroUsize::MAX);
    let rows = [
        ("no stop flag", unbounded()),
        (
            "a stop flag set",
            unbounded().with_stop_flag(Arc::new(AtomicBool::new(true))),
        ),
    ];
    for (scene, mut resumer) in rows {
        let error = run(&mut resumer, &program, &ids(&[5, 2])).expect_err(scene);
        assert!(
            matches!(
                error,
                QirResumeError::Resumer {
                    shot: QirShotId(5),
                    error: MpsResumerError::Capacity { qubits: 64, .. },
                }
            ),
            "{scene}: {error:?}"
        );
        assert_eq!(resumer.engine.prepared(), Vec::<MpsShape>::new(), "{scene}");
    }
}

/// Each state is prepared within `MpsShape::qubit_chain(n, χ)`, n the
/// program's qubits and χ the max bond dimension: 128 by default, else the
/// one set. Each row is n, the χ set, and the χ of the chain: 3 qubits by
/// default and at χ = 1; and 64 qubits at χ = 1, a chain however many
/// qubits, which only unbounded bonds would not make. The exact reference
/// holds at most 20 sites, so the 64-qubit run fails in the engine, after
/// the preparation the spy records.
#[test]
fn each_state_is_prepared_within_a_qubit_chain_of_the_max_bond_dimension() {
    let rows = [
        (3, None, DEFAULT_MAX_BOND_DIMENSION.get()),
        (3, Some(1), 1),
        (64, Some(1), 1),
    ];
    for (qubits, max_bond_dimension, expected) in rows {
        let program = program(qubits, &[&[gate1(OPID_H, 0), gate2(OPID_CX, 0, 1), ret()]]);
        let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
        if let Some(max_bond_dimension) = max_bond_dimension {
            resumer = resumer
                .with_max_bond_dimension(NonZeroUsize::new(max_bond_dimension).expect("not zero"));
        }
        // Whether the engine holds the state is not this rule.
        let _ = run(&mut resumer, &program, &ids(&[0]));
        let sites = usize::try_from(qubits).expect("a u32 fits a usize");
        let capacity = MpsShape::qubit_chain(sites, expected).expect("the sites form a chain");
        assert_eq!(resumer.engine.prepared(), vec![capacity], "{qubits} qubits");
    }
}

/// No shots, no records, even with the stop flag set, or with a program
/// whose qubits make no chain: a run of no shots has no shot to stop or to
/// prepare a state for, and leaves the flag set for its caller to clear.
/// Each row is a resumer and a program: no stop flag; a stop flag set; and
/// 64 qubits with unbounded bonds.
#[test]
fn no_shots_give_no_records() {
    let stop_flag = Arc::new(AtomicBool::new(true));
    let rows = [
        (
            "no stop flag",
            MpsResumer::new(ExactReference::new(), 7),
            coins(2),
        ),
        (
            "a stop flag set",
            MpsResumer::new(ExactReference::new(), 7).with_stop_flag(Arc::clone(&stop_flag)),
            coins(2),
        ),
        (
            "a program that makes no chain",
            MpsResumer::new(ExactReference::new(), 7).with_max_bond_dimension(NonZeroUsize::MAX),
            program(64, &[&[gate1(OPID_H, 0), ret()]]),
        ),
    ];
    for (scene, mut resumer, program) in rows {
        let records = run(&mut resumer, &program, &[]).expect(scene);
        assert_eq!(records, Vec::<Vec<OutputRecord>>::new(), "{scene}");
    }
    assert!(stop_flag.load(Ordering::Relaxed), "the flag is left set");
}

/// A run does its work when called, not when its future is polled: a
/// caller that drops the future still finds the run's report. A Bell pair
/// is run and its future dropped unpolled; the report holds the pair's
/// bond of 2, which a resumer that has not run reports as 1.
#[test]
fn a_run_does_its_work_when_called() {
    let mut resumer = MpsResumer::new(ExactReference::new(), 7);
    drop(resumer.run(&QirMachine::new(&bell()), &ids(&[0])));
    assert_eq!(resumer.last_run().largest_bond_dimension, 2);
}
