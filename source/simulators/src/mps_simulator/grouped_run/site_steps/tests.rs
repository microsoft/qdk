// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The site steps' rules, each step taken by hand on a run's first group,
//! its draws written out: a group's draws settle it on the value going on,
//! the shots that drew the other value waiting on one fork; a failed fork
//! fails the whole group, a failure settling the fork only the shots that
//! wait; a reset returns its site to |0⟩; and a group prepares its state
//! at its first step, reads the bond once per sequence of gates, and
//! P(One) once per draw for all its shots.

use tensornet::MpsEngine;

use crate::mps_simulator::MpsResumerError;
use crate::mps_simulator::grouped_run::GroupedRun;
use crate::mps_simulator::grouped_run::first_failure::{FirstFailure, GroupFailure};
use crate::mps_simulator::grouped_run::site_steps::DrawStep;
use crate::mps_simulator::grouped_run::test_utils::{
    Failure, SpyGroup, failed_at, gate_on, in_a_run, the_first_group,
};
use crate::mps_simulator::test_utils::programs::{program, ret};
use crate::mps_simulator::test_utils::reference_spy::{ReferenceSpy, ReferenceSpyError};
use crate::qir::{QirGate, QirOutcome, QirProgram, QirReadout, QirShotId};

/// A group's draws settle it on the value going on, the fewer's; the shots
/// that drew the other value wait, on one fork settled on theirs. Each
/// group answers the measurement's result with its value. Each row is the
/// draws of three shots at a fair coin's measurement, the group going on
/// and the group that waits, each settled (its shots' places, its
/// outcomes, and its site's value), and the forks: draws that differ split
/// the group; draws of one value fork nothing.
#[test]
fn draws_settle_the_group_on_the_value_going_on_and_fork_the_others() {
    let (zero, one) = (QirReadout::Zero, QirReadout::One);
    let rows = [
        (
            "draws that differ",
            [false, true, true],
            (vec![0], vec![answer(zero)], Some(zero)),
            Some((vec![1, 2], vec![answer(one)], Some(one))),
            1,
        ),
        (
            "draws of one value",
            [true, true, true],
            (vec![0, 1, 2], vec![answer(one)], Some(one)),
            None,
            0,
        ),
    ];
    for (scene, draws, going_on, waiting, forks) in rows {
        let engine = ReferenceSpy::new();
        let groups = on_a_fair_coin(&engine, |run, group| {
            let measurement = DrawStep::measurement(0, 0, false);
            run.take_draws(group, &draws, measurement).expect(scene);
            let split_off = run.waiting.pop();
            assert!(run.waiting.is_empty(), "{scene}: one fork at most");
            let waiting = split_off.map(|split_off| settled(&engine, &split_off));
            (settled(&engine, group), waiting)
        });
        assert_eq!(groups, (going_on, waiting), "{scene}");
        assert_eq!(engine.forks(), forks, "{scene}");
    }
}

/// A failed fork fails the whole group, before its shots split: the engine
/// leaves the state it failed on unspecified, so no group goes on with it.
/// Three shots draw Zero, One, One at a fair coin's measurement, on an
/// engine whose forks fail.
#[test]
fn a_failed_fork_fails_the_whole_group_before_it_splits() {
    let engine = ReferenceSpy::failing_forks();
    on_a_fair_coin(&engine, |run, group| {
        let measurement = DrawStep::measurement(0, 0, false);
        let result = run.take_draws(group, &[false, true, true], measurement);
        assert!(
            matches!(
                result,
                Err(GroupFailure::Resumer(MpsResumerError::Engine(
                    ReferenceSpyError::Device
                )))
            ),
            "the fork's failure fails the group"
        );
        assert_eq!(places(group), vec![0, 1, 2], "the group keeps its shots");
        assert!(run.waiting.is_empty(), "no shot waits");
    });
    assert_eq!(engine.uses_after_failure(), 0, "the group's state is left");
}

/// When settling the fork fails, only the shots that wait fail: the group's
/// own state was not touched, so the group goes on, settled on its value,
/// and the run keeps the failure of the first shot that waits. Three shots
/// draw Zero, One, One at a fair coin: shot 0 goes on, and shots 1 and 2
/// would wait on One. Each row is how settling fails, and the draw step:
/// the projection on One, at a measurement; or the X that returns a
/// reset's site to |0⟩.
#[test]
fn a_failure_settling_the_fork_fails_only_the_shots_that_wait() {
    let rows = [
        (
            "a projection on One",
            ReferenceSpy::failing_projections_on_one(),
            DrawStep::measurement(0, 0, false),
        ),
        (
            "a reset's X",
            ReferenceSpy::failing_x_gates(),
            DrawStep::reset_only(0),
        ),
    ];
    for (scene, engine, step) in rows {
        let (group, failure) = on_a_fair_coin(&engine, |run, group| {
            run.take_draws(group, &[false, true, true], step)
                .expect(scene);
            assert!(run.waiting.is_empty(), "{scene}: the failed fork waits not");
            let failure = kept_failure(run, &QirShotId::first(3));
            ((places(group), site_value(&engine, group)), failure)
        });
        assert_eq!(group, (vec![0], Some(QirReadout::Zero)), "{scene}");
        assert_eq!(failure, Some((QirShotId(1), Failure::Engine)), "{scene}");
    }
}

/// A reset returns its site to |0⟩ once the value is kept, whichever value
/// it kept; a reset alone answers no result. Each row is a draw step, the
/// value three shots all draw at it on a fair coin, and the outcomes the
/// group answers: a measurement and reset of One answers One; a reset
/// alone of One, or of Zero, none.
#[test]
fn a_reset_returns_its_site_to_zero_after_keeping_the_value() {
    let rows = [
        (
            "a measurement and reset of One",
            DrawStep::measurement(0, 0, true),
            true,
            vec![answer(QirReadout::One)],
        ),
        (
            "a reset alone of One",
            DrawStep::reset_only(0),
            true,
            vec![],
        ),
        (
            "a reset alone of Zero",
            DrawStep::reset_only(0),
            false,
            vec![],
        ),
    ];
    for (scene, step, one, outcomes) in rows {
        let engine = ReferenceSpy::new();
        let group = on_a_fair_coin(&engine, |run, group| {
            run.take_draws(group, &[one; 3], step).expect(scene);
            settled(&engine, group)
        });
        let expected = (vec![0, 1, 2], outcomes, Some(QirReadout::Zero));
        assert_eq!(group, expected, "{scene}");
    }
}

/// A group prepares its state at its first step, once, whichever kind that
/// step is. Each row is a first step, followed by a gate: a gate; and a
/// draw.
#[test]
fn a_group_prepares_its_state_at_its_first_step_once() {
    let x_gate = gate_on(QirGate::X, 0);
    let rows: [(&str, FirstStep); 2] = [
        ("a gate", |run, group| {
            run.apply(group, &gate_on(QirGate::H, 0))
        }),
        ("a draw", |run, group| {
            run.draw(group, DrawStep::reset_only(0))
        }),
    ];
    for (scene, first_step) in rows {
        let engine = ReferenceSpy::new();
        in_a_run(
            &engine,
            &one_qubit(),
            &QirShotId::first(3),
            None,
            |mut run| {
                let mut group = the_first_group(&mut run);
                assert!(engine.prepared().is_empty(), "{scene}: no step yet");
                first_step(&mut run, &mut group).expect(scene);
                run.apply(&mut group, &x_gate).expect(scene);
            },
        );
        assert_eq!(engine.prepared().len(), 1, "{scene}");
    }
}

/// A group's first step, given the run and the group.
type FirstStep = fn(
    &mut GroupedRun<'_, ReferenceSpy>,
    &mut SpyGroup,
) -> Result<(), GroupFailure<ReferenceSpyError>>;

/// The largest bond dimension is read once per sequence of gates, where it
/// ends: before a draw, which may shrink it, and not again until a gate is
/// applied; a split reads it for neither group. Steps on a run's first
/// group of three shots, each followed by the shapes read so far.
#[test]
fn the_bond_is_read_once_per_gate_sequence() {
    let engine = ReferenceSpy::new();
    let measurement = DrawStep::measurement(0, 0, false);
    in_a_run(
        &engine,
        &one_qubit(),
        &QirShotId::first(3),
        None,
        |mut run| {
            let mut group = the_first_group(&mut run);
            run.apply(&mut group, &gate_on(QirGate::X, 0))
                .expect("an X");
            assert_eq!(engine.shapes(), 0, "a gate reads nothing");
            run.draw(&mut group, measurement).expect("a certain One");
            assert_eq!(engine.shapes(), 1, "a draw reads it first");
            run.read_bond(&mut group).expect("no read");
            assert_eq!(engine.shapes(), 1, "no gate since");
            run.apply(&mut group, &gate_on(QirGate::H, 0))
                .expect("an H");
            run.read_bond(&mut group).expect("a read");
            assert_eq!(engine.shapes(), 2, "a gate since");
            run.take_draws(&mut group, &[false, true, true], measurement)
                .expect("a split");
            let mut split_off = run.waiting.pop().expect("the shots that wait");
            run.read_bond(&mut group).expect("no read");
            run.read_bond(&mut split_off).expect("no read");
            assert_eq!(engine.shapes(), 2, "a split reads it for neither group");
        },
    );
}

/// A draw reads P(One) once for all the group's shots, each drawing
/// against it: three shots at a fair coin's measurement.
#[test]
fn a_draw_reads_the_probability_of_one_once_for_the_group() {
    let engine = ReferenceSpy::new();
    on_a_fair_coin(&engine, |run, group| {
        let measurement = DrawStep::measurement(0, 0, false);
        run.draw(group, measurement).expect("a coin");
    });
    assert_eq!(engine.probability_reads(), 1);
}

/// Gives what `scene` gives, run on the first group of a run of three
/// shots of a program of one qubit, its site 0 turned to a fair coin by
/// an H.
fn on_a_fair_coin<R>(
    engine: &ReferenceSpy,
    scene: impl FnOnce(&mut GroupedRun<'_, ReferenceSpy>, &mut SpyGroup) -> R,
) -> R {
    in_a_run(
        engine,
        &one_qubit(),
        &QirShotId::first(3),
        None,
        |mut run| {
            let mut group = the_first_group(&mut run);
            run.apply(&mut group, &gate_on(QirGate::H, 0))
                .expect("an H");
            scene(&mut run, &mut group)
        },
    )
}

/// A program of one qubit, which a test's group runs no batch of: its steps
/// are taken by hand.
fn one_qubit() -> QirProgram {
    program(1, &[&[ret()]])
}

/// A group, settled: its shots' places, its outcomes so far, and its site
/// 0's value ([`site_value`]).
type Settled = (Vec<usize>, Vec<QirOutcome>, Option<QirReadout>);

/// `group`, settled.
fn settled(engine: &ReferenceSpy, group: &SpyGroup) -> Settled {
    (
        places(group),
        group.outcomes.clone(),
        site_value(engine, group),
    )
}

/// The places of `group`'s shots, in order.
fn places(group: &SpyGroup) -> Vec<usize> {
    group.shots.iter().map(|shot| shot.place).collect()
}

/// The value site 0 of `group`'s state holds, read from its probability
/// of One; `None` when it holds neither for sure.
fn site_value(engine: &ReferenceSpy, group: &SpyGroup) -> Option<QirReadout> {
    // The rounding of the reference's projections.
    const ROUNDING: f64 = 1e-12;
    let state = group.state.as_ref().expect("a settled group holds a state");
    let probability_of_one = engine
        .probability_of_one(state, 0)
        .expect("the spy reads a probability");
    if probability_of_one < ROUNDING {
        Some(QirReadout::Zero)
    } else if probability_of_one > 1.0 - ROUNDING {
        Some(QirReadout::One)
    } else {
        None
    }
}

/// The outcome that answers result 0 with `value`.
fn answer(value: QirReadout) -> QirOutcome {
    QirOutcome { result: 0, value }
}

/// The shot the failure `run` kept names among `shots`, the run's, and how
/// it failed; `None` when it kept none. The run keeps no failure after.
fn kept_failure(
    run: &mut GroupedRun<'_, ReferenceSpy>,
    shots: &[QirShotId],
) -> Option<(QirShotId, Failure)> {
    let failure = std::mem::replace(&mut run.failure, FirstFailure::none());
    let error = failure.into_error(shots)?;
    Some(failed_at(&error, "the kept failure"))
}
