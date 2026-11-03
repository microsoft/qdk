//! Repetition3 (the distance-3 bit-flip code, logical over `stim+rz`) on
//! the chain `static_driver() >> qodec >> simulator`, against the Qodec
//! runtime's own stack over the same noisy bits, and against the reference
//! values the crate pins.

mod common;

use executor_protocol::{
    Close, Executor, ExecutorReport, InstructionBatchRef, Needs, Operand, OperandKind,
    ReadoutFailure, ShotId, ShotOutcome, ShotSet, Value, shot_seed, static_driver, wait,
};
use qodec_demo::boundary::{self, Boundary};
use qodec_demo::coin::coin_device;
use qodec_demo::noisy_bits::{NoisyBitsBackend, simulator};
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::QodecLayer;
use qodec_demo::stack::{StackRan, on_the_stack, on_the_stack_over};
use qodec_runtime::Status;

use common::{
    DRAWS_FLIP, FLIP, SEED, SHOTS, each_status, repetition3, repetition3_program, shot_set, sorted,
    sorted_with_conditions, stim_rz,
};

/// What a run of the chain gave.
struct Ran {
    /// Each logical readout, marked with its result's status.
    outcomes: Vec<ShotOutcome>,
    /// The simulator's: each shot's log probability.
    report: ExecutorReport<()>,
}

/// The program on `shots` through the chain: the Qodec layer of
/// repetition3 over the noisy bits at `flip`.
fn on_the_chain(shots: &[ShotId], flip: f64) -> Ran {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let simulator = simulator(stim_rz().set, SEED, flip);
    let chain = (static_driver() >> layer.adapter() >> simulator).expect("every boundary matches");
    let program = repetition3_program(&qodec).to_static(Needs::default());
    let (outcomes, report) = chain
        .run(program, shot_set(shots))
        .expect("the job fits the chain")
        .wait();
    Ran {
        outcomes: outcomes.expect("the run succeeds"),
        report,
    }
}

/// The Qodec runtime's own stack on [`SHOTS`], over each shot's bits at
/// `flip`, drawn from the seed that the simulator gives the shot.
fn on_the_stack_at(flip: f64) -> StackRan<NoisyBitsBackend> {
    let qodec = repetition3();
    on_the_stack_over(
        &qodec,
        &repetition3_program(&qodec),
        &shot_set(&SHOTS),
        |shot| NoisyBitsBackend::new(shot_seed(SEED, shot), flip),
    )
    .expect("the Qodec stack's run")
}

/// The protocol's sets express both instruction sets of repetition3:
/// `idle`, which declares no action, takes its block alone; the physical
/// `M` takes its qubit and the tag of its readout.
#[test]
fn boundary_expresses_both_instruction_sets() {
    let qodec = repetition3();
    let [logical, physical] = qodec.layers() else {
        panic!("repetition3 has two layers");
    };
    let logical = Boundary::new(&logical.instruction_set).expect("the logical set");
    let physical = Boundary::new(&physical.instruction_set).expect("the physical set");

    use OperandKind::{Block, Real, Tag};
    let operands = |b: &Boundary, mnemonic: &str| b.set.operands(mnemonic).map(<[_]>::to_vec);
    assert_eq!(operands(&logical, "prepare_z"), Some(vec![Block]));
    assert_eq!(operands(&logical, "idle"), Some(vec![Block]));
    assert_eq!(operands(&logical, "rotate_z"), Some(vec![Block, Real]));
    assert_eq!(operands(&logical, "measure_z"), Some(vec![Block, Tag]));
    assert_eq!(operands(&physical, "R"), Some(vec![Block]));
    assert_eq!(operands(&physical, "M"), Some(vec![Block, Tag]));
    assert_eq!(operands(&physical, "CX"), Some(vec![Block, Block]));
    assert_eq!(operands(&physical, "rotate_z"), Some(vec![Block, Real]));
}

/// Qodec's `number` arguments may be written as integers: `theta=1` runs on
/// the chain as `theta=1.0` does, with the same outcomes, statuses and log
/// probabilities, though Qodec parses it as an integer and the protocol's
/// set declares a real there.
#[test]
fn a_number_written_as_an_integer_runs_as_its_real_spelling() {
    let qodec = repetition3();
    let run = |theta: &str| {
        let rotate = format!("rotate_z 0 theta={theta}");
        let program = Program::parse(&qodec, &["prepare_z 0", &rotate, "measure_z 0"]);
        let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
        let simulator = simulator(stim_rz().set, SEED, DRAWS_FLIP);
        let chain =
            (static_driver() >> layer.adapter() >> simulator).expect("every boundary matches");
        let (outcomes, report): (_, ExecutorReport<()>) = chain
            .run(program.to_static(Needs::default()), shot_set(&SHOTS))
            .expect("the job fits the chain")
            .wait();
        let log_p = SHOTS.map(|shot| report.log_p.get(shot));
        let outcomes = (outcomes.map(sorted_with_conditions)).map_err(|e| format!("{e:?}"));
        (outcomes, log_p)
    };
    let real = run("1.0");
    assert!(real.0.is_ok(), "precondition: theta=1.0 runs: {:?}", real.0);

    assert_eq!(run("1"), real);
}

/// The program prepares, idles, rotates and measures one logical qubit: on
/// the Qodec runtime's stack over coins, each shot gives one logical
/// readout, under the tag of `measure_z`.
#[test]
fn the_qodec_stack_gives_one_logical_readout_per_shot() {
    let qodec = repetition3();
    let program = repetition3_program(&qodec);
    let stack = on_the_stack(&qodec, &program, &shot_set(&SHOTS), SEED).expect("the stack runs");

    let mut read: Vec<_> = stack.outcomes.iter().map(|o| (o.shot, o.tag)).collect();
    read.sort_unstable();
    assert_eq!(read, SHOTS.map(|shot| (shot, 0)));
}

/// Without noise every logical readout is Zero: the program prepares |0>
/// and rotates about Z only, as the layering prototype's
/// `repetition3_with_idle_and_a_bound_rotation` reads on a state vector.
/// Here on the Qodec runtime's stack over each shot's noiseless bits, and
/// on the chain.
#[test]
fn without_noise_every_logical_readout_is_zero() {
    let zero = SHOTS.map(|shot| (shot, 0, Value::Zero)).to_vec();
    assert_eq!(sorted(on_the_stack_at(0.0).outcomes), zero, "on the stack");
    assert_eq!(
        sorted(on_the_chain(&SHOTS, 0.0).outcomes),
        zero,
        "on the chain"
    );
}

/// The chain gives, shot by shot, the outcomes of the Qodec runtime's own
/// stack over the same bits: per shot, the Qodec runtime over the shot's
/// `NoisyBitsBackend`, from the seed that the simulator gives the shot.
#[test]
fn the_chain_gives_the_qodec_stacks_outcomes() {
    let Ran { outcomes, .. } = on_the_chain(&SHOTS, FLIP);
    let expected = on_the_stack_at(FLIP);

    assert_eq!(outcomes.len(), SHOTS.len(), "one logical readout per shot");
    assert!(
        outcomes.iter().any(|o| o.value != outcomes[0].value),
        "the shots read different values: {outcomes:?}"
    );
    assert_eq!(sorted(outcomes), sorted(expected.outcomes));
}

/// The chain marks each logical readout, shot by shot, with the status the
/// Qodec runtime's own stack gives its result: what its decoder made of
/// the same physical readouts, here noisy enough that the statuses are not
/// all alike.
#[test]
fn the_chain_gives_the_qodec_stacks_statuses() {
    let Ran { outcomes, .. } = on_the_chain(&SHOTS, DRAWS_FLIP);
    let expected = on_the_stack_at(DRAWS_FLIP).statuses;

    let reported = each_status(&outcomes, &expected);
    assert!(
        reported.iter().any(|s| s.2 != reported[0].2),
        "the shots' statuses differ: {reported:?}"
    );
    assert_eq!(reported, expected);
}

/// A program that reads under tag 41 twice, in two calls, as a caller may
/// reuse a tag once its readout came up: each call's reply marks its
/// readout with its result's status, as the Qodec runtime's own stack
/// gives it. Its coins draw from a seed under which the second result is
/// inconsistent and the first is not, so the order shows.
#[test]
fn reused_readout_tags_keep_every_results_status() {
    const SEED_OF_DIFFERING_STATUSES: u64 = 0;
    let qodec = repetition3();
    let mut program = Program::parse(
        &qodec,
        &["prepare_z 0", "measure_z 0", "prepare_z 0", "measure_z 0"],
    );
    for call in program.batches.iter_mut().flatten() {
        call.operands = (call.operands.iter())
            .map(|operand| match operand {
                Operand::Tag(_) => Operand::Tag(41),
                other => *other,
            })
            .collect();
    }
    let shot = ShotSet::new(vec![2]).expect("one shot");
    let expected =
        on_the_stack(&qodec, &program, &shot, SEED_OF_DIFFERING_STATUSES).expect("the stack runs");
    assert_eq!(
        expected
            .statuses
            .iter()
            .map(|s| (s.0, s.1))
            .collect::<Vec<_>>(),
        [(2, 41), (2, 41)],
        "precondition: two results read under tag 41"
    );
    assert_ne!(
        expected.statuses[0].2, expected.statuses[1].2,
        "precondition: the two results' statuses differ"
    );
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let mut chain = (layer.adapter() >> coin_device(stim_rz().set, SEED_OF_DIFFERING_STATUSES))
        .expect("every boundary matches");
    let mut statuses = Vec::new();
    for instructions in &program.batches {
        let reply = chain
            .call(InstructionBatchRef {
                shots: &shot,
                instructions,
            })
            .expect("precondition: the layer accepts a call that reuses an earlier call's tag");
        let mut read = Vec::new();
        wait(reply, &mut read).expect("precondition: the call runs");
        statuses.extend((read.iter()).map(|o| (o.shot, o.tag, boundary::status(o.conditions))));
    }
    chain.close();

    assert_eq!(statuses, expected.statuses);
}

/// The chain's report gives each shot the log probability of its bits'
/// draws on the Qodec runtime's own stack: the chain sends the same
/// physical instructions, in the same order, and each shot draws from its
/// own seed. Noisy, so that each shot's differs.
#[test]
fn the_chain_reports_the_qodec_stacks_log_probabilities() {
    let Ran { report, .. } = on_the_chain(&SHOTS, DRAWS_FLIP);
    let expected = on_the_stack_at(DRAWS_FLIP);

    let stack: Vec<_> = expected.backends.iter().map(|(shot, _)| *shot).collect();
    let chain: Vec<_> = (0..=SHOTS[3] + 1)
        .filter(|&shot| report.log_p.get(shot).is_some())
        .collect();
    assert_eq!(chain, stack, "the same shots have one");
    for (shot, backend) in &expected.backends {
        let log_p = Some(backend.bits().log_p());
        assert_eq!(report.log_p.get(*shot), log_p, "shot {shot}");
    }
}

/// Shot 6 alone gives what it gives among 2, 5 and 11: its outcomes, its
/// statuses and its log probability, as it draws from its own seed. Noisy,
/// so that a draw from another seed would show.
#[test]
fn a_shot_gives_the_same_run_alone() {
    let among = on_the_chain(&SHOTS, DRAWS_FLIP);
    let alone = on_the_chain(&[6], DRAWS_FLIP);

    let of_6 = |ran: &Ran| {
        let outcomes = ran.outcomes.iter().filter(|o| o.shot == 6).copied();
        (
            sorted_with_conditions(outcomes.collect()),
            ran.report.log_p.get(6),
        )
    };
    let log_p = |shot| among.report.log_p.get(shot).expect("the bits know it");
    let others = [2, 5, 11].map(log_p);
    assert!(
        !others.contains(&log_p(6)),
        "shot 6 draws unlike the others"
    );
    let among = of_6(&among);
    assert!(!among.0.is_empty(), "shot 6 reads");
    assert_eq!(of_6(&alone), among);
}

/// A readout's status, never rejected (the program selects nothing).
fn status(inconsistent: bool, unresolved: bool) -> Status {
    Status {
        rejected: false,
        inconsistent,
        unresolved,
    }
}

/// `ran` gives, shot by shot over [`SHOTS`], `values` under tag 0, with
/// `statuses` and the log probabilities `log_p`, the latter within 1e-12.
fn assert_gives(ran: Ran, values: [Value; 4], statuses: [Status; 4], log_p: [f64; 4]) {
    let expected: Vec<_> = SHOTS.iter().zip(values).map(|(&s, v)| (s, 0, v)).collect();
    assert_eq!(sorted(ran.outcomes.clone()), expected);
    let expected: Vec<_> = (SHOTS.iter().zip(statuses))
        .map(|(&s, st)| (s, 0, st))
        .collect();
    assert_eq!(each_status(&ran.outcomes, &expected), expected);
    for (shot, expected) in SHOTS.into_iter().zip(log_p) {
        let log_p = ran.report.log_p.get(shot).expect("the bits know it");
        assert!((log_p - expected).abs() < 1e-12, "shot {shot}: {log_p}");
    }
}

/// The reference values at flip 0.01: shot 2 draws no flip and reads Zero;
/// shots 5, 6 and 11 draw one each, which the frame decoder detects but
/// does not correct.
#[test]
fn the_chain_gives_the_reference_values_at_low_noise() {
    let error = Value::Missing(ReadoutFailure::Other);
    // 33 draws a shot: 33 ln 0.99 without a flip, 32 ln 0.99 + ln 0.01
    // with one.
    let no_flip = -0.331_661_083_165_548_04;
    let one_flip = -4.926_780_933_300_13;
    assert_gives(
        on_the_chain(&SHOTS, FLIP),
        [Value::Zero, error, error, error],
        [
            status(false, false),
            status(true, false),
            status(true, true),
            status(true, true),
        ],
        [no_flip, one_flip, one_flip, one_flip],
    );
}

/// The reference values at flip 0.3: every shot draws several flips and
/// reads an error, and each shot's log probability differs.
#[test]
fn the_chain_gives_the_reference_values_when_noisy() {
    let error = Value::Missing(ReadoutFailure::Other);
    assert_gives(
        on_the_chain(&SHOTS, DRAWS_FLIP),
        [error; 4],
        [
            status(true, false),
            status(true, true),
            status(true, true),
            status(true, true),
        ],
        [
            -17.701_358_172_688_582,
            -18.548_656_033_075_787,
            -16.854_060_312_301_378,
            -20.243_251_753_850_192,
        ],
    );
}
