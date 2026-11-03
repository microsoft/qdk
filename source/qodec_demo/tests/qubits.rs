//! Where a Qodec layer can refuse a program whose logical blocks the
//! qubits below cannot hold (QD4), on repetition3, whose
//! runtime counts 5 physical qubits for one logical block and 8 for two
//! (`LayerPlan::required_resources`). The runtime takes the qubits below
//! as its blocks need them, so what a program needs below is how many
//! blocks it holds at once, not the numbers it gives them.
//!
//! The program states the qubits it uses when its job is given
//! (`Needs::qubits`), not when the chain is built, and the chain checks
//! that need against what its top offers, before any call
//! (`Chain::run`). So the layer could refuse there, had it offered the
//! qubits of its own set whose blocks fit below (a qubit a block); the
//! protocol's adapter facility offers what is below unchanged (here the
//! 5 qubits), and gives no hook to change it (its `.offers(..)` is
//! planned). Until then the layer refuses at each call
//! ([`QodecLayer::within`]), before its runtime runs.

mod common;

use executor_protocol::{
    CallInput, Capabilities, ExecutorLogic, InstructionCall, MakeState, Needs, OnePerShot,
    OnePerShotExecutor, Operand, Outcome, Outcomes, Rejected, ShotContext, ShotOutcome,
    StaticProgramError, Value, executor_fn, static_driver,
};
use qodec::Qodec;
use qodec_demo::coin::coin_device;
use qodec_demo::noisy_bits::{QUBITS, simulator};
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::QodecLayer;
use qodec_demo::recorder::{Sent, recorder};

use common::{SEED, SHOTS, repetition3, shot_set, sorted, stim_rz};

/// Repetition3's program on logical block `block`: prepare, three syndrome
/// rounds around a rotation, measure.
fn repetition3_on(qodec: &Qodec, block: u32) -> Program {
    Program::parse(
        qodec,
        &[
            &format!("prepare_z {block}"),
            &format!("idle {block}; idle {block}; rotate_z {block} theta=0.7; idle {block}"),
            &format!("measure_z {block}"),
        ],
    )
}

/// Repetition3's blocks 0 and 1 at once: prepared, a syndrome round each,
/// measured. The Qodec runtime counts 8 qubits below for them.
fn two_blocks(qodec: &Qodec) -> Program {
    Program::parse(
        qodec,
        &[
            "prepare_z 0; prepare_z 1",
            "idle 0; idle 1",
            "measure_z 0; measure_z 1",
        ],
    )
}

/// What a run on the noisy bits (5 qubits) gave: the outcomes or the
/// error, and the batches each shot sent below the layer.
fn on_the_bits(
    layer: QodecLayer,
    program: &Program,
    needs: Needs,
) -> (Result<Vec<ShotOutcome>, StaticProgramError>, Sent) {
    let bits = simulator(stim_rz().set, SEED, 0.0);
    let chain =
        (static_driver() >> layer.adapter() >> recorder() >> bits).expect("every boundary matches");
    let (outcomes, (sent, _)) = chain
        .run(program.to_static(needs), shot_set(&SHOTS))
        .expect("the job fits the chain")
        .wait();
    (outcomes, sent)
}

/// What a run on coins, which hold any number of qubits, gave: the outcomes
/// or the error.
fn on_coins(layer: QodecLayer, program: &Program) -> Result<Vec<ShotOutcome>, StaticProgramError> {
    let coins = coin_device(stim_rz().set, SEED);
    let chain = (static_driver() >> layer.adapter() >> coins).expect("every boundary matches");
    let (outcomes, _) = chain
        .run(program.to_static(Needs::default()), shot_set(&SHOTS))
        .expect("the job fits the chain")
        .wait();
    outcomes
}

/// Whether `error` is a refusal of an instruction of the program's batch
/// `batch`, made before that batch ran (a failure would be one the layer's
/// logic made after its runtime ran), naming `block` beyond the `blocks`
/// the qubits below hold.
fn is_refused_at_block(error: &StaticProgramError, batch: usize, block: u32, blocks: u32) -> bool {
    let named = format!("block {block} is beyond the {blocks} the qubits below hold");
    matches!(
        error,
        StaticProgramError::Refused {
            batch: b,
            rejected: Rejected::Instruction { reason, .. },
        } if *b == batch && *reason == named
    )
}

/// Within the noisy bits' qubits, the layer runs a program on block 0,
/// which they hold, as it does unbounded.
#[test]
fn within_the_qubits_below_a_held_block_runs() {
    let qodec = repetition3();
    let program = repetition3_on(&qodec, 0);
    let layer = || QodecLayer::new(&qodec, 0).expect("repetition3 plans");

    let (within, _) = on_the_bits(layer().within(QUBITS), &program, Needs::default());
    let (unbounded, _) = on_the_bits(layer(), &program, Needs::default());
    let within = within.expect("block 0 runs within 5 qubits");
    assert_eq!(
        sorted(within),
        sorted(unbounded.expect("the unbounded run"))
    );
}

/// Within the noisy bits' qubits, which hold one block, the layer refuses
/// a call on block 1, naming it: its check refuses it, before the runtime
/// runs.
#[test]
fn within_the_qubits_below_a_call_on_a_block_beyond_them_is_refused() {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");

    let program = repetition3_on(&qodec, 1);
    let (outcomes, _) = on_the_bits(layer.within(QUBITS), &program, Needs::default());
    let error = outcomes.expect_err("block 1 is refused");
    assert!(is_refused_at_block(&error, 0, 1, 1), "{error:?}");
}

/// Within the noisy bits' qubits, which hold one block, the layer refuses a
/// later call on block 1, once calls on block 0 ran: it checks each call,
/// before the runtime runs, so the batch is refused, not failed below.
#[test]
fn within_the_qubits_below_a_later_call_on_a_block_beyond_them_is_refused() {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let program = Program::parse(&qodec, &["prepare_z 0", "idle 0", "prepare_z 1"]);

    let (outcomes, _) = on_the_bits(layer.within(QUBITS), &program, Needs::default());
    let error = outcomes.expect_err("block 1 is refused");
    assert!(is_refused_at_block(&error, 2, 1, 1), "{error:?}");
}

/// The layer counts what its blocks need below by its runtime's count: two
/// blocks need 8 qubits, so within 8 a call on blocks 0 and 1 runs, and
/// within 7 it is refused.
#[test]
fn within_counts_the_qubits_blocks_need_by_the_runtimes_count() {
    let qodec = repetition3();
    let program = two_blocks(&qodec);
    let layer = || QodecLayer::new(&qodec, 0).expect("repetition3 plans");

    on_coins(layer().within(8), &program).expect("within 8 qubits, two blocks run");
    let error = on_coins(layer().within(7), &program).expect_err("within 7, block 1 is refused");
    assert!(is_refused_at_block(&error, 0, 1, 1), "{error:?}");
}

/// Within 3 qubits, which hold no block of repetition3 (one needs 5), the
/// layer refuses a call on block 0.
#[test]
fn within_too_few_qubits_for_any_block_a_call_on_block_0_is_refused() {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");

    let error = on_coins(layer.within(3), &repetition3_on(&qodec, 0)).expect_err("block 0");
    assert!(is_refused_at_block(&error, 0, 0, 0), "{error:?}");
}

/// Unbounded, the layer lowers blocks 0 and 1 to 8 qubits, which the noisy
/// bits do not hold: the run fails below, in the bits.
#[test]
fn unbounded_blocks_beyond_the_qubits_below_fail_below() {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");

    let (outcomes, _) = on_the_bits(layer, &two_blocks(&qodec), Needs::default());
    let error = outcomes.expect_err("two blocks fail below").to_string();
    assert!(error.contains("is not one of the 5 bits"), "{error}");
}

/// The Qodec runtime takes the qubits below as its blocks need them, the
/// lowest free first, whatever the blocks' numbers: block 1 alone runs on
/// the noisy bits' 5 qubits, as block 0 does. So what a program needs
/// below is how many blocks it holds at once, which the runtime alone
/// tracks, not the numbers it gives them; [`QodecLayer::within`], which
/// goes by the numbers, refuses block 1 alone.
#[test]
fn unbounded_a_lone_block_runs_on_the_lowest_qubits_below() {
    let qodec = repetition3();
    let layer = || QodecLayer::new(&qodec, 0).expect("repetition3 plans");

    let (one, sent_one) = on_the_bits(layer(), &repetition3_on(&qodec, 1), Needs::default());
    let (zero, sent_zero) = on_the_bits(layer(), &repetition3_on(&qodec, 0), Needs::default());
    let one = one.expect("block 1 alone runs on 5 qubits");
    assert_eq!(sorted(one), sorted(zero.expect("block 0 runs")));
    assert_eq!(sent_one, sent_zero, "the same qubits below");
}

/// A job on blocks 0 and 1 that states 2 qubits is accepted when given
/// over the noisy bits, whose 5 qubits hold one block: the layer offers
/// upstream the 5 qubits below unchanged, so it refuses no job for its
/// blocks.
#[test]
fn a_job_needing_more_blocks_than_fit_below_is_accepted() {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let bits = simulator(stim_rz().set, SEED, 0.0);
    let chain = (static_driver() >> layer.adapter() >> bits).expect("every boundary matches");
    let program = two_blocks(&qodec).to_static(Needs::default().with_qubits(2));

    assert!(chain.run(program, shot_set(&SHOTS)).is_ok());
}

/// FINDING (QD4): the layer could offer upstream the qubits of its own set
/// whose blocks fit below (1, as one block fits in 5 qubits by its
/// runtime's count), so that a job on blocks 0 and 1 stating 2 qubits is
/// refused when it is given, before any call, as an unmet need. The
/// adapter facility's `.offers(..)` is planned for this. Such an offer
/// goes by the blocks' numbers, as [`QodecLayer::within`] does, so it is
/// sound but not complete. The Qodec prototype cleared the offer instead
/// (no bound, so no job was refused for its blocks), since the runtime
/// counts what a program needs below from the blocks it holds at once.
#[test]
#[ignore = "finding QD4: the adapter facility offers the qubits below unchanged"]
fn a_job_needing_more_blocks_than_fit_below_is_refused_when_given() {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let bits = simulator(stim_rz().set, SEED, 0.0);
    let chain = (static_driver() >> layer.adapter() >> bits).expect("every boundary matches");
    let program = two_blocks(&qodec).to_static(Needs::default().with_qubits(2));

    let Err(faults) = chain.run(program, shot_set(&SHOTS)) else {
        panic!("a job on 2 blocks stating 2 qubits is refused over 5 qubits");
    };
    assert!(
        matches!(&faults.0[..], [Rejected::Unmet(unmet)] if unmet.needed == 2 && unmet.offered == 1),
        "{faults}"
    );
}

/// An executor of `stim+rz` that answers Zero to every readout and keeps no
/// state between calls: it offers no resumable shots, and states so.
fn without_resumable_shots() -> OnePerShotExecutor<
    InstructionCall,
    (),
    impl MakeState<()>,
    impl ExecutorLogic<InstructionCall, ()>,
> {
    let set = stim_rz().set;
    let promise = Capabilities::default()
        .with_instruction_set(Some(set))
        .with_resumable_shots(false);
    let zeros =
        |_: &mut (), input: &CallInput<'_, InstructionCall>, outcomes: &mut Outcomes<'_>| {
            for call in input.instructions() {
                for operand in call.operands.iter() {
                    if let Operand::Tag(tag) = *operand {
                        outcomes.push(Outcome::new(tag, Value::Zero));
                    }
                }
            }
            Ok::<_, String>(())
        };
    executor_fn(promise, |_: ShotContext| Ok(()), zeros).shots(OnePerShot)
}

/// The layer builds over an executor without resumable shots, though its
/// runtime sends a shot several batches, reading between them: it needs
/// nothing of what is below.
#[test]
fn a_layer_over_an_executor_without_resumable_shots_builds() {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    assert!((static_driver() >> layer.adapter() >> without_resumable_shots()).is_ok());
}

/// FINDING (QD4, the Qodec prototype's
/// `a_layer_over_a_device_without_feedback_does_not_build`, feedback being
/// the protocol's former name for resumable shots): the layer needs
/// resumable shots below, and a chain whose executor lacks them should not
/// build. The adapter facility's `.needs(..)` is planned for this.
#[test]
#[ignore = "finding QD4: the adapter facility needs nothing of what is below"]
fn a_layer_over_an_executor_without_resumable_shots_does_not_build() {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    assert!((static_driver() >> layer.adapter() >> without_resumable_shots()).is_err());
}
