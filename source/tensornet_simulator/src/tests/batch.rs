// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The batch strategy, `MpsBatchSimulator`, as a `ShotStrategy`: the
//! prototype's MPS7–MPS13 and MPS20, on the region ISA.

use std::rc::Rc;

use executor_protocol::{
    Close as _, Describe as _, Need, Needs, Rejected, RetryClass, ShotSet, ShotStrategy as _,
    Unmet, Value, open,
};
use tensornet::{MpsEngine, reference::ExactReference};

use super::{
    TestDoubleEngine, crossing_pairs, cx, h, measure, measure_reset, region, submit, values_of,
};
use crate::{MpsBatchReport, MpsBatchSimulator, sampler_seed};

fn batch(qubits: usize, max_bond: usize) -> MpsBatchSimulator<ExactReference> {
    MpsBatchSimulator::new(Rc::new(ExactReference::new()), qubits, max_bond).expect("a chain")
}

fn counting(qubits: usize) -> MpsBatchSimulator<TestDoubleEngine> {
    MpsBatchSimulator::new(Rc::new(TestDoubleEngine::counting()), qubits, 64).expect("a chain")
}

fn unmet(result: Result<impl Sized, Rejected>) -> Unmet {
    match result {
        Err(Rejected::Unmet(unmet)) => unmet,
        Err(other) => panic!("expected Unmet, got {other:?}"),
        Ok(_) => panic!("expected Unmet, got a session"),
    }
}

#[test]
fn mps7_mps12_mps20_needs_it_does_not_offer_are_unmet_at_open() {
    // It offers its qubits, and neither feedback, resets, replayable shots
    // nor probabilities: a caller that needs one is refused at open, before
    // a state is prepared, naming this strategy.
    let strategy = counting(4);
    let shots = ShotSet::range(1..=100).expect("shots");
    let capabilities = strategy.capabilities();
    assert_eq!(capabilities.qubits, Some(4));
    assert!(!capabilities.feedback && !capabilities.resets);
    assert!(!capabilities.replayable && !capabilities.probabilities);
    for (needs, need, needed, offered) in [
        (Needs::default().with_feedback(true), Need::Feedback, 1, 0),
        (Needs::default().with_resets(true), Need::Resets, 1, 0),
        (
            Needs::default().with_replayable(true),
            Need::Replayable,
            1,
            0,
        ),
        (
            Needs::default().with_probabilities(true),
            Need::Probabilities,
            1,
            0,
        ),
        (Needs::default().with_qubits(Some(5)), Need::Qubits, 5, 4),
    ] {
        let u = unmet(strategy.open(1, &shots, &needs));
        assert!(
            u.component.contains("MpsBatchSimulator<"),
            "{}",
            u.component
        );
        assert_eq!((u.need, u.needed, u.offered), (need, needed, offered));
        assert_eq!(Rejected::Unmet(u).retry(), RetryClass::Never);
    }
    assert!(
        strategy
            .open(1, &shots, &Needs::default().with_qubits(Some(4)))
            .is_ok()
    );
    assert_eq!(
        strategy.engine().prepared(),
        1,
        "only the admitted session prepared"
    );
}

#[test]
fn mps8_one_sample_from_the_sessions_seed() {
    // The same seed gives the same samples, another seed others; and the
    // samples are the engine's, for J's size, from the derived seed: a lone
    // shot draws row 0.
    let strategy = batch(2, 64);
    let bell = [region([h(0), cx(0, 1)]), measure(0, 0), measure(1, 1)];
    let shots = ShotSet::range(1..=200).expect("shots");
    let run = |seed, shots: &ShotSet| {
        let mut session = open(&strategy, seed, shots).expect("opens");
        let (events, end) = submit(&mut session, shots, &bell).expect("accepted");
        end.expect("ran");
        events
    };
    assert_eq!(run(3, &shots), run(3, &shots));
    assert_ne!(run(3, &shots), run(4, &shots));
    let events = run(3, &shots);
    let first = values_of(&events, 0);
    assert_eq!(first, values_of(&events, 1).into_iter().collect::<Vec<_>>());
    let ones = first.iter().filter(|(_, v)| *v == Value::One).count();
    assert!((60..=140).contains(&ones), "{ones} of 200");
    let engine = ExactReference::new();
    for seed in [0, 3, 99] {
        let one = ShotSet::one(17);
        let mut state = engine.prepare(strategy.capacity()).expect("prepares");
        for operation in [h(0), cx(0, 1)] {
            crate::lowering::apply_operation(&engine, &mut state, operation).expect("applies");
        }
        let row = engine
            .sample(&state, &[0, 1], 1, sampler_seed(seed).expect("in range"))
            .expect("samples");
        let events = run(seed, &one);
        assert_eq!(
            events
                .iter()
                .map(|e| (e.shot, e.tag, e.value))
                .collect::<Vec<_>>(),
            [(17, 0, Value::from(row[0])), (17, 1, Value::from(row[1]))]
        );
    }
}

#[test]
fn mps9_a_submission_is_on_all_of_j_and_ends_with_its_measurements() {
    let strategy = batch(2, 64);
    let shots = ShotSet::range(1..=10).expect("shots");
    let mut session = open(&strategy, 1, &shots).expect("opens");
    // Part of J would let the shots' states diverge: that is feedback.
    let part = ShotSet::range(1..=5).expect("shots");
    let u = unmet(submit(&mut session, &part, &[region([h(0)])]));
    assert!(u.component.contains("MpsBatchSession<"), "{}", u.component);
    assert_eq!((u.need, u.needed, u.offered), (Need::Feedback, 1, 0));
    assert_eq!(
        submit(&mut session, &ShotSet::range(1..=11).expect("shots"), &[]).err(),
        Some(Rejected::ShotNotInSession(11))
    );
    // A measurement before the end, a reset, a qubit beyond: Instruction,
    // at the instruction.
    for (batch, at, words) in [
        (vec![measure(0, 0), region([h(1)])], 0, "before the end"),
        (
            vec![region([h(0)]), executor_reset(1), measure(0, 0)],
            1,
            "reset is not supported",
        ),
        (vec![region([h(0)]), measure(2, 0)], 1, "beyond"),
        (vec![region([h(2)])], 0, "region 0"),
        (
            vec![region([h(0)]), measure_reset(0, 0), measure(0, 1)],
            2,
            "after MeasureResetZ",
        ),
        (vec![region([h(0)]), measure(0, usize::MAX)], 1, "tag"),
    ] {
        match submit(&mut session, &shots, &batch) {
            Err(Rejected::Instruction { at: found, reason }) => {
                assert_eq!(found, at, "{reason}");
                assert!(reason.contains(words), "{reason}");
            }
            other => panic!("{batch:?}: {other:?}"),
        }
    }
    // Nothing of a rejected batch ran: two submissions still make a Bell pair.
    let (events, end) = submit(&mut session, &shots, &[region([h(0)])]).expect("accepted");
    assert_eq!((events.len(), end), (0, Ok(())));
    let (events, end) = submit(
        &mut session,
        &shots,
        &[region([cx(0, 1)]), measure(0, 0), measure(1, 1)],
    )
    .expect("accepted");
    end.expect("ran");
    assert_eq!(values_of(&events, 0), values_of(&events, 1));
    // Once sampled, the shots' states are no longer one: feedback again.
    let u = unmet(submit(&mut session, &shots, &[region([h(0)])]));
    assert_eq!(u.need, Need::Feedback);
    let report = session.close();
    assert_eq!(report.submissions, 2);
    assert!(report.log_p.is_empty());
    assert_eq!(
        report.shots,
        MpsBatchReport {
            regions: 2,
            operations: 2,
            sampled_shots: 10,
            measurements: 20,
            max_bond: 2,
        }
    );
    assert_eq!(
        submit(&mut session, &shots, &[]).err(),
        Some(Rejected::Closed)
    );
    assert!(session.state().is_none());
}

fn executor_reset(qubit: usize) -> qdk_simulators::execution::RegionInstruction {
    qdk_simulators::execution::RegionInstruction::Reset { qubit }
}

#[test]
fn events_come_by_shot_in_the_order_j_was_opened_with() {
    let strategy = batch(2, 64);
    let opened = ShotSet::new(vec![7, 3, 5]).expect("shots");
    let mut session = open(&strategy, 1, &opened).expect("opens");
    let submitted = ShotSet::new(vec![3, 5, 7]).expect("shots");
    let (events, end) = submit(
        &mut session,
        &submitted,
        &[region([h(0)]), measure(0, 4), measure(1, 2)],
    )
    .expect("accepted");
    end.expect("ran");
    assert_eq!(
        events.iter().map(|e| (e.shot, e.tag)).collect::<Vec<_>>(),
        [(7, 4), (7, 2), (3, 4), (3, 2), (5, 4), (5, 2)]
    );
    assert!(values_of(&events, 2).iter().all(|(_, v)| *v == Value::Zero));
}

#[test]
fn a_repeated_qubit_reads_one_column() {
    // M0 sampled each qubit once and read its column for every measurement
    // of it; so does the session (the prototype split instead).
    let strategy = batch(2, 64);
    let shots = ShotSet::range(1..=50).expect("shots");
    let mut session = open(&strategy, 2, &shots).expect("opens");
    let (events, end) = submit(
        &mut session,
        &shots,
        &[region([h(0)]), measure(0, 0), measure(0, 1)],
    )
    .expect("accepted");
    end.expect("ran");
    assert_eq!(events.len(), 100);
    assert_eq!(values_of(&events, 0), values_of(&events, 1));
    let values: Vec<_> = values_of(&events, 0).into_iter().map(|(_, v)| v).collect();
    assert!(values.contains(&Value::One) && values.contains(&Value::Zero));
    assert_eq!(session.close().shots.measurements, 100);
}

#[test]
fn mps3_mps13_a_state_beyond_its_capacity_fails_the_session() {
    // The reference never truncates, so two pairs across the middle (bond 4)
    // outgrow χ = 2: the submission fails, then the session refuses.
    let strategy = batch(4, 2);
    let shots = ShotSet::range(1..=4).expect("shots");
    let mut session = open(&strategy, 1, &shots).expect("opens");
    let (events, end) = submit(
        &mut session,
        &shots,
        &[region(crossing_pairs()), measure(0, 0)],
    )
    .expect("accepted: the failure is the run's");
    assert!(events.is_empty());
    let reason = end.expect_err("fails");
    assert!(
        reason.contains("bond 4") && reason.contains("capacity"),
        "{reason}"
    );
    assert!(session.state().is_none());
    match submit(&mut session, &shots, &[measure(0, 0)]) {
        Err(Rejected::SessionFailed(why)) => assert_eq!(why, reason),
        other => panic!("{other:?}"),
    }
    // At χ = 4 it fits.
    let fits = batch(4, 4);
    let mut session = open(&fits, 1, &shots).expect("opens");
    let (_, end) = submit(
        &mut session,
        &shots,
        &[region(crossing_pairs()), measure(0, 0), measure(2, 1)],
    )
    .expect("accepted");
    end.expect("ran");
    assert_eq!(session.close().shots.max_bond, 4);
}

#[test]
fn mps4_an_engine_that_cannot_prepare_is_unavailable_at_open() {
    let strategy =
        MpsBatchSimulator::new(Rc::new(TestDoubleEngine::refusing()), 2, 8).expect("a chain");
    let shots = ShotSet::range(1..=3).expect("shots");
    match open(&strategy, 1, &shots) {
        Err(error @ Rejected::Unavailable(_)) => {
            assert!(error.to_string().contains("refuses to prepare"), "{error}");
            assert_eq!(error.retry(), RetryClass::Later);
        }
        Err(other) => panic!("{other:?}"),
        Ok(_) => panic!("opened"),
    }
}

#[test]
fn mps11_shot_sets_are_refused_before_any_state() {
    assert_eq!(ShotSet::new(Vec::new()).err(), Some(Rejected::NoShots));
    assert_eq!(
        ShotSet::new(vec![2, 1, 2]).err(),
        Some(Rejected::RepeatedShot(2))
    );
    let strategy = counting(2);
    let stack = crate::MpsStack::shared(Rc::clone(strategy.engine()));
    let program = super::bell();
    assert!(matches!(
        stack.sample(&program, 0, Some(1)),
        Err(crate::StackError::Rejected(Rejected::NoShots))
    ));
    assert_eq!(strategy.engine().prepared(), 0);
}

#[test]
fn a_shape_that_is_no_chain_is_refused_by_new() {
    assert!(MpsBatchSimulator::new(Rc::new(ExactReference::new()), 0, 4).is_err());
    assert!(MpsBatchSimulator::new(Rc::new(ExactReference::new()), 2, 0).is_err());
}
