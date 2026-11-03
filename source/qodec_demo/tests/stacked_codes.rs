//! c422 (one layer) and c4c6 (two layers stacked) on the chain
//! `static_driver() >> qodec layers >> coins`, against the Qodec runtime's
//! own stack over the same coins: the lowerer's examples, as the Qodec
//! prototype ran them.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use executor_protocol::{ExecutorReport, Needs, Operand, ShotId, ShotOutcome, static_driver};
use qodec::Qodec;
use qodec_demo::coin::CoinBackend;
use qodec_demo::coin::coin_device;
use qodec_demo::profiler::Profile;
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::{QodecLayer, Statuses};
use qodec_demo::recorder::{Sent, recorder};
use qodec_demo::stack::{StackRan, on_the_stack};
use qodec_runtime::Status;

use common::{
    CODE_SHOTS, SEED, bottom, c4c6, c4c6_program, c422, c422_program, each_status, shot_set, sorted,
};

/// The shots of every run here.
fn shots() -> Vec<ShotId> {
    CODE_SHOTS.collect()
}

/// The Qodec runtime's own stack on `shots`, over the coins of [`SEED`].
fn on_the_stack_of(qodec: &Qodec, program: &Program, shots: &[ShotId]) -> StackRan<CoinBackend> {
    on_the_stack(qodec, program, &shot_set(shots), SEED).expect("the Qodec stack's run")
}

/// What a run of c422's chain gave: its outcomes, its layer's statuses, and
/// what the coins ran.
fn c422_on_the_chain() -> (Vec<ShotOutcome>, Statuses, ExecutorReport<Profile>) {
    let qodec = c422();
    let layer = QodecLayer::new(&qodec, 0).expect("c422 plans");
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> layer.adapter() >> coins).expect("every boundary matches");
    let program = c422_program(&qodec).to_static(Needs::default());
    let (outcomes, (statuses, report)) = chain
        .run(program, shot_set(&shots()))
        .expect("the job fits the chain")
        .wait();
    (outcomes.expect("the run succeeds"), statuses, report)
}

/// What a run of c4c6's chain gave: its outcomes, each layer's statuses
/// (the top's, then the middle's), and what the coins ran.
type C4c6Ran = (
    Vec<ShotOutcome>,
    (Statuses, (Statuses, ExecutorReport<Profile>)),
);

/// c4c6's chain on `shots`, its two layers grouped as
/// `(static_driver() >> top >> middle) >> coins`.
fn c4c6_on_the_chain(shots: &[ShotId]) -> C4c6Ran {
    let qodec = c4c6();
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> top.adapter() >> middle.adapter() >> coins)
        .expect("every boundary matches");
    let program = c4c6_program(&qodec).to_static(Needs::default());
    let (outcomes, report) = chain
        .run(program, shot_set(shots))
        .expect("the job fits the chain")
        .wait();
    (outcomes.expect("the run succeeds"), report)
}

/// One layer: the chain gives, shot by shot, the outcomes of the Qodec
/// runtime's own stack over the same coins.
#[test]
fn c422_gives_the_qodec_stacks_outcomes() {
    let qodec = c422();
    let expected = on_the_stack_of(&qodec, &c422_program(&qodec), &shots());
    let (outcomes, ..) = c422_on_the_chain();

    assert!(!outcomes.is_empty(), "the program reads");
    assert_eq!(sorted(outcomes), sorted(expected.outcomes));
}

/// One layer: the coins run the bottom instructions of the Qodec runtime's
/// own stack, as many of each and in as many batches.
#[test]
fn c422_sends_the_qodec_stacks_bottom_instructions() {
    let qodec = c422();
    let expected = on_the_stack_of(&qodec, &c422_program(&qodec), &shots());
    let (.., report) = c422_on_the_chain();

    assert!(expected.ran().instructions() > 0, "the stack ran");
    assert_eq!(report.states, expected.ran());
}

/// One layer: the layer reports the statuses of the Qodec runtime's own
/// stack. Nothing lies below the layer but the coins, so no status is lost.
#[test]
fn c422_reports_the_qodec_stacks_statuses() {
    let qodec = c422();
    let expected = on_the_stack_of(&qodec, &c422_program(&qodec), &shots());
    let (_, statuses, _) = c422_on_the_chain();

    assert!(!expected.statuses.is_empty(), "the program reads");
    assert_eq!(each_status(statuses), expected.statuses);
}

/// Two layers stacked: the chain gives, shot by shot, the outcomes of the
/// Qodec runtime's own stack over the same coins.
#[test]
fn c4c6_gives_the_qodec_stacks_outcomes() {
    let qodec = c4c6();
    let expected = on_the_stack_of(&qodec, &c4c6_program(&qodec), &shots());
    let (outcomes, _) = c4c6_on_the_chain(&shots());

    assert!(!outcomes.is_empty(), "the program reads");
    assert_eq!(sorted(outcomes), sorted(expected.outcomes));
}

/// Two layers stacked: the coins run the bottom instructions of the Qodec
/// runtime's own stack, as many of each and in as many batches.
#[test]
fn c4c6_sends_the_qodec_stacks_bottom_instructions() {
    let qodec = c4c6();
    let expected = on_the_stack_of(&qodec, &c4c6_program(&qodec), &shots());
    let (_, (_, (_, report))) = c4c6_on_the_chain(&shots());

    assert!(expected.ran().instructions() > 0, "the stack ran");
    assert_eq!(report.states, expected.ran());
}

/// Two layers stacked: each shot runs the bottom batches of the Qodec
/// runtime's own stack, in the stack's order and operand for operand, the
/// corrections' bits (`Z_if`) included.
#[test]
fn c4c6_runs_the_qodec_stacks_bottom_batches_in_order() {
    let qodec = c4c6();
    let program = c4c6_program(&qodec);
    let stack = on_the_stack_of(&qodec, &program, &shots());
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> top.adapter() >> middle.adapter() >> recorder() >> coins)
        .expect("every boundary matches");
    let (outcomes, (_, (_, (sent, _)))): (_, (Statuses, (Statuses, (Sent, _)))) = chain
        .run(program.to_static(Needs::default()), shot_set(&shots()))
        .expect("the job fits the chain")
        .wait();
    outcomes.expect("the run succeeds");

    let expected = (stack.backends.iter())
        .map(|(shot, coins)| (*shot, coins.sent().to_vec()))
        .collect();
    assert!(!sent.0.is_empty(), "the shots ran");
    assert_eq!(sent, Sent(expected));
}

/// Shot 3 alone gives, through both layers, the outcomes it gives among
/// shots 0 to 3: each layer keeps a runtime per shot.
#[test]
fn a_shot_of_c4c6_gives_the_same_outcomes_alone() {
    let (among, _) = c4c6_on_the_chain(&shots());
    let (alone, _) = c4c6_on_the_chain(&[3]);

    let among: Vec<_> = sorted(among).into_iter().filter(|o| o.0 == 3).collect();
    assert!(!among.is_empty(), "shot 3 reads");
    assert_eq!(sorted(alone), among);
}

/// Each top result's status, with the tag of its first readout, shot by
/// shot, a shot's in the order of its results.
type EachStatus = Vec<(ShotId, u32, Status)>;

/// Each top result's status, as the chain's top layer reports it and as
/// the stack gives it.
fn top_statuses() -> (EachStatus, EachStatus) {
    let qodec = c4c6();
    let stack = on_the_stack_of(&qodec, &c4c6_program(&qodec), &shots());
    let (_, (top, _)) = c4c6_on_the_chain(&shots());
    (each_status(top), stack.statuses)
}

/// Two layers stacked, where the statuses part (QD5): the top
/// layer reports the stack's statuses, but for `unresolved`, which the
/// stack's top runtime takes from the runtime below it (each shot's
/// `measure_z_all`, tag 1), and the chain's top layer, given no status
/// from below, leaves clear.
#[test]
fn c4c6_top_statuses_lack_the_status_from_below() {
    let (chain, stack) = top_statuses();

    let resolved = |mut status: Status| {
        status.unresolved = false;
        status
    };
    let unresolved = |statuses: &EachStatus| -> Vec<_> {
        (statuses.iter())
            .filter(|(.., s)| s.unresolved)
            .map(|&(shot, tag, _)| (shot, tag))
            .collect()
    };
    assert_eq!(
        unresolved(&stack),
        shots().into_iter().map(|j| (j, 1)).collect::<Vec<_>>(),
        "precondition: the stack's top takes `unresolved` from below, at \
         each shot's tag 1"
    );
    assert_eq!(unresolved(&chain), []);
    let all_resolved = |s: EachStatus| -> Vec<_> {
        (s.into_iter())
            .map(|(shot, tag, st)| (shot, tag, resolved(st)))
            .collect()
    };
    assert_eq!(all_resolved(chain), all_resolved(stack));
}

/// Two layers stacked: the status the top layer lacks is in the middle
/// layer's report, beside the top's (QD5, (a) today): the middle
/// runtime's results are unresolved, on every shot.
#[test]
fn c4c6_middle_statuses_are_in_the_middle_layers_report() {
    let (_, (_, (middle, _))) = c4c6_on_the_chain(&shots());

    for shot in shots() {
        let of_shot = middle.0.get(&shot).expect("the shot ran");
        assert!(
            of_shot.iter().any(|(_, status)| status.unresolved),
            "shot {shot}: {of_shot:?}"
        );
    }
}

/// Whether each result of the middle layer is unresolved, by shot and in
/// order, on c4c6's chain run on the program of `lines`.
fn middle_unresolved(lines: &[&str]) -> BTreeMap<ShotId, Vec<bool>> {
    let qodec = c4c6();
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> top.adapter() >> middle.adapter() >> coins)
        .expect("every boundary matches");
    let program = Program::parse(&qodec, lines).to_static(Needs::default());
    let (outcomes, (_, (statuses, _))): (_, (Statuses, (Statuses, _))) = chain
        .run(program, shot_set(&shots()))
        .expect("the job fits the chain")
        .wait();
    outcomes.expect("the run succeeds");
    (statuses.0.into_iter())
        .map(|(shot, each)| (shot, each.iter().map(|(_, s)| s.unresolved).collect()))
        .collect()
}

/// Two layers stacked, c4c6's program run in three lengths: the middle
/// runtime raises `unresolved` on each shot during the idles, a call that
/// gives the driver no readout, not at `measure_z_all`; from the idles' first
/// result on, every result carries it, the measure's included, since a
/// status is sticky.
#[test]
fn c4c6_middle_raises_unresolved_in_the_idles_and_every_later_result_carries_it() {
    let idles = ["idle 0"; 10].join("; ");
    let prepared = middle_unresolved(&["prepare_z_all 0"]);
    let idled = middle_unresolved(&["prepare_z_all 0", &idles]);
    let measured = middle_unresolved(&["prepare_z_all 0", &idles, "measure_z_all 0"]);

    for shot in shots() {
        let (prepare, idle, all) = (&prepared[&shot], &idled[&shot], &measured[&shot]);
        assert!(
            prepare.len() < idle.len() && idle.len() < all.len(),
            "precondition: shot {shot}'s idles and measure give the middle results"
        );
        assert_eq!(
            all[..idle.len()],
            idle[..],
            "precondition: shot {shot}'s runs agree on what they share"
        );
        assert_eq!(
            idle[..prepare.len()],
            prepare[..],
            "precondition: shot {shot}'s runs agree on what they share"
        );
        let expected: Vec<bool> = (0..all.len()).map(|i| i >= prepare.len()).collect();
        assert_eq!(*all, expected, "shot {shot}");
    }
}

/// Two layers stacked: the middle layer reports a status for each result
/// of the calls it is sent, in their order, with the tag of the result's
/// first readout, though the top layer numbers its readouts below from 0
/// in each batch, so that later calls reuse earlier calls' tags.
#[test]
fn c4c6_middle_report_keeps_every_upper_results_status() {
    let qodec = c4c6();
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> top.adapter() >> recorder() >> middle.adapter() >> coins)
        .expect("every boundary matches");
    let program = c4c6_program(&qodec).to_static(Needs::default());
    let (outcomes, (_, (sent, (statuses, _)))): (_, (Statuses, (Sent, (Statuses, _)))) = chain
        .run(program, shot_set(&shots()))
        .expect("the job fits the chain")
        .wait();
    outcomes.expect("the run succeeds");

    for shot in shots() {
        let first_tags: Vec<u32> = (sent.0[&shot].iter().flatten())
            .filter_map(|call| {
                call.operands.iter().find_map(|operand| match *operand {
                    Operand::Tag(tag) => Some(tag),
                    _ => None,
                })
            })
            .collect();
        let distinct: BTreeSet<_> = first_tags.iter().collect();
        assert!(
            distinct.len() < first_tags.len(),
            "precondition: shot {shot}'s calls to the middle layer reuse tags"
        );
        let reported: Vec<u32> = statuses.0[&shot].iter().map(|&(tag, _)| tag).collect();
        assert_eq!(reported, first_tags, "shot {shot}");
    }
}

/// FINDING (QD5): what the stack gives, which the chain cannot until a
/// lower status has a way up the chain: QD5 (b), a status on each
/// outcome, planned.
#[test]
#[ignore = "finding QD5: a lower layer's status has no way up the chain"]
fn c4c6_top_statuses_equal_the_qodec_stacks() {
    let (chain, stack) = top_statuses();
    assert_eq!(chain, stack);
}
