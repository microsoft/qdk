//! c422 (one layer) and c4c6 (two layers stacked) on the chain
//! `static_driver() >> qodec layers >> coins`, against the Qodec runtime's
//! own stack over the same coins: the lowerer's examples, as the Qodec
//! prototype ran them.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use executor_protocol::{
    AdapterError, Below, CallInput, ExecutorReport, InstructionCall, Needs, OnePerShot, Operand,
    Outcomes, Rejected, ShotContext, ShotId, ShotOutcome, adapter_fn, shot_seed, static_driver,
};
use qodec::Qodec;
use qodec_demo::boundary::status;
use qodec_demo::coin::CoinBackend;
use qodec_demo::coin::coin_device;
use qodec_demo::profiler::Profile;
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::QodecLayer;
use qodec_demo::recorder::{Sent, recorder};
use qodec_demo::stack::{StackRan, on_the_stack, on_the_stack_over};
use qodec_runtime::Status;

use common::devices::{Script, ScriptedBackend, scripted_device};
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

/// What a run of c422's chain gave: its outcomes, and what the coins ran.
fn c422_on_the_chain() -> (Vec<ShotOutcome>, ExecutorReport<Profile>) {
    let qodec = c422();
    let layer = QodecLayer::new(&qodec, 0).expect("c422 plans");
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> layer.adapter() >> coins).expect("every boundary matches");
    let program = c422_program(&qodec).to_static(Needs::default());
    let (outcomes, report) = chain
        .run(program, shot_set(&shots()))
        .expect("the job fits the chain")
        .wait();
    (outcomes.expect("the run succeeds"), report)
}

/// What a run of c4c6's chain gave: its outcomes, and what the coins ran.
type C4c6Ran = (Vec<ShotOutcome>, ExecutorReport<Profile>);

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

/// One layer: each result's status, as the conditions its outcomes carry,
/// is the Qodec runtime's own stack's.
#[test]
fn c422_gives_the_qodec_stacks_statuses() {
    let qodec = c422();
    let expected = on_the_stack_of(&qodec, &c422_program(&qodec), &shots());
    let (outcomes, _) = c422_on_the_chain();

    assert!(
        (expected.statuses.iter()).any(|(.., s)| *s != Status::default()),
        "precondition: the stack raises a status"
    );
    assert_eq!(
        each_status(&outcomes, &expected.statuses),
        expected.statuses
    );
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
    let (_, report) = c4c6_on_the_chain(&shots());

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
    let (outcomes, (sent, _)): (_, (Sent, _)) = chain
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

/// What c4c6's chain over `$device` gave each top result, as the conditions
/// its outcomes carry, and what the Qodec runtime's own stack gave it, over
/// `$backend_of` each shot. A macro, since the device's type cannot be
/// named in a function's bounds.
macro_rules! c4c6_statuses_over {
    ($device:expr, $backend_of:expr) => {{
        let qodec = c4c6();
        let program = c4c6_program(&qodec);
        let stack = on_the_stack_over(&qodec, &program, &shot_set(&shots()), $backend_of)
            .expect("the Qodec stack's run");
        let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
        let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
        let chain = (static_driver() >> top.adapter() >> middle.adapter() >> $device)
            .expect("every boundary matches");
        let (outcomes, _) = chain
            .run(program.to_static(Needs::default()), shot_set(&shots()))
            .expect("the job fits the chain")
            .wait();
        let outcomes = outcomes.expect("the run succeeds");
        (each_status(&outcomes, &stack.statuses), stack.statuses)
    }};
}

/// c4c6's top statuses over the coins of [`SEED`]
/// (`c4c6_statuses_over!`).
fn top_statuses() -> (EachStatus, EachStatus) {
    let below = bottom(&c4c6());
    let coins = coin_device(Arc::clone(&below.set), SEED);
    c4c6_statuses_over!(coins, |shot| {
        CoinBackend::new(shot_seed(SEED, shot), Arc::clone(&below))
    })
}

/// Two layers stacked: each top result's status is the stack's, the
/// `unresolved` the middle runtime raises included (each shot's
/// `measure_z_all`, tag 1): the middle layer marks its outcomes with it,
/// and the top layer's adapter adds it to the conditions of the outcomes
/// it gives, as the stack's top runtime takes it from the runtime below
/// into its results.
#[test]
fn c4c6_top_statuses_equal_the_qodec_stacks() {
    let (chain, stack) = top_statuses();

    let unresolved = (stack.iter())
        .filter(|(.., s)| s.unresolved)
        .map(|&(shot, tag, _)| (shot, tag));
    assert_eq!(
        unresolved.collect::<Vec<_>>(),
        shots().into_iter().map(|j| (j, 1)).collect::<Vec<_>>(),
        "precondition: the stack's top takes `unresolved` from below, at \
         each shot's tag 1"
    );
    assert_eq!(chain, stack);
}

/// Two layers stacked, over a device that reads every bit 0 but loses
/// each shot's first readout: the loss comes up as a value with none
/// (`Value::Missing`), which the top runtime takes as an unresolved bit,
/// so the chain gives the stack's statuses, result by result.
#[test]
fn a_lost_readout_reaches_c4c6s_top_as_a_value() {
    let below = bottom(&c4c6());
    let lose_the_first: Script = |_, n| (n != 0).then_some(false);
    let device = scripted_device(Arc::clone(&below.set), SEED, lose_the_first);
    let (chain, stack) = c4c6_statuses_over!(device, |shot| {
        ScriptedBackend::new(shot_seed(SEED, shot), Arc::clone(&below), lose_the_first)
    });

    let unresolved = Status {
        unresolved: true,
        ..Status::default()
    };
    assert_eq!(
        stack,
        (shots().into_iter())
            .flat_map(|shot| [(shot, 0, unresolved), (shot, 1, unresolved)])
            .collect::<Vec<_>>(),
        "precondition: the stack's top results are unresolved, and only that"
    );
    assert_eq!(chain, stack);
}

/// What an observer between c4c6's two layers saw of each shot: for each
/// instruction of the top layer's calls below that reads, in order, the
/// tag of its first readout, and the status that the outcome under that
/// tag, in the reply to its call, carries, if the reply has one.
type SeenBelowTheTop = BTreeMap<ShotId, Vec<(u32, Option<Status>)>>;

/// c4c6's chain on the program `to_program` makes of c4c6, with an
/// observer between its two layers that runs each call of the top layer
/// below, shot by shot, and keeps what it saw ([`SeenBelowTheTop`]).
fn seen_below_the_top(to_program: impl FnOnce(&Qodec) -> Program) -> SeenBelowTheTop {
    let qodec = c4c6();
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let set = Arc::clone(top.sent());
    let seen = Arc::new(Mutex::new(SeenBelowTheTop::new()));
    let log = Arc::clone(&seen);
    let observe = move |shot: &mut ShotId,
                        input: &CallInput<'_, InstructionCall>,
                        below: &mut Below<'_, InstructionCall>,
                        outcomes: &mut Outcomes<'_>| {
        let read = below.run(input.instructions())?;
        let mut log = log.lock().expect("no panic holds the log");
        let of_shot = log.entry(*shot).or_default();
        for call in input.instructions() {
            if set.layouts[call.opcode.index()].readouts == 0 {
                continue;
            }
            let first = (call.operands.iter())
                .find_map(|o| match *o {
                    Operand::Tag(t) => Some(t),
                    _ => None,
                })
                .expect("a call that reads names its tags");
            let on_it = read.iter().find(|o| o.tag == first);
            of_shot.push((first, on_it.map(|o| status(o.conditions))));
        }
        outcomes.extend(read);
        Ok::<_, AdapterError>(())
    };
    let observer =
        adapter_fn(|c: ShotContext| Ok::<_, Rejected>(c.shot()), observe).shots(OnePerShot);
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> top.adapter() >> observer >> middle.adapter() >> coins)
        .expect("every boundary matches");
    let program = to_program(&qodec).to_static(Needs::default());
    let (outcomes, _) = chain
        .run(program, shot_set(&shots()))
        .expect("the job fits the chain")
        .wait();
    outcomes.expect("the run succeeds");
    let seen = seen.lock().expect("no panic holds the log");
    seen.clone()
}

/// Two layers stacked: each result the middle layer gives a shot, one per
/// middle instruction that reads, has its status on the outcomes of its
/// reply, though the top layer numbers its readouts below from 0 in each
/// batch, so that a shot's later calls reuse its earlier calls' tags.
#[test]
fn the_middle_layer_gives_a_status_per_result_of_each_shot() {
    let seen = seen_below_the_top(c4c6_program);

    assert_eq!(
        seen.keys().copied().collect::<Vec<_>>(),
        shots(),
        "precondition: every shot reads below the top"
    );
    for (shot, results) in seen {
        let tags: BTreeSet<_> = results.iter().map(|(tag, _)| tag).collect();
        assert!(
            tags.len() < results.len(),
            "precondition: shot {shot}'s calls to the middle layer reuse tags"
        );
        let missing: Vec<_> = (results.iter().enumerate())
            .filter(|(_, (_, status))| status.is_none())
            .collect();
        assert_eq!(missing, [], "shot {shot}");
    }
}

/// Two layers stacked: the middle layer marks each shot's outcomes
/// `unresolved`, the status its runtime raises on every shot of c4c6.
#[test]
fn c4c6_middle_raises_unresolved_on_every_shot() {
    let seen = seen_below_the_top(c4c6_program);

    for shot in shots() {
        let of_shot = seen.get(&shot).expect("the shot ran");
        assert!(
            (of_shot.iter()).any(|(_, status)| status.is_some_and(|s| s.unresolved)),
            "shot {shot}: {of_shot:?}"
        );
    }
}

/// Whether each result of the middle layer is unresolved, by shot and in
/// order, on c4c6's chain run on the program of `lines`.
fn middle_unresolved(lines: &[&str]) -> BTreeMap<ShotId, Vec<bool>> {
    (seen_below_the_top(|qodec| Program::parse(qodec, lines)).into_iter())
        .map(|(shot, each)| {
            let unresolved = (each.iter())
                .map(|(_, s)| s.expect("each middle result has its status").unresolved)
                .collect();
            (shot, unresolved)
        })
        .collect()
}

/// Two layers stacked, c4c6's program run in three lengths: the middle
/// runtime raises `unresolved` on each shot during the idles, a call that
/// gives the driver no readout, not at `measure_z_all`; from the idles'
/// first result on, every result carries it, the measure's included, since
/// a status is sticky.
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
