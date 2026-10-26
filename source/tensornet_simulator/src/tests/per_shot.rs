// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The per-shot strategy, `PerShot(TensorNetShots)`, and its terminal
//! `TensorNetTerminal`: the prototype's MPS1–MPS4 on the region ISA. MPS1
//! compared the prototype's MPS terminal with its state vector; here the
//! per-shot terminal is checked against probabilities known by hand and
//! against the batch strategy's distribution.

use std::rc::Rc;

use executor_protocol::{
    Close as _, Describe as _, Executor as _, PerShot, Rejected, ShotSet, Value, open,
};
use qdk_simulators::{
    Simulator as _,
    bytecode::runtime::run_shot,
    cpu_full_state_simulator::FullStateSimulator,
    execution::{RegionInstruction, UnitaryOperation},
};
use tensornet::reference::ExactReference;

use super::{
    TestDoubleEngine,
    cases::{
        IPE_BITS, IPE_PHASE, allowed_distance, count, estimate, peak, per_shot_bell_pairs,
        per_shot_feedback_and_resets, per_shot_phase_estimation, total_variation,
    },
    crossing_pairs, cx, h, iterative_phase_estimation, measure, measure_reset,
    phase_estimation_distribution, region, run_batch, submit, values_of,
};
use crate::{MpsStack, TensorNetReport, TensorNetShots, TensorNetTerminal, mps_capacity};

fn per_shot(qubits: usize, max_bond: usize) -> PerShot<TensorNetShots<ExactReference>> {
    MpsStack::new(ExactReference::new())
        .with_max_bond_dimension(Some(u32::try_from(max_bond).expect("small")))
        .per_shot(qubits)
        .expect("a chain")
}

#[test]
fn mps1_bell_pairs_shot_by_shot_through_the_walk() {
    per_shot_bell_pairs(&MpsStack::new(ExactReference::new()), 200);
}

#[test]
fn the_per_shot_strategy_offers_feedback_resets_replayable_and_probabilities() {
    let strategy = per_shot(3, 8);
    let maker = strategy.0.capabilities();
    assert_eq!(maker.qubits, Some(3));
    assert!(maker.probabilities);
    let session = open(&strategy, 1, &ShotSet::range(1..=4).expect("shots")).expect("opens");
    let offered = session.capabilities();
    assert!(offered.feedback && offered.resets && offered.replayable && offered.probabilities);
}

#[test]
fn a_shot_draws_the_same_alone_or_with_others() {
    let strategy = per_shot(2, 8);
    let batch = [region([h(0), h(1)]), measure(0, 0), measure(1, 1)];
    let all = ShotSet::range(1..=10).expect("shots");
    let mut session = open(&strategy, 9, &all).expect("opens");
    let (together, end) = submit(&mut session, &all, &batch).expect("accepted");
    end.expect("ran");
    for j in [1, 4, 10] {
        let one = ShotSet::one(j);
        let mut alone = open(&strategy, 9, &one).expect("opens");
        let (events, end) = submit(&mut alone, &one, &batch).expect("accepted");
        end.expect("ran");
        let mine: Vec<_> = together.iter().filter(|e| e.shot == j).copied().collect();
        assert_eq!(events, mine, "shot {j}");
    }
    let values: Vec<_> = values_of(&together, 0)
        .into_iter()
        .map(|(_, v)| v)
        .collect();
    assert!(values.contains(&Value::One) && values.contains(&Value::Zero));
}

#[test]
fn feedback_and_resets_run_shot_by_shot() {
    per_shot_feedback_and_resets(
        &MpsStack::new(ExactReference::new()).with_max_bond_dimension(Some(4)),
        40,
    );
}

#[test]
fn mps2_the_report_keeps_the_largest_bond() {
    let ghz = [h(0), cx(0, 1), cx(1, 2)];
    for (operations, bond) in [
        (vec![h(0), h(3)], 1),
        (vec![h(0), cx(0, 1)], 2),
        (ghz.to_vec(), 2),
        (crossing_pairs().to_vec(), 4),
    ] {
        let strategy = per_shot(4, 64);
        let shots = ShotSet::range(1..=2).expect("shots");
        let mut session = open(&strategy, 1, &shots).expect("opens");
        let (_, end) =
            submit(&mut session, &shots, &[region(operations.clone())]).expect("accepted");
        end.expect("ran");
        let report = session.close();
        assert_eq!(report.shots.max_bond, bond, "{operations:?}");
        assert_eq!(report.shots.batches, 2);
    }
}

#[test]
fn mps3_a_shot_beyond_its_capacity_fails() {
    let capacity = mps_capacity(4, 2).expect("a chain");
    let mut terminal =
        TensorNetTerminal::new(Rc::new(ExactReference::new()), capacity, 1).expect("prepares");
    let (outcomes, end) = run_batch(&mut terminal, &[region(crossing_pairs())]);
    assert!(outcomes.is_empty());
    let reason = end.expect_err("fails");
    assert!(reason.contains("bond 4"), "{reason}");
    assert!(terminal.state().is_none());
    assert_eq!(
        terminal.call(&[measure(0, 0)]).err(),
        Some(Rejected::SessionFailed(reason))
    );
}

#[test]
fn mps4_an_engine_that_cannot_prepare_is_unavailable_at_the_first_call() {
    let engine = Rc::new(TestDoubleEngine::refusing());
    assert!(
        TensorNetTerminal::new(Rc::clone(&engine), mps_capacity(2, 2).expect("a chain"), 1)
            .is_err()
    );
    let strategy = PerShot(TensorNetShots::new(Rc::clone(&engine), 2, 2).expect("a chain"));
    let shots = ShotSet::one(1);
    let mut session = open(&strategy, 1, &shots).expect("opens: making a shot cannot fail");
    match submit(&mut session, &shots, &[measure(0, 0)]) {
        Err(Rejected::Unavailable(reason)) => assert!(reason.contains("refuses"), "{reason}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_shot_the_engine_could_not_prepare_runs_when_retried_later() {
    // `Unavailable` says "retry later": the shot prepares again on the retry.
    let engine = Rc::new(TestDoubleEngine::refusing_first(2));
    let strategy = PerShot(TensorNetShots::new(Rc::clone(&engine), 1, 2).expect("a chain"));
    let shots = ShotSet::one(1);
    let mut session = open(&strategy, 1, &shots).expect("opens");
    assert_eq!(engine.prepared(), 1);
    let batch = [measure(0, 0)];
    match submit(&mut session, &shots, &batch) {
        Err(Rejected::Unavailable(reason)) => assert!(reason.contains("refuses"), "{reason}"),
        other => panic!("{other:?}"),
    }
    let (events, end) = submit(&mut session, &shots, &batch).expect("the retry is accepted");
    end.expect("ran");
    assert_eq!(engine.prepared(), 3);
    assert_eq!(values_of(&events, 0), [(1, Value::Zero)]);
}

#[test]
fn the_terminal_rejects_a_batch_before_running_any_of_it() {
    let mut terminal = TensorNetTerminal::new(
        Rc::new(ExactReference::new()),
        mps_capacity(2, 2).expect("a chain"),
        3,
    )
    .expect("prepares");
    for (batch, at) in [
        (
            vec![region([UnitaryOperation::X { target: 0 }]), measure(2, 0)],
            1,
        ),
        (vec![region([cx(1, 1)])], 0),
        (vec![measure(0, usize::MAX)], 0),
        (
            vec![
                region([UnitaryOperation::X { target: 0 }]),
                RegionInstruction::Reset { qubit: 5 },
            ],
            1,
        ),
    ] {
        assert!(
            matches!(terminal.call(&batch), Err(Rejected::Instruction { at: found, .. }) if found == at),
            "{batch:?}"
        );
    }
    // Nothing ran: |00⟩ still, so X then a measurement reads One, surely.
    let (outcomes, end) = run_batch(
        &mut terminal,
        &[
            region([UnitaryOperation::X { target: 0 }]),
            measure(0, 0),
            measure_reset(0, 1),
            measure(0, 2),
        ],
    );
    end.expect("ran");
    assert_eq!(
        outcomes,
        [(0, Value::One), (1, Value::One), (2, Value::Zero)]
    );
    assert!(terminal.log_p().abs() < 1e-12);
    assert_eq!(terminal.capabilities().states, Some(1));
    assert_eq!(
        terminal.close(),
        TensorNetReport {
            batches: 1,
            regions: 1,
            operations: 1,
            operators: 1,
            measurements: 3,
            resets: 0,
            max_bond: 1,
        }
    );
    assert_eq!(terminal.call(&[]).err(), Some(Rejected::Closed));
}

#[test]
fn per_shot_draws_follow_the_batch_strategys_distribution() {
    // Ry-like state from H, T, H: P(One) = (1 − cos π/4)/2 on qubit 0.
    let operations = [h(0), UnitaryOperation::T { target: 0 }, h(0)];
    let p1 = (1.0 - std::f64::consts::FRAC_PI_4.cos()) / 2.0;
    let shots = ShotSet::range(1..=4000).expect("shots");
    let batch = [region(operations), measure(0, 0)];
    let strategy = per_shot(1, 2);
    let mut session = open(&strategy, 21, &shots).expect("opens");
    let (events, end) = submit(&mut session, &shots, &batch).expect("accepted");
    end.expect("ran");
    let ones = f64::from(count(events.iter().filter(|e| e.value == Value::One))) / 4000.0;
    assert!((ones - p1).abs() < 0.03, "{ones} against {p1}");
    let batched =
        crate::MpsBatchSimulator::new(Rc::new(ExactReference::new()), 1, 2).expect("a chain");
    let mut session = open(&batched, 21, &shots).expect("opens");
    let (events, end) = submit(&mut session, &shots, &batch).expect("accepted");
    end.expect("ran");
    let ones = f64::from(count(events.iter().filter(|e| e.value == Value::One))) / 4000.0;
    assert!((ones - p1).abs() < 0.03, "{ones} against {p1}");
}

/// H₂'s kind of program, scaled down to fit `ExactReference`: phase
/// estimation with `ICMP`-conditioned feedback, mid-circuit measurements and
/// resets, through the walk over `PerShot(TensorNetShots)`, and through the
/// legacy runtime on the dense CPU simulator. The samplers differ, so the
/// rows do not match shot for shot; both follow textbook phase estimation's
/// distribution and peak at the phase's nearest estimate, and each walked
/// shot's ln p is its estimate's probability (`per_shot_phase_estimation`).
#[test]
fn phase_estimation_with_feedback_follows_the_legacy_runtime_and_peaks_at_the_phase() {
    const SHOTS: u32 = 2000;
    let expected = phase_estimation_distribution(IPE_BITS, IPE_PHASE);
    assert!((expected.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    assert!((expected[5] - 0.8756).abs() < 1e-4, "{expected:?}");
    let walked = per_shot_phase_estimation(&MpsStack::new(ExactReference::new()), SHOTS, 1e-9);

    let program = iterative_phase_estimation(IPE_BITS, IPE_PHASE);
    let mut legacy = vec![0; expected.len()];
    for seed in 0..SHOTS {
        let mut sim = FullStateSimulator::new(3, IPE_BITS as usize, seed, Default::default());
        legacy[estimate(&run_shot(program.program(), &mut sim))] += 1;
    }
    let distance = total_variation(&legacy, &expected);
    assert!(
        distance < allowed_distance(SHOTS),
        "the legacy runtime: {legacy:?} against {expected:?}, distance {distance}"
    );
    assert_eq!(peak(&legacy), 5, "{legacy:?}");
    let shots = f64::from(SHOTS);
    let frequencies: Vec<f64> = legacy.iter().map(|&n| f64::from(n) / shots).collect();
    let distance = total_variation(&walked, &frequencies);
    assert!(
        distance < 2.0 * allowed_distance(SHOTS),
        "the walk {walked:?} against the legacy runtime {legacy:?}: distance {distance}"
    );
}
