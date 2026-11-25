// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The grouped run's rules: shots share one state until their draws
//! differ, a shot with no quantum call prepares none, a shot draws at a
//! measurement or reset whose value is certain, and a run holds at most
//! ⌊log₂ N⌋ + 1 states at once; a failed run names its first failing shot
//! in the order of its shots, whichever group goes on, no group runs on
//! when its shots all come after that shot, a group that ends reads its
//! weight even after the run failed, the report keeps the least weight
//! whichever group ends last, an engine's failure in any call fails the
//! shot with the engine's error, and no state an engine call failed on is
//! used again; a batch is lowered whole before its first step runs, and
//! only a batch a shot reaches; and a stop flag ends a run before the next
//! batch, naming the first shot that had not ended. How a group's shots
//! draw is in the tests beside `grouped_run/shot_group.rs`, how a group's
//! state takes each step in those beside `grouped_run/site_steps.rs`, and
//! which failure a run keeps in those beside `grouped_run/first_failure.rs`.
//!
//! The rules on dropped and ending groups and on stops are checked on
//! groups and failures set by hand on a run ([`in_a_run`]), so they hold
//! whatever the shots draw.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use tensornet::reference::ExactReference;
use tensornet::{MpsEngine, MpsShape};

use crate::bytecode::Instruction;
use crate::bytecode::encoding::{OPID_CX, OPID_H, OPID_MZ, OPID_RY, OPID_X};
use crate::mps_simulator::grouped_run::GroupEnd;
use crate::mps_simulator::grouped_run::test_utils::{
    Failure, failed_at, gate_on, in_a_run, stopped, the_first_group,
};
use crate::mps_simulator::lowering::SiteStep;
use crate::mps_simulator::shot_rng::shot_rng;
use crate::mps_simulator::test_utils::programs::{
    branch, coin_copied_by_a_branch, coins, gate, gate1, gate2, jump, measure, mz, program,
    program_with, read_result, readout_noise, reset, ret, set,
};
use crate::mps_simulator::test_utils::reference_spy::{
    BASIS_STATE_WEIGHT_STEP, Call, ReferenceSpy, Weight,
};
use crate::mps_simulator::test_utils::{LONE_SHOT_CALLS, bell, ids, results, run};
use crate::mps_simulator::{DEFAULT_MAX_BOND_DIMENSION, MpsResumer, MpsResumerError};
use crate::qir::{QirGate, QirProgram, QirResumeError, QirShotId};

/// Shots share one state until their draws differ, and fork it once where
/// they do; and a measurement steers its own shot's later gates, so each
/// shot records one of the program's readouts. Each row is a program, run
/// on 16 shots, its readouts, and the states it prepares and forks: a
/// program of no randomness prepares one state for all its shots and forks
/// none; a coin copied by a branch splits the shots once, the copy's
/// branch running on the fork, and each shot reads its coin twice.
#[test]
fn shots_share_one_state_until_their_draws_differ() {
    let no_randomness = program(
        2,
        &[&[gate1(OPID_X, 0), gate2(OPID_CX, 0, 1), mz(0), mz(1), ret()]],
    );
    let rows = [
        (
            "no randomness",
            no_randomness,
            vec![results(&[1, 1])],
            (1, 0),
        ),
        (
            "a coin copied by a branch",
            coin_copied_by_a_branch(),
            vec![results(&[0, 0]), results(&[1, 1])],
            (1, 1),
        ),
    ];
    for (scene, program, readouts, states) in rows {
        let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
        let records = run(&mut resumer, &program, &QirShotId::first(16)).expect("the program runs");
        assert!(
            readouts.iter().all(|readout| records.contains(readout)),
            "{scene}: 16 shots read {readouts:?}"
        );
        assert!(
            records.iter().all(|record| readouts.contains(record)),
            "{scene}: each shot reads one of {readouts:?}: {records:?}"
        );
        let engine = &resumer.engine;
        assert_eq!((engine.prepared().len(), engine.forks()), states, "{scene}");
    }
}

/// A state is prepared only when a shot first needs it: shots that end
/// with no quantum call record their results, all `Zero`, and prepare no
/// state. Each row is such a program: one of no qubits; and one of two
/// qubits whose quantum part runs only when a classical flag is set, here
/// clear.
#[test]
fn a_shot_with_no_quantum_call_prepares_no_state() {
    let (quantum, done) = (1, 2);
    let flag_clear = program(
        2,
        &[
            &[set(0, 0), branch(0, quantum, done)],
            &[gate1(OPID_H, 0), mz(0), jump(done)],
            &[ret()],
        ],
    );
    let rows = [
        ("no qubits", program(0, &[&[ret()]]), results(&[0])),
        ("a quantum part not taken", flag_clear, results(&[0, 0])),
    ];
    for (scene, program, readout) in rows {
        let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
        let records = run(&mut resumer, &program, &ids(&[0, 1])).expect("the program runs");
        assert_eq!(records, vec![readout; 2], "{scene}");
        assert_eq!(resumer.engine.prepared(), Vec::<MpsShape>::new(), "{scene}");
    }
}

/// A shot draws a uniform at a measurement or a reset whose value is
/// certain, as at any other, so its later draws are those it would make
/// were the value uncertain. How a group draws is in the tests beside
/// `grouped_run/shot_group.rs`; here, a whole run draws at each certain
/// call: a qubit at |0⟩, or flipped to |1⟩, measured
/// (or reset), then turned to a fair coin and measured into a second
/// result, which reads the second uniform of the shot's RNG against ½.
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
            let program = program_with(1, 2, &[&block]);
            let records = run(
                &mut MpsResumer::new(ExactReference::new(), 5),
                &program,
                &ids(&shots),
            )
            .expect("the program runs");
            let first = u8::from(records_it && certain_one);
            for (&shot, records) in shots.iter().zip(records) {
                let mut rng = shot_rng(5, QirShotId(shot));
                let _certain = rng.next_uniform();
                let coin = u8::from(rng.next_uniform() < 0.5);
                assert_eq!(
                    records,
                    results(&[first, coin]),
                    "{scene} of {certain_one}, shot {shot}"
                );
            }
        }
    }
}

/// A run of N shots holds at most ⌊log₂ N⌋ + 1 states at once, however its
/// draws split them. Each row is a program run on 16 shots, with the seeds
/// it runs under. Twelve biased coins (P(One) 0.15) each split off a few
/// shots: a run that went on with the larger group would hold them all at
/// once. Four fair coins split the shots about in half: a run that took
/// the waiting groups in the order they split off would hold the halves
/// side by side.
#[test]
fn a_run_holds_at_most_log2_of_its_shots_plus_one_states() {
    let angle = 2.0 * 0.15_f64.sqrt().asin();
    let mut block: Vec<_> = (0..12)
        .flat_map(|qubit| [gate(OPID_RY, qubit, 0, angle), mz(qubit)])
        .collect();
    block.push(ret());
    let rows = [
        ("twelve biased coins", program(12, &[&block]), 7..8),
        ("four fair coins", coins(4), 0..20),
    ];
    for (scene, program, seeds) in rows {
        for seed in seeds {
            let mut resumer = MpsResumer::new(ReferenceSpy::new(), seed);
            run(&mut resumer, &program, &QirShotId::first(16)).expect("the program runs");
            assert!(
                resumer.engine.forks() > 5,
                "{scene}, seed {seed}: the coins split the shots often"
            );
            assert!(
                resumer.engine.most_live() <= 5,
                "{scene}, seed {seed}: at most ⌊log₂ 16⌋ + 1 = 5 states at once, not {}",
                resumer.engine.most_live()
            );
        }
    }
}

/// A failed run names its first failing shot in the order of its shots,
/// whichever group of a split goes on. Each row is a scene, the shot it
/// names, and how that shot fails.
#[test]
fn a_failed_run_names_its_first_failing_shot_in_shot_order() {
    let rows = [
        a_coin_that_traps_on_either_value(),
        several_failing_groups(),
        the_engines_failure_met_by_one_group(),
    ];
    for (scene_run, shot, failure) in rows.into_iter().flatten() {
        assert_eq!(scene_run.run(), (shot, failure), "{}", scene_run.scene);
    }
}

/// A group that waits is dropped unrun once a shot before all its shots
/// failed: it could not change the shot the run names. A group of shots 1
/// and 2, split off a run's first group by hand, waits to run a program
/// that applies an X. Each row is the shot whose failure the run kept (a
/// stop, here), and the gates the run applies: shot 0's failure drops the
/// group unrun; the failure of shot 1, its own first shot, does not.
#[test]
fn a_waiting_group_whose_shots_all_come_after_the_failure_is_dropped_unrun() {
    let x_then_end = program(1, &[&[gate1(OPID_X, 0), ret()]]);
    let shots = QirShotId::first(3);
    for (failing, applies) in [(0, 0), (1, 1)] {
        let engine = ReferenceSpy::new();
        let result = in_a_run(&engine, &x_then_end, &shots, None, |mut run| {
            let mut group = the_first_group(&mut run);
            group.keep_shots_that_drew(&[true, false, false], false);
            run.waiting.push(group);
            run.failure.keep(failing, stopped());
            run.run()
        });
        let scene = format!("shot {failing} failed");
        let error = result.expect_err(&scene);
        assert_eq!(
            failed_at(&error, &scene),
            (shots[failing], Failure::Stopped),
            "{scene}"
        );
        assert_eq!(engine.applies(), applies, "{scene}");
    }
}

/// A group runs no step once its shots all come after the run's failure, as
/// after a split whose fork failed to settle for the shots before them: it
/// is dropped, the rest of its batch unrun. A group of shots 1 and 2, its
/// batch two X gates, on a program that then ends. Each row is the shot
/// whose failure the run kept, whether the group is dropped, and the gates
/// applied: shot 0's failure drops it before either X; shot 1's, its own
/// first shot's, lets it run both and end.
#[test]
fn a_group_runs_no_step_once_its_shots_all_come_after_the_failure() {
    let end = program(1, &[&[ret()]]);
    let x_gate = SiteStep::Apply(gate_on(QirGate::X, 0));
    for (failing, dropped, applies) in [(0, true, 0), (1, false, 2)] {
        let engine = ReferenceSpy::new();
        let was_dropped = in_a_run(&engine, &end, &QirShotId::first(3), None, |mut run| {
            let mut group = the_first_group(&mut run);
            group.keep_shots_that_drew(&[true, false, false], false);
            group.start_batch(vec![x_gate.clone(), x_gate.clone()]);
            run.failure.keep(failing, stopped());
            let end_of_group = run.run_group(&mut group).expect("no step fails");
            matches!(end_of_group, GroupEnd::Dropped)
        });
        let scene = format!("shot {failing} failed");
        assert_eq!(was_dropped, dropped, "{scene}");
        assert_eq!(engine.applies(), applies, "{scene}");
    }
}

/// A group whose shots end reads the weight its state kept into the run's
/// report, even in a run that failed: the report keeps the least weight
/// read from the shots that ended. A group of shot 0, split off a run's
/// first group by hand, on an engine whose every state keeps half its
/// weight, runs an X to its end after shot 1 failed.
#[test]
fn a_group_that_ends_reads_its_weight_even_after_the_run_failed() {
    let engine = ReferenceSpy::retaining(Weight::Fixed(0.5));
    let end = program(1, &[&[ret()]]);
    let least_weight = in_a_run(&engine, &end, &QirShotId::first(2), None, |mut run| {
        let mut group = the_first_group(&mut run);
        group.keep_shots_that_drew(&[false, true], false);
        group.start_batch(vec![SiteStep::Apply(gate_on(QirGate::X, 0))]);
        run.failure.keep(1, stopped());
        let end_of_group = run.run_group(&mut group).expect("no step fails");
        assert!(matches!(end_of_group, GroupEnd::Ended(_)), "shot 0 ends");
        run.report.least_retained_weight
    });
    assert!((least_weight - 0.5).abs() < 1e-12, "{least_weight}");
}

/// The report keeps the least weight read from the groups that ended,
/// whichever ends last. Two groups, split by hand from a run's first group
/// of two shots, on an engine whose weight falls with the basis state read
/// ([`BASIS_STATE_WEIGHT_STEP`]: |0⟩ keeps 1 − 1 step, |1⟩ 1 − 2 steps):
/// one stays at |1⟩, the other takes an X back to |0⟩, and both end. Each
/// row is which group ends first; the least is |1⟩'s either way.
#[test]
fn the_report_keeps_the_least_weight_whichever_group_ends_last() {
    let end = program(1, &[&[ret()]]);
    for (scene, at_one_first) in [("|1⟩ ends first", true), ("|0⟩ ends first", false)] {
        let engine = ReferenceSpy::retaining(Weight::ByBasisState);
        let least_weight = in_a_run(&engine, &end, &QirShotId::first(2), None, |mut run| {
            let mut at_zero = the_first_group(&mut run);
            run.apply(&mut at_zero, &gate_on(QirGate::X, 0))
                .expect("an X");
            let state = at_zero.state.as_ref().expect("prepared by the X");
            let fork = engine.fork(state).expect("a fork");
            let shots = at_zero.keep_shots_that_drew(&[false, true], false);
            let mut at_one = at_zero.split_off(shots, fork, None);
            at_zero.start_batch(vec![SiteStep::Apply(gate_on(QirGate::X, 0))]);
            let order = if at_one_first {
                [&mut at_one, &mut at_zero]
            } else {
                [&mut at_zero, &mut at_one]
            };
            for group in order {
                let end_of_group = run.run_group(group).expect(scene);
                assert!(matches!(end_of_group, GroupEnd::Ended(_)), "{scene}");
            }
            run.report.least_retained_weight
        });
        let at_one_weight = 1.0 - 2.0 * BASIS_STATE_WEIGHT_STEP;
        assert!(
            (least_weight - at_one_weight).abs() < 1e-12,
            "{scene}: {least_weight}"
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
        let scene = format!("{call:?}");
        assert_eq!(
            failed_at(&error, &scene),
            (QirShotId(9), Failure::Engine),
            "{scene}"
        );
    }
}

/// A state an engine call failed on is not used again, as the engine
/// leaves it unspecified: the call's group fails, and no later call is
/// given its state. Each row is one of the calls given a state that a Bell
/// pair's shot makes alone, failing. A failed fork has its own test, in
/// `mps_simulator/grouped_run/site_steps/tests.rs`.
#[test]
fn a_state_an_engine_call_failed_on_is_not_used_again() {
    for call in LONE_SHOT_CALLS {
        if call == Call::Prepare {
            continue; // A failed preparation gives no state.
        }
        let mut resumer = MpsResumer::new(ReferenceSpy::failing_first(call), 5);
        let scene = format!("a failed {call:?}");
        run(&mut resumer, &bell(), &ids(&[9])).expect_err(&scene);
        assert_eq!(resumer.engine.uses_after_failure(), 0, "{scene}");
    }
}

/// A refused call fails its batch before any of the batch's gates is
/// applied: the whole batch is lowered before its first step runs, and the
/// run fails with the refusal, naming its first shot. A batch of an `h`,
/// then readout noise, run on shots 3 and 4. Which calls lowering refuses
/// is in `mps_simulator/lowering/tests.rs`.
#[test]
fn a_refused_call_fails_its_batch_before_any_of_its_gates_is_applied() {
    let h_then_readout_noise = program(1, &[&[gate1(OPID_H, 0), readout_noise(0.1, 0), ret()]]);
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7);
    let error = run(&mut resumer, &h_then_readout_noise, &ids(&[3, 4]))
        .expect_err("the method refuses readout noise");
    assert!(
        matches!(
            error,
            QirResumeError::Resumer {
                shot: QirShotId(3),
                error: MpsResumerError::Unsupported { .. },
            }
        ),
        "{error:?}"
    );
    assert_eq!(resumer.engine.applies(), 0);
}

/// A noise call on a path no shot takes fails nothing: the run refuses a
/// call when a shot's batch holds it, not because the program holds it.
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

/// A run whose stop flag is already set fails before any gate, naming its
/// first shot, even when its program would end at its first resume; the
/// resumer leaves the flag set, so each run fails so until the caller
/// clears it. Each row is a run with the flag set: four coins; a program
/// that only returns; and four coins again.
#[test]
fn a_set_stop_flag_fails_the_run_before_any_gate() {
    let stop_flag = Arc::new(AtomicBool::new(true));
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7).with_stop_flag(stop_flag);
    let rows = [
        ("four coins", coins(4), ids(&[3, 1, 2])),
        (
            "a program that only returns",
            program(1, &[&[ret()]]),
            ids(&[4, 1]),
        ),
        ("four coins again", coins(4), ids(&[3, 1, 2])),
    ];
    for (scene, program, shots) in rows {
        let error = run(&mut resumer, &program, &shots).expect_err(scene);
        assert_eq!(
            failed_at(&error, scene),
            (shots[0], Failure::Stopped),
            "{scene}"
        );
    }
    assert_eq!(resumer.engine.applies(), 0);
}

/// A flag set during a batch lets the group under way run the rest of it,
/// and stops each group split off by a draw before it runs on: set at the
/// first gate of eight coins on 16 shots, whose draws split the shots into
/// many groups, then an X in a second batch that every shot reaches, the
/// run applies the eight gates of the group under way, none of the groups
/// split off, and names shot 0, as no shot ended. The run frees every
/// state it held.
#[test]
fn a_stop_flag_set_during_a_run_stops_it_once_the_batch_under_way_ends() {
    let mut block: Vec<_> = (0..8)
        .flat_map(|qubit| [gate1(OPID_H, qubit), mz(qubit)])
        .collect();
    block.extend([read_result(0, 0), gate1(OPID_X, 0), ret()]);
    let coins_then_a_batch = program(8, &[&block]);
    let mut resumer = resumer_stopping_at_apply(1);
    let scene = "a flag set at the first gate";
    let error = run(&mut resumer, &coins_then_a_batch, &QirShotId::first(16)).expect_err(scene);
    assert_eq!(failed_at(&error, scene), (QirShotId(0), Failure::Stopped));
    assert_eq!(
        resumer.engine.applies(),
        8,
        "the gates of the group under way, none of the groups split off"
    );
    assert_eq!(resumer.engine.live(), 0, "no state is kept");
}

/// A stopped run names the first shot that had not ended, not the run's
/// first shot: a run's first group keeps shot 0 and splits off shots 1 and
/// 2 by hand, on a state of their own, to run after it. The flag, set as
/// shot 0's group applies the program's X, stops the group of shots 1 and
/// 2 before it runs, once shot 0 ended.
#[test]
fn a_stopped_run_names_the_first_shot_that_had_not_ended() {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let engine = ReferenceSpy::stopping_at_apply(1, Arc::clone(&stop_flag));
    let x_then_end = program(1, &[&[gate1(OPID_X, 0), ret()]]);
    let shots = QirShotId::first(3);
    let result = in_a_run(&engine, &x_then_end, &shots, Some(&stop_flag), |mut run| {
        let mut first = the_first_group(&mut run);
        let later = first.keep_shots_that_drew(&[false, true, true], false);
        let capacity = MpsShape::qubit_chain(1, DEFAULT_MAX_BOND_DIMENSION.get())
            .expect("one site is a chain");
        let state = engine.prepare(&capacity).expect("the spy prepares a state");
        run.waiting.push(first.split_off(later, state, None));
        run.waiting.push(first);
        run.run()
    });
    let scene = "a flag set at shot 0's X";
    assert_eq!(
        failed_at(&result.expect_err(scene), scene),
        (QirShotId(1), Failure::Stopped)
    );
    assert_eq!(engine.applies(), 1, "shot 0's X alone");
}

/// The flag is read before each batch, not once per group, whatever that
/// batch would do: three shots run a first batch (a gate, then `mz`); set
/// at that gate, the flag stops the run before a second batch, naming the
/// first of the run's shots, none of which had ended, after only that gate;
/// with no second batch, the shots end and the run returns their records.
/// Each row is what follows the first batch: a second batch that runs
/// gates chosen by a coin, traps, or is refused (a noise call); or no
/// second batch.
#[test]
fn a_stop_flag_is_read_before_each_batch() {
    let shots = ids(&[1, 2, 0]);
    for (second_batch, program, ended) in after_a_first_batch() {
        let mut resumer = resumer_stopping_at_apply(1);
        let result = run(&mut resumer, &program, &shots);
        match ended {
            Some(value) => assert_eq!(
                result.expect("the shots ended"),
                vec![results(&[value]); 3],
                "{second_batch}"
            ),
            None => assert_eq!(
                failed_at(&result.expect_err(second_batch), second_batch),
                (QirShotId(1), Failure::Stopped),
                "{second_batch}"
            ),
        }
        assert_eq!(
            resumer.engine.applies(),
            1,
            "{second_batch}: only the first batch's gate applied"
        );
    }
}

/// What follows the first batch, a gate then `mz`, in each scene of
/// `a_stop_flag_is_read_before_each_batch`: the scene, its program, and the
/// value each shot reads when the shots end (`None` when the flag stops
/// the run).
fn after_a_first_batch() -> [(&'static str, QirProgram, Option<u8>); 4] {
    // A branch on a register that holds neither 0 nor 1 traps.
    let a_trap = [set(0, 2), branch(0, 1, 1)];
    [
        (
            "gates chosen by a coin",
            program(
                2,
                &[
                    &a_first_batch_then(OPID_H, &[branch(0, 1, 2)]),
                    &[gate1(OPID_X, 1), gate1(OPID_H, 1), ret()],
                    &[gate1(OPID_H, 1), gate1(OPID_X, 1), ret()],
                ],
            ),
            None,
        ),
        (
            "a trap",
            program(1, &[&a_first_batch_then(OPID_X, &a_trap), &[ret()]]),
            None,
        ),
        (
            "a noise call",
            program(
                1,
                &[&a_first_batch_then(OPID_X, &[readout_noise(0.1, 0), ret()])],
            ),
            None,
        ),
        (
            "no batch: the shots end",
            program(1, &[&[gate1(OPID_X, 0), mz(0), ret()]]),
            Some(1),
        ),
    ]
}

/// A first batch, the gate `first` on qubit 0 then its `mz` read into
/// register 0, followed by `then`.
fn a_first_batch_then(first: u64, then: &[Instruction<u64>]) -> Vec<Instruction<u64>> {
    let mut block = vec![gate1(first, 0), mz(0), read_result(0, 0)];
    block.extend_from_slice(then);
    block
}

/// A resumer with seed 7 whose engine sets the resumer's stop flag when it
/// applies its gate number `apply` (from 1), as a user's interrupt would.
fn resumer_stopping_at_apply(apply: usize) -> MpsResumer<ReferenceSpy> {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let engine = ReferenceSpy::stopping_at_apply(apply, Arc::clone(&stop_flag));
    MpsResumer::new(engine, 7).with_stop_flag(stop_flag)
}

/// A failing run's scene, the shot the run names, and how that shot fails.
type NamedFailure = (FailingRun, QirShotId, Failure);

/// The first `count` shots, forward and reversed, each with its order's
/// name.
fn in_both_orders(count: u32) -> [(&'static str, Vec<QirShotId>); 2] {
    let forward = QirShotId::first(count);
    let reversed = forward.iter().rev().copied().collect();
    [("forward", forward), ("reversed", reversed)]
}

/// On a coin whose shots that read 1 trap, and on one whose shots that
/// read 0 trap, at seed 5, shots 0 to 7 forward and reversed: the run
/// names the first, in the run's order, of the shots that read the value
/// that traps. At seed 5, shots 2, 4 and 7 read 1. They are the fewer, so
/// they go on from the split and the others wait: the failing group goes
/// on on the first coin and waits on the second. So the run names shot 2
/// forward and 7 reversed on the first, and 0 forward and 6 reversed on
/// the second.
fn a_coin_that_traps_on_either_value() -> Vec<NamedFailure> {
    let reading_one = failing_alone(
        || MpsResumer::new(ReferenceSpy::new(), 5),
        &coins_then_trap_unless_all(1, 0),
        &QirShotId::first(8),
    );
    assert_eq!(
        reading_one,
        ids(&[2, 4, 7]),
        "at seed 5, shots 2, 4 and 7 of 0 to 7 read 1"
    );
    let mut rows = Vec::new();
    // Each row: the value that passes, and the shot the run names forward
    // and reversed.
    for (pass, named) in [(0, [2, 7]), (1, [0, 6])] {
        for ((order, shots), shot) in in_both_orders(8).into_iter().zip(named) {
            let scene = format!("a coin that traps unless {pass}, {order}");
            let program = coins_then_trap_unless_all(1, pass);
            let scene_run = FailingRun::new(scene, 5, program, shots);
            rows.push((scene_run, QirShotId(shot), Failure::Trap));
        }
    }
    rows
}

/// Among several failing groups, on two coins whose every group but the
/// one that read 0, 0 traps, 16 shots forward and reversed: the run names
/// the first of the shots that fail alone.
fn several_failing_groups() -> Vec<NamedFailure> {
    in_both_orders(16)
        .into_iter()
        .map(|(order, shots)| {
            let program = coins_then_trap_unless_all(2, 0);
            let make = || MpsResumer::new(ReferenceSpy::new(), 7);
            let failing = failing_alone(make, &program, &shots);
            assert!(failing.len() > 2, "most shots trap");
            let scene = format!("several failing groups, {order}");
            let scene_run = FailingRun::new(scene, 7, program, shots);
            (scene_run, failing[0], Failure::Trap)
        })
        .collect()
}

/// The engine's failure, met by the shots of one group: a coin on an
/// engine whose projections on One fail, 16 shots forward and reversed:
/// the run names the first of the shots that read 1.
fn the_engines_failure_met_by_one_group() -> Vec<NamedFailure> {
    in_both_orders(16)
        .into_iter()
        .map(|(order, shots)| {
            let make = || MpsResumer::new(ReferenceSpy::failing_projections_on_one(), 5);
            let failing = failing_alone(make, &coins(1), &shots);
            assert!(!failing.is_empty(), "some of 16 coins read 1");
            let scene = format!("the engine's failure, met by one group, {order}");
            let scene_run = FailingRun {
                engine: ReferenceSpy::failing_projections_on_one,
                ..FailingRun::new(scene, 5, coins(1), shots)
            };
            (scene_run, failing[0], Failure::Engine)
        })
        .collect()
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
    program_with(qubits, qubits + 1, &blocks)
}

/// A run that fails: its scene, for the failure message, and its engine,
/// seed, program and shots.
struct FailingRun {
    /// What the run is, for the failure message.
    scene: String,
    /// Makes the run's engine.
    engine: fn() -> ReferenceSpy,
    /// The run's seed.
    seed: u64,
    /// The run's program.
    program: QirProgram,
    /// The run's shots, in the run's order.
    shots: Vec<QirShotId>,
}

impl FailingRun {
    /// The scene on the spy's engine that fails no call, which a scene
    /// whose engine fails replaces by struct update.
    fn new(
        scene: impl Into<String>,
        seed: u64,
        program: QirProgram,
        shots: Vec<QirShotId>,
    ) -> Self {
        Self {
            scene: scene.into(),
            engine: ReferenceSpy::new,
            seed,
            program,
            shots,
        }
    }

    /// Runs the scene on a new resumer; returns the shot the run names,
    /// and how it failed.
    fn run(&self) -> (QirShotId, Failure) {
        let mut resumer = MpsResumer::new((self.engine)(), self.seed);
        let error = run(&mut resumer, &self.program, &self.shots).expect_err(&self.scene);
        failed_at(&error, &self.scene)
    }
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
